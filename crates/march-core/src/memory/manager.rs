use super::*;

use super::search::{
    calculate_frequency_score, calculate_path_match_score, calculate_recency_score,
    collect_open_path_segments, query_tokens, tokenize_text, topic_warnings_for_entries,
};
use super::storage::{
    MemorySourceRevision, capture_source_revision, infer_level, initialize_global_schema,
    load_global_memories, load_project_memories, normalize_scope, normalize_stable_id,
    normalize_tags, parse_global_numeric_id, persist_global_memory, persist_project_memory,
    validate_record,
};

pub struct MemoryManager {
    project_memory_dir: PathBuf,
    global_db_path: PathBuf,
    project_memories: IndexMap<String, MemoryRecord>,
    global_memories: IndexMap<String, MemoryRecord>,
    source_revision: Option<MemorySourceRevision>,
    fts_connection: Mutex<Option<Connection>>,
    dirty_usage_updates: HashSet<String>,
}

impl MemoryManager {
    pub fn load(project_root: impl AsRef<Path>) -> Result<Self> {
        let project_memory_dir = project_root.as_ref().join(PROJECT_MEMORY_DIR);
        fs::create_dir_all(&project_memory_dir)
            .with_context(|| format!("failed to create {}", project_memory_dir.display()))?;

        let settings_dir = march_settings_dir()?;
        fs::create_dir_all(&settings_dir)
            .with_context(|| format!("failed to create {}", settings_dir.display()))?;
        let global_db_path = settings_dir.join("settings.db");
        let mut manager = Self {
            project_memory_dir,
            global_db_path,
            project_memories: IndexMap::new(),
            global_memories: IndexMap::new(),
            source_revision: None,
            fts_connection: Mutex::new(None),
            dirty_usage_updates: HashSet::new(),
        };
        initialize_global_schema(&manager.global_db_path)?;
        manager.reload()?;
        Ok(manager)
    }

    pub fn reload(&mut self) -> Result<()> {
        let revision = capture_source_revision(&self.project_memory_dir, &self.global_db_path)?;
        if self.source_revision.as_ref() == Some(&revision) {
            return Ok(());
        }
        let pending_usage = self
            .dirty_usage_updates
            .iter()
            .filter_map(|id| self.memory_by_prefixed_id(id).cloned())
            .collect::<Vec<_>>();
        self.project_memories = load_project_memories(&self.project_memory_dir)?;
        self.global_memories = load_global_memories(&self.global_db_path)?;
        for memory in pending_usage {
            self.update_loaded_memory(&memory);
        }
        self.rebuild_fts_index()?;
        self.source_revision = Some(revision);
        Ok(())
    }

    pub fn list_visible(&mut self, active_agent: &str) -> Result<Vec<MemoryRecord>> {
        self.reload()?;
        let mut memories = self
            .visible_memories(active_agent)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        memories.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.prefixed_id().cmp(&right.prefixed_id()))
        });
        Ok(memories)
    }

    pub fn search(&mut self, query: &MemoryQuery, limit: usize) -> Result<MemoryIndexView> {
        self.reload()?;

        if query.context_pressure_percent.unwrap_or_default() > LOW_CONTEXT_PRESSURE_SKIP_THRESHOLD
        {
            return Ok(MemoryIndexView::default());
        }

        let visible = self
            .visible_memories(&query.active_agent)
            .into_iter()
            .collect::<Vec<_>>();
        if visible.is_empty() {
            return Ok(MemoryIndexView::default());
        }

        let limited = limit.max(1);
        let total_count = visible.len();
        let query_tokens = query_tokens(query);
        let mut bm25_scores = if total_count <= 50 || query_tokens.is_empty() {
            IndexMap::new()
        } else {
            self.fts_scores(&query_tokens)?
        };

        let max_access_count = visible
            .iter()
            .map(|memory| memory.access_count)
            .max()
            .unwrap_or(0);
        let open_path_segments = collect_open_path_segments(&query.open_file_paths);

        let mut scored = visible
            .into_iter()
            .map(|memory| {
                let prefixed_id = memory.prefixed_id();
                let bm25_score = bm25_scores.shift_remove(&prefixed_id).unwrap_or(0.0);
                let path_match_score = calculate_path_match_score(memory, &open_path_segments);
                let recency_score = calculate_recency_score(memory.updated_at);
                let frequency_score =
                    calculate_frequency_score(memory.access_count, max_access_count);
                let skip_penalty = if memory.skip_count > 10 { 0.5 } else { 1.0 };
                let score = if total_count <= 50 {
                    (0.35
                        * if query_tokens.is_empty() {
                            1.0
                        } else {
                            bm25_score.max(0.3)
                        })
                        + (0.25 * path_match_score)
                        + (0.25 * recency_score)
                        + (0.15 * frequency_score)
                } else {
                    (0.5 * bm25_score)
                        + (0.25 * path_match_score)
                        + (0.15 * recency_score)
                        + (0.10 * frequency_score)
                } * skip_penalty;

                (memory.prefixed_id(), score)
            })
            .collect::<Vec<_>>();

        scored.sort_by(|left, right| {
            right
                .1
                .partial_cmp(&left.1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let topic_warnings = topic_warnings_for_entries(
            scored.iter().map(|(id, _)| id.as_str()),
            &self.project_memories,
            &self.global_memories,
        );
        let selected_ids = scored
            .into_iter()
            .filter(|(_, score)| total_count <= 50 || *score > 0.0)
            .take(if total_count <= 50 {
                total_count
            } else {
                limited
            })
            .collect::<Vec<_>>();

        if selected_ids.is_empty() {
            return Ok(MemoryIndexView::default());
        }

        let selected_id_set = selected_ids
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<HashSet<_>>();
        self.increment_skip_counts(&selected_id_set);

        let entries = selected_ids
            .into_iter()
            .filter_map(|(id, score)| {
                self.memory_by_prefixed_id(&id)
                    .map(|memory| (memory, score))
            })
            .map(|(memory, score)| MemoryIndexEntry {
                id: memory.prefixed_id(),
                memory_type: memory.memory_type.clone(),
                topic: memory.topic.clone(),
                title: memory.title.clone(),
                level: memory.level,
                score,
            })
            .collect::<Vec<_>>();

        Ok(MemoryIndexView {
            entries,
            topic_warnings,
        })
    }

    pub fn recall(&mut self, id: &str, active_agent: &str) -> Result<MemoryRecord> {
        let mut memory = self.peek(id, active_agent)?;
        memory.access_count = memory.access_count.saturating_add(1);
        memory.skip_count = 0;
        self.update_loaded_memory(&memory);
        self.mark_usage_dirty(memory.prefixed_id());
        Ok(self
            .memory_by_prefixed_id(&memory.prefixed_id())
            .cloned()
            .or_else(|| self.memory_by_raw_id(&memory.id).cloned())
            .or_else(|| self.memory_by_stable_id(&memory.stable_id).cloned())
            .unwrap_or(memory))
    }

    pub fn peek(&mut self, id: &str, active_agent: &str) -> Result<MemoryRecord> {
        self.reload()?;
        let memory = self
            .memory_by_prefixed_id(id)
            .cloned()
            .or_else(|| self.memory_by_raw_id(id).cloned())
            .or_else(|| self.memory_by_stable_id(id).cloned())
            .ok_or_else(|| anyhow!("memory {} not found", id))?;
        if !memory.scope.is_visible_to(active_agent) {
            bail!("memory {} is not visible to agent {}", id, active_agent);
        }
        Ok(memory)
    }

    pub fn memorize(
        &mut self,
        request: MemorizeRequest,
        active_agent: &str,
    ) -> Result<MemoryRecord> {
        self.flush_pending_usage_updates()?;
        let level = infer_level(request.level.as_deref(), &request.memory_type)?;
        let scope = normalize_scope(request.scope.as_deref(), active_agent);
        let now = SystemTime::now();

        let mut record = match level {
            MemoryLevel::Project => self
                .project_memories
                .get(request.id.trim())
                .cloned()
                .unwrap_or_else(|| MemoryRecord {
                    id: request.id.trim().to_string(),
                    level,
                    scope: scope.clone(),
                    stable_id: request.id.trim().to_string(),
                    memory_type: request.memory_type.trim().to_string(),
                    topic: request.topic.trim().to_string(),
                    title: request.title.trim().to_string(),
                    content: request.content.trim().to_string(),
                    tags: normalize_tags(request.tags.clone()),
                    access_count: 0,
                    skip_count: 0,
                    created_at: now,
                    updated_at: now,
                }),
            MemoryLevel::Global => self
                .lookup_existing_global_memory(&request.id)?
                .unwrap_or_else(|| MemoryRecord {
                    id: request.id.trim().to_string(),
                    level,
                    scope: scope.clone(),
                    stable_id: request.id.trim().to_string(),
                    memory_type: request.memory_type.trim().to_string(),
                    topic: request.topic.trim().to_string(),
                    title: request.title.trim().to_string(),
                    content: request.content.trim().to_string(),
                    tags: normalize_tags(request.tags.clone()),
                    access_count: 0,
                    skip_count: 0,
                    created_at: now,
                    updated_at: now,
                }),
        };

        record.scope = scope;
        record.stable_id = normalize_stable_id(level, request.id.trim());
        record.memory_type = request.memory_type.trim().to_string();
        record.topic = request.topic.trim().to_string();
        record.title = request.title.trim().to_string();
        record.content = request.content.trim().to_string();
        record.tags = normalize_tags(request.tags);
        record.updated_at = now;

        validate_record(&record)?;
        self.persist_memory(&record)?;
        self.source_revision = None;
        self.reload()?;
        Ok(self
            .memory_by_prefixed_id(&record.prefixed_id())
            .cloned()
            .or_else(|| self.memory_by_stable_id(&record.stable_id).cloned())
            .unwrap_or(record))
    }

    pub fn update_memory(&mut self, request: UpdateMemoryRequest) -> Result<MemoryRecord> {
        self.reload()?;
        let mut memory = self
            .memory_by_prefixed_id(&request.id)
            .cloned()
            .or_else(|| self.memory_by_raw_id(&request.id).cloned())
            .or_else(|| self.memory_by_stable_id(&request.id).cloned())
            .ok_or_else(|| anyhow!("memory {} not found", request.id))?;

        if let Some(title) = request.title {
            memory.title = title.trim().to_string();
        }
        if let Some(content) = request.content {
            memory.content = content.trim().to_string();
        }
        if let Some(topic) = request.topic {
            memory.topic = topic.trim().to_string();
        }
        if let Some(memory_type) = request.memory_type {
            memory.memory_type = memory_type.trim().to_string();
        }
        if let Some(tags) = request.tags {
            memory.tags = normalize_tags(tags);
        }
        memory.updated_at = SystemTime::now();

        validate_record(&memory)?;
        self.persist_memory(&memory)?;
        self.source_revision = None;
        self.reload()?;
        Ok(memory)
    }

    pub fn forget(&mut self, id: &str) -> Result<()> {
        self.reload()?;
        let memory = self
            .memory_by_prefixed_id(id)
            .cloned()
            .or_else(|| self.memory_by_raw_id(id).cloned())
            .or_else(|| self.memory_by_stable_id(id).cloned())
            .ok_or_else(|| anyhow!("memory {} not found", id))?;
        match memory.level {
            MemoryLevel::Project => {
                let path = self.project_memory_dir.join(format!("{}.md", memory.id));
                if path.exists() {
                    fs::remove_file(&path)
                        .with_context(|| format!("failed to remove {}", path.display()))?;
                }
            }
            MemoryLevel::Global => {
                let connection = self.open_global_connection()?;
                connection
                    .execute(
                        "DELETE FROM memories WHERE id = ?1",
                        params![parse_global_numeric_id(&memory.id)?],
                    )
                    .context("failed to delete global memory")?;
            }
        }
        self.source_revision = None;
        self.reload()
    }

    pub fn reassign_scope_from_agent(&mut self, agent_name: &str) -> Result<usize> {
        self.reload()?;
        let target = agent_name.trim();
        if target.is_empty() {
            return Ok(0);
        }

        let mut changed = 0usize;
        let memories = self
            .project_memories
            .values()
            .chain(self.global_memories.values())
            .cloned()
            .collect::<Vec<_>>();
        for mut memory in memories {
            let should_change = matches!(
                &memory.scope,
                MemoryScope::Agent(name) if name.eq_ignore_ascii_case(target)
            );
            if !should_change {
                continue;
            }
            memory.scope = MemoryScope::Shared;
            memory.updated_at = SystemTime::now();
            self.persist_memory(&memory)?;
            changed += 1;
        }
        if changed > 0 {
            self.source_revision = None;
            self.reload()?;
        }
        Ok(changed)
    }

    pub fn flush_pending_usage_updates(&mut self) -> Result<()> {
        if self.dirty_usage_updates.is_empty() {
            return Ok(());
        }

        let dirty_ids = self.dirty_usage_updates.drain().collect::<Vec<_>>();
        for id in dirty_ids {
            if let Some(memory) = self.memory_by_prefixed_id(&id).cloned() {
                self.persist_memory(&memory)?;
            }
        }

        self.source_revision = None;
        Ok(())
    }

    fn visible_memories(&self, active_agent: &str) -> Vec<&MemoryRecord> {
        self.project_memories
            .values()
            .chain(self.global_memories.values())
            .filter(|memory| memory.scope.is_visible_to(active_agent))
            .collect()
    }

    fn lookup_existing_global_memory(&self, id: &str) -> Result<Option<MemoryRecord>> {
        let trimmed = id.trim();
        if let Some(memory) = self.memory_by_stable_id(trimmed) {
            return Ok(Some(memory.clone()));
        }

        let Ok(numeric_id) = trimmed.parse::<i64>() else {
            return Ok(None);
        };
        Ok(self.global_memories.get(&numeric_id.to_string()).cloned())
    }

    fn fts_scores(&self, tokens: &[String]) -> Result<IndexMap<String, f32>> {
        let query = tokens
            .iter()
            .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        if query.trim().is_empty() {
            return Ok(IndexMap::new());
        }

        let guard = self
            .fts_connection
            .lock()
            .map_err(|_| anyhow!("failed to lock memory fts connection"))?;
        let Some(connection) = guard.as_ref() else {
            return Ok(IndexMap::new());
        };

        let mut statement = connection
            .prepare(
                "SELECT memory_id, bm25(memory_fts, 8.0, 1.0, 4.0, 2.0) AS rank
                 FROM memory_fts
                 WHERE memory_fts MATCH ?1
                 ORDER BY rank
                 LIMIT 50",
            )
            .context("failed to prepare memory fts query")?;
        let rows = statement
            .query_map(params![query], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })
            .context("failed to execute memory fts query")?;

        let mut scores = IndexMap::new();
        for row in rows {
            let (id, rank) = row.context("failed to decode memory fts row")?;
            let normalized = 1.0 / (1.0 + rank.abs() as f32);
            scores.insert(id, normalized.clamp(0.0, 1.0));
        }
        Ok(scores)
    }

    fn increment_skip_counts(&mut self, ids: &HashSet<String>) {
        for id in ids {
            if let Some(mut memory) = self.memory_by_prefixed_id(id).cloned() {
                memory.skip_count = memory.skip_count.saturating_add(1);
                self.update_loaded_memory(&memory);
                self.mark_usage_dirty(id.clone());
            }
        }
    }

    fn memory_by_prefixed_id(&self, id: &str) -> Option<&MemoryRecord> {
        let trimmed = id.trim();
        if let Some(raw_id) = trimmed.strip_prefix("p:") {
            return self.project_memories.get(raw_id);
        }
        if let Some(raw_id) = trimmed.strip_prefix("g:") {
            return self.global_memories.get(raw_id);
        }
        None
    }

    fn memory_by_raw_id(&self, id: &str) -> Option<&MemoryRecord> {
        self.project_memories
            .get(id.trim())
            .or_else(|| self.global_memories.get(id.trim()))
    }

    fn memory_by_stable_id(&self, id: &str) -> Option<&MemoryRecord> {
        let trimmed = id.trim();
        self.project_memories
            .values()
            .chain(self.global_memories.values())
            .find(|memory| memory.stable_id == trimmed)
    }

    fn persist_memory(&mut self, memory: &MemoryRecord) -> Result<()> {
        match memory.level {
            MemoryLevel::Project => persist_project_memory(&self.project_memory_dir, memory),
            MemoryLevel::Global => persist_global_memory(&self.global_db_path, memory),
        }
    }

    fn rebuild_fts_index(&mut self) -> Result<()> {
        let mut connection =
            Connection::open_in_memory().context("failed to create in-memory memory index")?;
        connection
            .execute_batch(
                "
                CREATE VIRTUAL TABLE memory_fts USING fts5(
                    memory_id UNINDEXED,
                    title,
                    content,
                    tags,
                    topic,
                    tokenize = 'unicode61 remove_diacritics 2'
                );
                ",
            )
            .context("failed to initialize memory fts index")?;
        let transaction = connection
            .transaction()
            .context("failed to start memory fts transaction")?;
        for memory in self
            .project_memories
            .values()
            .chain(self.global_memories.values())
        {
            transaction
                .execute(
                    "INSERT INTO memory_fts (memory_id, title, content, tags, topic)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![
                        memory.prefixed_id(),
                        tokenize_text(&memory.title),
                        tokenize_text(&memory.content),
                        tokenize_text(&memory.tags.join(" ")),
                        tokenize_text(&memory.topic),
                    ],
                )
                .with_context(|| format!("failed to index memory {}", memory.prefixed_id()))?;
        }
        transaction
            .commit()
            .context("failed to commit memory fts rebuild")?;
        let mut guard = self
            .fts_connection
            .lock()
            .map_err(|_| anyhow!("failed to lock memory fts connection"))?;
        *guard = Some(connection);
        Ok(())
    }

    fn update_loaded_memory(&mut self, memory: &MemoryRecord) {
        match memory.level {
            MemoryLevel::Project => {
                self.project_memories
                    .insert(memory.id.clone(), memory.clone());
            }
            MemoryLevel::Global => {
                self.global_memories
                    .insert(memory.id.clone(), memory.clone());
            }
        }
    }

    fn mark_usage_dirty(&mut self, id: impl Into<String>) {
        self.dirty_usage_updates.insert(id.into());
    }

    fn open_global_connection(&self) -> Result<Connection> {
        Connection::open(&self.global_db_path)
            .with_context(|| format!("failed to open {}", self.global_db_path.display()))
    }
}
