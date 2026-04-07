use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemorySourceRevision {
    pub project_files: Vec<(String, u64, u64)>,
    pub global_db_mtime_secs: Option<u64>,
    pub global_db_len: Option<u64>,
}

pub(super) fn initialize_global_schema(db_path: &Path) -> Result<()> {
    let connection = Connection::open(db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id           INTEGER PRIMARY KEY,
                stable_id    TEXT,
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

            CREATE UNIQUE INDEX IF NOT EXISTS idx_memories_stable_id ON memories(stable_id);
            CREATE INDEX IF NOT EXISTS idx_memories_scope ON memories(scope);
            CREATE INDEX IF NOT EXISTS idx_memories_topic ON memories(topic);
            ",
        )
        .context("failed to initialize memories schema in settings db")?;

    if !table_has_column(&connection, "memories", "stable_id")? {
        connection
            .execute("ALTER TABLE memories ADD COLUMN stable_id TEXT", [])
            .context("failed to add memories.stable_id column")?;
    }

    connection
        .execute_batch(
            "
            UPDATE memories
               SET stable_id = CAST(id AS TEXT)
             WHERE stable_id IS NULL OR trim(stable_id) = '';

            CREATE UNIQUE INDEX IF NOT EXISTS idx_memories_stable_id ON memories(stable_id);
            ",
        )
        .context("failed to backfill global memory stable ids")?;

    Ok(())
}

pub(super) fn capture_source_revision(
    memory_dir: &Path,
    global_db_path: &Path,
) -> Result<MemorySourceRevision> {
    let mut project_files = Vec::new();
    if memory_dir.exists() {
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
            let metadata = entry
                .metadata()
                .with_context(|| format!("failed to read metadata for {}", path.display()))?;
            let modified = metadata.modified().ok();
            let modified_secs = modified
                .map(|time| {
                    time.duration_since(UNIX_EPOCH)
                        .unwrap_or(Duration::from_secs(0))
                        .as_secs()
                })
                .unwrap_or(0);
            project_files.push((
                path.file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
                modified_secs,
                metadata.len(),
            ));
        }
    }

    let (global_db_mtime_secs, global_db_len) = if global_db_path.exists() {
        let metadata = fs::metadata(global_db_path)
            .with_context(|| format!("failed to read metadata for {}", global_db_path.display()))?;
        let modified_secs = metadata.modified().ok().map(|time| {
            time.duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_secs()
        });
        (modified_secs, Some(metadata.len()))
    } else {
        (None, None)
    };

    Ok(MemorySourceRevision {
        project_files,
        global_db_mtime_secs,
        global_db_len,
    })
}

pub(super) fn load_project_memories(memory_dir: &Path) -> Result<IndexMap<String, MemoryRecord>> {
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

pub(super) fn load_global_memories(db_path: &Path) -> Result<IndexMap<String, MemoryRecord>> {
    let connection = Connection::open(db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id           INTEGER PRIMARY KEY,
                stable_id    TEXT,
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

    if !table_has_column(&connection, "memories", "stable_id")? {
        connection
            .execute("ALTER TABLE memories ADD COLUMN stable_id TEXT", [])
            .context("failed to add memories.stable_id column")?;
    }
    connection
        .execute_batch(
            "
            UPDATE memories
               SET stable_id = CAST(id AS TEXT)
             WHERE stable_id IS NULL OR trim(stable_id) = '';

            CREATE UNIQUE INDEX IF NOT EXISTS idx_memories_stable_id ON memories(stable_id);
            ",
        )
        .context("failed to ensure global memory stable ids exist")?;

    let mut statement = connection
        .prepare(
            "SELECT id, stable_id, scope, memory_type, topic, title, content, tags,
                    access_count, skip_count, created_at, updated_at
             FROM memories
             ORDER BY updated_at DESC, id DESC",
        )
        .context("failed to prepare global memories query")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })
        .context("failed to query global memories")?;

    let mut memories = IndexMap::new();
    for row in rows {
        let (
            id,
            stable_id,
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
            stable_id: stable_id
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| id.to_string()),
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
        id: id.clone(),
        level: MemoryLevel::Project,
        scope,
        stable_id: id.clone(),
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

pub(super) fn persist_project_memory(memory_dir: &Path, memory: &MemoryRecord) -> Result<()> {
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

pub(super) fn persist_global_memory(db_path: &Path, memory: &MemoryRecord) -> Result<()> {
    let connection = Connection::open(db_path)
        .with_context(|| format!("failed to open {}", db_path.display()))?;
    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memories (
                id           INTEGER PRIMARY KEY,
                stable_id    TEXT,
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

    if !table_has_column(&connection, "memories", "stable_id")? {
        connection
            .execute("ALTER TABLE memories ADD COLUMN stable_id TEXT", [])
            .context("failed to add memories.stable_id column")?;
    }
    connection
        .execute_batch(
            "
            UPDATE memories
               SET stable_id = CAST(id AS TEXT)
             WHERE stable_id IS NULL OR trim(stable_id) = '';

            CREATE UNIQUE INDEX IF NOT EXISTS idx_memories_stable_id ON memories(stable_id);
            ",
        )
        .context("failed to ensure global memory stable ids exist")?;

    let stable_id = normalize_stable_id(memory.level, &memory.stable_id);
    let numeric_id = connection
        .query_row(
            "SELECT id FROM memories WHERE stable_id = ?1",
            params![stable_id.clone()],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .context("failed to inspect existing global memory stable id")?
        .or_else(|| memory.id.parse::<i64>().ok())
        .unwrap_or(
            connection
                .query_row("SELECT MAX(id) FROM memories", [], |row| {
                    row.get::<_, Option<i64>>(0)
                })
                .context("failed to inspect current global memory max id")?
                .unwrap_or(0)
                + 1,
        );
    connection
        .execute(
            "INSERT INTO memories (
                id, stable_id, scope, memory_type, topic, title, content, tags,
                access_count, skip_count, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
                stable_id = excluded.stable_id,
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
                stable_id,
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

pub(super) fn parse_memory_body(body: &str) -> (String, String) {
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

pub(super) fn infer_level(explicit: Option<&str>, memory_type: &str) -> Result<MemoryLevel> {
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

pub(super) fn normalize_stable_id(level: MemoryLevel, raw: &str) -> String {
    let trimmed = raw.trim();
    match level {
        MemoryLevel::Project => trimmed.to_string(),
        MemoryLevel::Global => trimmed.to_string(),
    }
}

pub(super) fn normalize_scope(explicit: Option<&str>, active_agent: &str) -> MemoryScope {
    match explicit.map(str::trim).filter(|scope| !scope.is_empty()) {
        None => MemoryScope::Shared,
        Some(scope) if scope.eq_ignore_ascii_case("shared") => MemoryScope::Shared,
        Some(scope) if scope.eq_ignore_ascii_case(active_agent) => {
            MemoryScope::Agent(active_agent.to_string())
        }
        Some(scope) => MemoryScope::Agent(scope.to_string()),
    }
}

pub(super) fn normalize_tags(tags: Vec<String>) -> Vec<String> {
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

pub(super) fn validate_record(memory: &MemoryRecord) -> Result<()> {
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

pub(super) fn parse_global_numeric_id(raw: &str) -> Result<i64> {
    raw.trim()
        .parse::<i64>()
        .with_context(|| format!("invalid global memory id {}", raw))
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    Ok(connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .with_context(|| format!("failed to inspect table info for {table}"))?
        .query_map([], |row| row.get::<_, String>(1))
        .with_context(|| format!("failed to enumerate columns for {table}"))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to decode columns for {table}"))?
        .into_iter()
        .any(|name| name == column))
}

pub(super) fn unix_timestamp(time: SystemTime) -> Result<i64> {
    i64::try_from(
        time.duration_since(UNIX_EPOCH)
            .context("time was before unix epoch")?
            .as_secs(),
    )
    .context("unix timestamp overflow")
}

pub(super) fn from_unix_timestamp(value: i64) -> Result<SystemTime> {
    let seconds = u64::try_from(value).context("negative unix timestamp")?;
    Ok(UNIX_EPOCH + Duration::from_secs(seconds))
}
