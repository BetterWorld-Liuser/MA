use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use indexmap::IndexMap;
use jieba_rs::Jieba;
use lazy_static::lazy_static;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::settings::march_settings_dir;

const PROJECT_MEMORY_DIR: &str = ".march/memories";
const LOW_CONTEXT_PRESSURE_SKIP_THRESHOLD: u8 = 95;

lazy_static! {
    static ref JIEBA: Jieba = Jieba::new();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLevel {
    Project,
    Global,
}

impl MemoryLevel {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Project => "p",
            Self::Global => "g",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "agent", rename_all = "snake_case")]
pub enum MemoryScope {
    Shared,
    Agent(String),
}

impl MemoryScope {
    pub fn from_storage(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("shared") {
            Self::Shared
        } else {
            Self::Agent(trimmed.to_string())
        }
    }

    pub fn as_storage(&self) -> String {
        match self {
            Self::Shared => "shared".to_string(),
            Self::Agent(name) => name.clone(),
        }
    }

    pub fn is_visible_to(&self, active_agent: &str) -> bool {
        matches!(self, Self::Shared)
            || matches!(self, Self::Agent(name) if name.eq_ignore_ascii_case(active_agent))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub level: MemoryLevel,
    pub scope: MemoryScope,
    pub memory_type: String,
    pub topic: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub access_count: u32,
    pub skip_count: u32,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

impl MemoryRecord {
    pub fn prefixed_id(&self) -> String {
        format!("{}:{}", self.level.prefix(), self.id)
    }

    fn normalized_tags(&self) -> HashSet<String> {
        self.tags
            .iter()
            .flat_map(|tag| tokenize_terms(tag))
            .collect::<HashSet<_>>()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryIndexEntry {
    pub id: String,
    pub memory_type: String,
    pub topic: String,
    pub title: String,
    pub level: MemoryLevel,
    pub score: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryIndexView {
    pub entries: Vec<MemoryIndexEntry>,
    pub topic_warnings: Vec<String>,
}

impl MemoryIndexView {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn render_for_prompt(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        output.push_str(&format!(
            "Matched {} relevant memories. Use recall_memory(id) for full details.\n\n",
            self.entries.len()
        ));

        for entry in &self.entries {
            output.push_str(&format!(
                "  {:<24} [{:<10}] {:<12} {}\n",
                entry.id, entry.memory_type, entry.topic, entry.title
            ));
        }

        if !self.topic_warnings.is_empty() {
            output.push('\n');
            for warning in &self.topic_warnings {
                output.push_str(&format!("! {warning}\n"));
            }
        }

        output.trim_end().to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    pub latest_user_message: Option<String>,
    pub open_file_paths: Vec<PathBuf>,
    pub recent_assistant_messages: Vec<String>,
    pub active_agent: String,
    pub context_pressure_percent: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct MemorizeRequest {
    pub id: String,
    pub memory_type: String,
    pub topic: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub scope: Option<String>,
    pub level: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateMemoryRequest {
    pub id: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub topic: Option<String>,
    pub memory_type: Option<String>,
}

pub struct MemoryManager {
    project_memory_dir: PathBuf,
    global_db_path: PathBuf,
    project_memories: IndexMap<String, MemoryRecord>,
    global_memories: IndexMap<String, MemoryRecord>,
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
        };
        manager.initialize_global_schema()?;
        manager.reload()?;
        Ok(manager)
    }

    pub fn reload(&mut self) -> Result<()> {
        self.project_memories = load_project_memories(&self.project_memory_dir)?;
        self.global_memories = load_global_memories(&self.global_db_path)?;
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
            .filter(|(_, score)| *score > 0.0)
            .take(limited)
            .collect::<Vec<_>>();

        if selected_ids.is_empty() {
            return Ok(MemoryIndexView::default());
        }

        let selected_id_set = selected_ids
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<HashSet<_>>();
        self.increment_skip_counts(&selected_id_set)?;

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
        self.reload()?;
        let mut memory = self
            .memory_by_prefixed_id(id)
            .cloned()
            .ok_or_else(|| anyhow!("memory {} not found", id))?;
        if !memory.scope.is_visible_to(active_agent) {
            bail!("memory {} is not visible to agent {}", id, active_agent);
        }

        memory.access_count = memory.access_count.saturating_add(1);
        memory.skip_count = 0;
        memory.updated_at = SystemTime::now();
        self.persist_memory(&memory)?;
        self.reload()?;
        Ok(memory)
    }

    pub fn memorize(
        &mut self,
        request: MemorizeRequest,
        active_agent: &str,
    ) -> Result<MemoryRecord> {
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
        record.memory_type = request.memory_type.trim().to_string();
        record.topic = request.topic.trim().to_string();
        record.title = request.title.trim().to_string();
        record.content = request.content.trim().to_string();
        record.tags = normalize_tags(request.tags);
        record.updated_at = now;

        validate_record(&record)?;
        self.persist_memory(&record)?;
        self.reload()?;
        Ok(self
            .memory_by_prefixed_id(&record.prefixed_id())
            .cloned()
            .unwrap_or(record))
    }

    pub fn update_memory(&mut self, request: UpdateMemoryRequest) -> Result<MemoryRecord> {
        self.reload()?;
        let mut memory = self
            .memory_by_prefixed_id(&request.id)
            .cloned()
            .or_else(|| self.memory_by_raw_id(&request.id).cloned())
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
        self.reload()?;
        Ok(memory)
    }

    pub fn forget(&mut self, id: &str) -> Result<()> {
        self.reload()?;
        let memory = self
            .memory_by_prefixed_id(id)
            .cloned()
            .or_else(|| self.memory_by_raw_id(id).cloned())
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
            self.reload()?;
        }
        Ok(changed)
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
        if let Some(memory) = self.global_memories.get(trimmed) {
            return Ok(Some(memory.clone()));
        }

        let Ok(numeric_id) = trimmed.parse::<i64>() else {
            return Ok(None);
        };
        Ok(self.global_memories.get(&numeric_id.to_string()).cloned())
    }

    fn initialize_global_schema(&self) -> Result<()> {
        let connection = self.open_global_connection()?;
        connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS memories (
                    id           INTEGER PRIMARY KEY,
                    scope        TEXT    NOT NULL DEFAULT 'shared',
                    memory_type  TEXT    NOT NULL,
                    topic        TEXT    NOT NULL,
                    title        TEXT    NOT NULL,
                    content      TEXT    NOT NULL,
                    tags         TEXT    NOT NULL DEFAULT '',
                    access_count INTEGER NOT NULL DEFAULT 0,
                    skip_count   INTEGER NOT NULL DEFAULT 0,
                    created_at   INTEGER NOT NULL,
                    updated_at   INTEGER NOT NULL
                );

                CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);
                CREATE INDEX IF NOT EXISTS idx_memories_topic ON memories(topic);
                ",
            )
            .context("failed to initialize memories schema in settings db")
    }

    fn fts_scores(&self, tokens: &[String]) -> Result<IndexMap<String, f32>> {
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

        let query = tokens
            .iter()
            .map(|token| format!("\"{}\"", token.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        if query.trim().is_empty() {
            return Ok(IndexMap::new());
        }

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

    fn increment_skip_counts(&mut self, ids: &HashSet<String>) -> Result<()> {
        for id in ids {
            if let Some(mut memory) = self.memory_by_prefixed_id(id).cloned() {
                memory.skip_count = memory.skip_count.saturating_add(1);
                self.persist_memory(&memory)?;
            }
        }
        Ok(())
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

    fn persist_memory(&mut self, memory: &MemoryRecord) -> Result<()> {
        match memory.level {
            MemoryLevel::Project => persist_project_memory(&self.project_memory_dir, memory),
            MemoryLevel::Global => persist_global_memory(&self.global_db_path, memory),
        }
    }

    fn open_global_connection(&self) -> Result<Connection> {
        Connection::open(&self.global_db_path)
            .with_context(|| format!("failed to open {}", self.global_db_path.display()))
    }
}

fn load_project_memories(memory_dir: &Path) -> Result<IndexMap<String, MemoryRecord>> {
    let mut memories = IndexMap::new();
    if !memory_dir.exists() {
        return Ok(memories);
    }

    let mut entries = fs::read_dir(memory_dir)
        .with_context(|| format!("failed to read {}", memory_dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to enumerate {}", memory_dir.display()))?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let memory = parse_project_memory_file(&path, &raw)?;
        memories.insert(memory.id.clone(), memory);
    }
    Ok(memories)
}

fn load_global_memories(db_path: &Path) -> Result<IndexMap<String, MemoryRecord>> {
    let connection = Connection::open(db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id           INTEGER PRIMARY KEY,
                scope        TEXT    NOT NULL DEFAULT 'shared',
                memory_type  TEXT    NOT NULL,
                topic        TEXT    NOT NULL,
                title        TEXT    NOT NULL,
                content      TEXT    NOT NULL,
                tags         TEXT    NOT NULL DEFAULT '',
                access_count INTEGER NOT NULL DEFAULT 0,
                skip_count   INTEGER NOT NULL DEFAULT 0,
                created_at   INTEGER NOT NULL,
                updated_at   INTEGER NOT NULL
            );
            ",
        )
        .context("failed to ensure global memories table exists")?;

    let mut statement = connection
        .prepare(
            "SELECT id, scope, memory_type, topic, title, content, tags,
                    access_count, skip_count, created_at, updated_at
             FROM memories
             ORDER BY updated_at DESC, id DESC",
        )
        .context("failed to prepare global memories query")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })
        .context("failed to query global memories")?;

    let mut memories = IndexMap::new();
    for row in rows {
        let (
            id,
            scope,
            memory_type,
            topic,
            title,
            content,
            tags,
            access_count,
            skip_count,
            created_at,
            updated_at,
        ) = row.context("failed to decode global memory row")?;
        let memory = MemoryRecord {
            id: id.to_string(),
            level: MemoryLevel::Global,
            scope: MemoryScope::from_storage(&scope),
            memory_type,
            topic,
            title,
            content,
            tags: tags.split_whitespace().map(ToString::to_string).collect(),
            access_count: access_count as u32,
            skip_count: skip_count as u32,
            created_at: from_unix_timestamp(created_at)?,
            updated_at: from_unix_timestamp(updated_at)?,
        };
        memories.insert(memory.id.clone(), memory);
    }
    Ok(memories)
}

fn parse_project_memory_file(path: &Path, raw: &str) -> Result<MemoryRecord> {
    let id = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("invalid memory filename {}", path.display()))?
        .to_string();
    let (frontmatter, body) = split_frontmatter(raw);

    let scope = MemoryScope::from_storage(
        frontmatter
            .get("scope")
            .map(String::as_str)
            .unwrap_or("shared"),
    );
    let memory_type = frontmatter
        .get("type")
        .cloned()
        .unwrap_or_else(|| "fact".to_string());
    let topic = frontmatter
        .get("topic")
        .cloned()
        .unwrap_or_else(|| "general".to_string());
    let tags = frontmatter
        .get("tags")
        .map(|value| {
            value
                .split_whitespace()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let access_count = frontmatter
        .get("access_count")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let skip_count = frontmatter
        .get("skip_count")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    let created_at = frontmatter
        .get("created_at")
        .and_then(|value| value.parse::<i64>().ok())
        .map(from_unix_timestamp)
        .transpose()?
        .unwrap_or_else(SystemTime::now);
    let updated_at = frontmatter
        .get("updated_at")
        .and_then(|value| value.parse::<i64>().ok())
        .map(from_unix_timestamp)
        .transpose()?
        .unwrap_or(created_at);

    let (title, content) = parse_memory_body(&body);
    Ok(MemoryRecord {
        id,
        level: MemoryLevel::Project,
        scope,
        memory_type,
        topic,
        title,
        content,
        tags,
        access_count,
        skip_count,
        created_at,
        updated_at,
    })
}

fn persist_project_memory(memory_dir: &Path, memory: &MemoryRecord) -> Result<()> {
    let path = memory_dir.join(format!("{}.md", memory.id));
    let mut frontmatter = BTreeMap::new();
    frontmatter.insert("type".to_string(), memory.memory_type.clone());
    frontmatter.insert("scope".to_string(), memory.scope.as_storage());
    frontmatter.insert("topic".to_string(), memory.topic.clone());
    frontmatter.insert("tags".to_string(), memory.tags.join(" "));
    frontmatter.insert("access_count".to_string(), memory.access_count.to_string());
    frontmatter.insert("skip_count".to_string(), memory.skip_count.to_string());
    frontmatter.insert(
        "created_at".to_string(),
        unix_timestamp(memory.created_at)?.to_string(),
    );
    frontmatter.insert(
        "updated_at".to_string(),
        unix_timestamp(memory.updated_at)?.to_string(),
    );

    let mut output = String::from("---\n");
    for (key, value) in frontmatter {
        output.push_str(&format!("{key}: {value}\n"));
    }
    output.push_str("---\n");
    output.push_str(&format!("# {}\n\n", memory.title.trim()));
    output.push_str(memory.content.trim());
    output.push('\n');

    fs::create_dir_all(memory_dir)
        .with_context(|| format!("failed to create {}", memory_dir.display()))?;
    fs::write(&path, output).with_context(|| format!("failed to write {}", path.display()))
}

fn persist_global_memory(db_path: &Path, memory: &MemoryRecord) -> Result<()> {
    let connection = Connection::open(db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id           INTEGER PRIMARY KEY,
                scope        TEXT    NOT NULL DEFAULT 'shared',
                memory_type  TEXT    NOT NULL,
                topic        TEXT    NOT NULL,
                title        TEXT    NOT NULL,
                content      TEXT    NOT NULL,
                tags         TEXT    NOT NULL DEFAULT '',
                access_count INTEGER NOT NULL DEFAULT 0,
                skip_count   INTEGER NOT NULL DEFAULT 0,
                created_at   INTEGER NOT NULL,
                updated_at   INTEGER NOT NULL
            );
            ",
        )
        .context("failed to ensure global memories table exists")?;

    let numeric_id = if let Ok(id) = memory.id.parse::<i64>() {
        id
    } else {
        connection
            .query_row("SELECT MAX(id) FROM memories", [], |row| {
                row.get::<_, Option<i64>>(0)
            })
            .context("failed to inspect current global memory max id")?
            .unwrap_or(0)
            + 1
    };
    connection
        .execute(
            "INSERT INTO memories (
                id, scope, memory_type, topic, title, content, tags,
                access_count, skip_count, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET
                scope = excluded.scope,
                memory_type = excluded.memory_type,
                topic = excluded.topic,
                title = excluded.title,
                content = excluded.content,
                tags = excluded.tags,
                access_count = excluded.access_count,
                skip_count = excluded.skip_count,
                created_at = excluded.created_at,
                updated_at = excluded.updated_at",
            params![
                numeric_id,
                memory.scope.as_storage(),
                memory.memory_type,
                memory.topic,
                memory.title,
                memory.content,
                memory.tags.join(" "),
                i64::from(memory.access_count),
                i64::from(memory.skip_count),
                unix_timestamp(memory.created_at)?,
                unix_timestamp(memory.updated_at)?,
            ],
        )
        .context("failed to persist global memory")?;
    Ok(())
}

fn split_frontmatter(raw: &str) -> (BTreeMap<String, String>, String) {
    let normalized = raw.replace("\r\n", "\n");
    let Some(rest) = normalized.strip_prefix("---\n") else {
        return (BTreeMap::new(), normalized);
    };
    let Some((frontmatter_raw, body)) = rest.split_once("\n---\n") else {
        return (BTreeMap::new(), normalized);
    };

    let mut frontmatter = BTreeMap::new();
    for line in frontmatter_raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Some((key, value)) = line.split_once(':') {
            frontmatter.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    (frontmatter, body.to_string())
}

fn parse_memory_body(body: &str) -> (String, String) {
    let normalized = body.trim().replace("\r\n", "\n");
    let mut lines = normalized.lines();
    let title = lines
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .unwrap_or("Untitled memory")
        .to_string();
    let content = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    (title, content)
}

fn infer_level(explicit: Option<&str>, memory_type: &str) -> Result<MemoryLevel> {
    if let Some(explicit) = explicit {
        return match explicit.trim().to_ascii_lowercase().as_str() {
            "project" => Ok(MemoryLevel::Project),
            "global" => Ok(MemoryLevel::Global),
            other => bail!("unsupported memory level {}", other),
        };
    }

    if memory_type.trim().eq_ignore_ascii_case("preference") {
        Ok(MemoryLevel::Global)
    } else {
        Ok(MemoryLevel::Project)
    }
}

fn normalize_scope(explicit: Option<&str>, active_agent: &str) -> MemoryScope {
    match explicit.map(str::trim).filter(|scope| !scope.is_empty()) {
        None => MemoryScope::Shared,
        Some(scope) if scope.eq_ignore_ascii_case("shared") => MemoryScope::Shared,
        Some(scope) if scope.eq_ignore_ascii_case(active_agent) => {
            MemoryScope::Agent(active_agent.to_string())
        }
        Some(scope) => MemoryScope::Agent(scope.to_string()),
    }
}

fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !normalized.iter().any(|existing| existing == trimmed) {
            normalized.push(trimmed.to_string());
        }
    }
    normalized
}

fn validate_record(memory: &MemoryRecord) -> Result<()> {
    if memory.id.trim().is_empty() {
        bail!("memory id cannot be empty");
    }
    if memory.title.trim().is_empty() {
        bail!("memory title cannot be empty");
    }
    if memory.content.trim().is_empty() {
        bail!("memory content cannot be empty");
    }
    if memory.topic.trim().is_empty() {
        bail!("memory topic cannot be empty");
    }
    if memory.memory_type.trim().is_empty() {
        bail!("memory_type cannot be empty");
    }
    Ok(())
}

fn query_tokens(query: &MemoryQuery) -> Vec<String> {
    let mut combined = String::new();
    if let Some(message) = &query.latest_user_message {
        combined.push_str(message);
        combined.push(' ');
    }
    for path in &query.open_file_paths {
        combined.push_str(&path.to_string_lossy());
        combined.push(' ');
    }
    for message in &query.recent_assistant_messages {
        combined.push_str(message);
        combined.push(' ');
    }

    tokenize_terms(&combined)
}

fn tokenize_text(raw: &str) -> String {
    tokenize_terms(raw).join(" ")
}

fn tokenize_terms(raw: &str) -> Vec<String> {
    let prepared = raw
        .replace(
            ['/', '\\', '.', '_', '-', ':', ',', '(', ')', '[', ']'],
            " ",
        )
        .replace('\n', " ");
    let mut tokens = Vec::new();
    let mut seen = HashSet::new();

    for piece in JIEBA.cut(&prepared, false) {
        for token in piece.split_whitespace() {
            let lowered = token.trim().to_ascii_lowercase();
            if lowered.is_empty() {
                continue;
            }
            if lowered.chars().all(|ch| ch.is_ascii_punctuation()) {
                continue;
            }
            if seen.insert(lowered.clone()) {
                tokens.push(lowered);
            }
        }
    }

    tokens
}

fn collect_open_path_segments(paths: &[PathBuf]) -> HashSet<String> {
    let mut segments = HashSet::new();
    for path in paths {
        for token in tokenize_terms(&path.to_string_lossy()) {
            segments.insert(token);
        }
    }
    segments
}

fn calculate_path_match_score(memory: &MemoryRecord, path_segments: &HashSet<String>) -> f32 {
    if path_segments.is_empty() {
        return 0.0;
    }
    let tags = memory.normalized_tags();
    let hits = tags.intersection(path_segments).count();
    if hits == 0 {
        0.0
    } else {
        hits as f32 / path_segments.len().max(1) as f32
    }
}

fn calculate_recency_score(updated_at: SystemTime) -> f32 {
    let age = SystemTime::now()
        .duration_since(updated_at)
        .unwrap_or(Duration::from_secs(0));
    let days = age.as_secs_f32() / 86_400.0;
    1.0 / (1.0 + days)
}

fn calculate_frequency_score(access_count: u32, max_access_count: u32) -> f32 {
    if max_access_count == 0 {
        return 0.0;
    }
    let current = (1.0 + access_count as f32).ln();
    let max = (1.0 + max_access_count as f32).ln();
    (current / max).clamp(0.0, 1.0)
}

fn topic_warnings_for_entries<'a>(
    ids: impl Iterator<Item = &'a str>,
    project_memories: &IndexMap<String, MemoryRecord>,
    global_memories: &IndexMap<String, MemoryRecord>,
) -> Vec<String> {
    let mut topic_counts = BTreeMap::<String, usize>::new();
    for id in ids {
        let memory = if let Some(raw_id) = id.strip_prefix("p:") {
            project_memories.get(raw_id)
        } else if let Some(raw_id) = id.strip_prefix("g:") {
            global_memories.get(raw_id)
        } else {
            None
        };
        let Some(memory) = memory else {
            continue;
        };
        *topic_counts.entry(memory.topic.clone()).or_default() += 1;
    }

    topic_counts
        .into_iter()
        .filter(|(_, count)| *count > 5)
        .map(|(topic, count)| {
            format!(
                "Topic \"{}\" currently has {} matched memories; consider merging them into a tighter summary.",
                topic, count
            )
        })
        .collect()
}

fn parse_global_numeric_id(raw: &str) -> Result<i64> {
    raw.trim()
        .parse::<i64>()
        .with_context(|| format!("invalid global memory id {}", raw))
}

fn unix_timestamp(time: SystemTime) -> Result<i64> {
    i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .context("time was before unix epoch")?
            .as_secs(),
    )
    .context("unix timestamp overflow")
}

fn from_unix_timestamp(value: i64) -> Result<SystemTime> {
    let seconds = u64::try_from(value).context("negative unix timestamp")?;
    Ok(UNIX_EPOCH + Duration::from_secs(seconds))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenization_keeps_cjk_and_path_words_searchable() {
        let tokens = tokenize_terms("src/auth/middleware.rs 刷新令牌");
        assert!(tokens.contains(&"src".to_string()));
        assert!(tokens.contains(&"auth".to_string()));
        assert!(tokens.contains(&"middleware".to_string()));
        assert!(tokens.iter().any(|token| token.contains("刷新")));
    }

    #[test]
    fn memory_body_parsing_extracts_heading_as_title() {
        let (title, content) = parse_memory_body("# JWT 规则\n\naccess token 15 分钟");
        assert_eq!(title, "JWT 规则");
        assert_eq!(content, "access token 15 分钟");
    }
}
