use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::context::{ConversationHistory, DisplayTurn, Hint, NoteEntry, Role, ToolSummary};

/// `.ma/ma.db` 的薄封装。
/// 这一层只负责把设计文档里的持久化结构稳定落盘，不参与运行时决策。
pub struct MaStorage {
    db_path: PathBuf,
    connection: Connection,
}

#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub id: i64,
    pub name: String,
    pub title_source: TaskTitleSource,
    pub title_locked: bool,
    pub selected_model: Option<String>,
    pub created_at: SystemTime,
    pub last_active: SystemTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTitleSource {
    Default,
    Auto,
    Manual,
}

#[derive(Debug, Clone)]
pub struct PersistedOpenFile {
    pub path: PathBuf,
    pub locked: bool,
}

#[derive(Debug, Clone)]
pub struct PersistedTask {
    pub task: TaskRecord,
    pub history: ConversationHistory,
    pub notes: IndexMap<String, NoteEntry>,
    pub open_files: Vec<PersistedOpenFile>,
    pub hints: Vec<Hint>,
}

#[derive(Debug, Clone)]
pub struct PersistedTaskState {
    pub history: ConversationHistory,
    pub notes: IndexMap<String, NoteEntry>,
    pub open_files: Vec<PersistedOpenFile>,
    pub hints: Vec<Hint>,
    pub last_active: SystemTime,
}

impl MaStorage {
    pub fn open(workdir: impl AsRef<Path>) -> Result<Self> {
        let workdir = workdir.as_ref();
        let ma_dir = workdir.join(".ma");
        fs::create_dir_all(&ma_dir)
            .with_context(|| format!("failed to create {}", ma_dir.display()))?;

        let db_path = ma_dir.join("ma.db");
        let connection = Connection::open(&db_path)
            .with_context(|| format!("failed to open {}", db_path.display()))?;

        connection
            .pragma_update(None, "foreign_keys", "ON")
            .context("failed to enable sqlite foreign_keys")?;

        let mut storage = Self {
            db_path,
            connection,
        };
        storage.initialize_schema()?;
        storage.delete_expired_hints()?;
        Ok(storage)
    }

    pub fn database_path(&self) -> &Path {
        &self.db_path
    }

    pub fn create_task(&self, name: impl AsRef<str>) -> Result<TaskRecord> {
        self.create_task_with_metadata(name, TaskTitleSource::Default, false)
    }

    pub fn create_task_with_metadata(
        &self,
        name: impl AsRef<str>,
        title_source: TaskTitleSource,
        title_locked: bool,
    ) -> Result<TaskRecord> {
        let name = name.as_ref().trim();
        if name.is_empty() {
            bail!("task name cannot be empty");
        }

        let now = SystemTime::now();
        let now_ts = unix_timestamp(now)?;
        self.connection
            .execute(
                "INSERT INTO tasks (name, title_source, title_locked, created_at, last_active)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    name,
                    title_source.as_db_value(),
                    if title_locked { 1 } else { 0 },
                    now_ts,
                    now_ts
                ],
            )
            .context("failed to insert task")?;

        Ok(TaskRecord {
            id: self.connection.last_insert_rowid(),
            name: name.to_string(),
            title_source,
            title_locked,
            selected_model: None,
            created_at: now,
            last_active: now,
        })
    }

    pub fn update_task_title(
        &self,
        task_id: i64,
        name: impl AsRef<str>,
        title_source: TaskTitleSource,
        title_locked: bool,
    ) -> Result<()> {
        let name = name.as_ref().trim();
        if name.is_empty() {
            bail!("task name cannot be empty");
        }

        let affected = self
            .connection
            .execute(
                "UPDATE tasks
                 SET name = ?2, title_source = ?3, title_locked = ?4
                 WHERE id = ?1",
                params![
                    task_id,
                    name,
                    title_source.as_db_value(),
                    if title_locked { 1 } else { 0 }
                ],
            )
            .context("failed to update task title")?;

        if affected == 0 {
            bail!("task {} not found", task_id);
        }

        Ok(())
    }

    pub fn delete_task(&self, task_id: i64) -> Result<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("failed to start delete_task transaction")?;

        // 历史数据库里这些子表可能没有 `ON DELETE CASCADE`。
        // 这里显式按依赖顺序清理，保证旧工作区升级后也能稳定删除任务。
        transaction
            .execute(
                "DELETE FROM conversation_turns WHERE task_id = ?1",
                params![task_id],
            )
            .context("failed to delete task conversation turns")?;
        transaction
            .execute("DELETE FROM notes WHERE task_id = ?1", params![task_id])
            .context("failed to delete task notes")?;
        transaction
            .execute(
                "DELETE FROM open_files WHERE task_id = ?1",
                params![task_id],
            )
            .context("failed to delete task open files")?;

        let affected = transaction
            .execute("DELETE FROM tasks WHERE id = ?1", params![task_id])
            .context("failed to delete task")?;

        if affected == 0 {
            bail!("task {} not found", task_id);
        }

        transaction
            .commit()
            .context("failed to commit delete_task transaction")?;

        Ok(())
    }

    pub fn update_task_model(&self, task_id: i64, selected_model: Option<String>) -> Result<()> {
        let normalized = selected_model.and_then(|model| {
            let trimmed = model.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

        let affected = self
            .connection
            .execute(
                "UPDATE tasks
                 SET selected_model = ?2
                 WHERE id = ?1",
                params![task_id, normalized],
            )
            .context("failed to update task model")?;

        if affected == 0 {
            bail!("task {} not found", task_id);
        }

        Ok(())
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, title_source, title_locked, created_at, last_active
                 , selected_model
                 FROM tasks
                 ORDER BY last_active DESC, id DESC",
            )
            .context("failed to prepare list_tasks query")?;

        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            })
            .context("failed to query tasks")?;

        let mut tasks = Vec::new();
        for row in rows {
            let (id, name, title_source, title_locked, created_at, last_active, selected_model) =
                row.context("failed to decode task row")?;
            tasks.push(TaskRecord {
                id,
                name,
                title_source: TaskTitleSource::from_db_value(&title_source)?,
                title_locked: title_locked != 0,
                selected_model,
                created_at: system_time_from_unix(created_at)?,
                last_active: system_time_from_unix(last_active)?,
            });
        }
        Ok(tasks)
    }

    pub fn load_task(&self, task_id: i64) -> Result<PersistedTask> {
        let task = self.load_task_record(task_id)?;
        let history = self.load_conversation_history(task_id)?;
        let notes = self.load_notes(task_id)?;
        let open_files = self.load_open_files(task_id)?;
        let hints = self.load_hints()?;

        Ok(PersistedTask {
            task,
            history,
            notes,
            open_files,
            hints,
        })
    }

    pub fn save_task_state(&mut self, task_id: i64, state: &PersistedTaskState) -> Result<()> {
        let transaction = self
            .connection
            .transaction()
            .context("failed to start sqlite transaction")?;

        update_task_last_active(&transaction, task_id, state.last_active)?;
        replace_conversation_history(&transaction, task_id, &state.history)?;
        replace_notes(&transaction, task_id, &state.notes)?;
        replace_open_files(&transaction, task_id, &state.open_files)?;
        replace_hints(&transaction, &state.hints)?;

        transaction
            .commit()
            .context("failed to commit sqlite transaction")?;
        Ok(())
    }

    fn initialize_schema(&mut self) -> Result<()> {
        self.connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS tasks (
                    id          INTEGER PRIMARY KEY,
                    name        TEXT    NOT NULL,
                    title_source TEXT   NOT NULL DEFAULT 'default',
                    title_locked INTEGER NOT NULL DEFAULT 0,
                    selected_model TEXT,
                    created_at  INTEGER NOT NULL,
                    last_active INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS conversation_turns (
                    id             INTEGER PRIMARY KEY,
                    task_id        INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    role           TEXT    NOT NULL,
                    content        TEXT    NOT NULL,
                    tool_summaries TEXT,
                    created_at     INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS notes (
                    task_id  INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    note_id  TEXT    NOT NULL,
                    content  TEXT    NOT NULL,
                    position INTEGER NOT NULL,
                    PRIMARY KEY (task_id, note_id)
                );

                CREATE TABLE IF NOT EXISTS open_files (
                    task_id  INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    path     TEXT    NOT NULL,
                    position INTEGER NOT NULL,
                    locked   INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (task_id, path)
                );

                CREATE TABLE IF NOT EXISTS hints (
                    id              INTEGER PRIMARY KEY,
                    content         TEXT    NOT NULL,
                    expires_at      INTEGER,
                    turns_remaining INTEGER,
                    created_at      INTEGER NOT NULL
                );
                ",
            )
            .context("failed to initialize sqlite schema")?;
        self.ensure_task_title_columns()
    }

    fn delete_expired_hints(&self) -> Result<()> {
        let now_ts = unix_timestamp(SystemTime::now())?;
        self.connection
            .execute(
                "DELETE FROM hints
                 WHERE (expires_at IS NOT NULL AND expires_at <= ?1)
                    OR turns_remaining = 0",
                params![now_ts],
            )
            .context("failed to delete expired hints")?;
        Ok(())
    }

    fn load_task_record(&self, task_id: i64) -> Result<TaskRecord> {
        let raw = self
            .connection
            .query_row(
                "SELECT id, name, title_source, title_locked, created_at, last_active
                 , selected_model
                 FROM tasks
                 WHERE id = ?1",
                params![task_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, Option<String>>(6)?,
                    ))
                },
            )
            .optional()
            .context("failed to load task row")?
            .with_context(|| format!("task {} not found", task_id))?;

        Ok(TaskRecord {
            id: raw.0,
            name: raw.1,
            title_source: TaskTitleSource::from_db_value(&raw.2)?,
            title_locked: raw.3 != 0,
            selected_model: raw.6,
            created_at: system_time_from_unix(raw.4)?,
            last_active: system_time_from_unix(raw.5)?,
        })
    }

    fn ensure_task_title_columns(&self) -> Result<()> {
        let mut statement = self
            .connection
            .prepare("PRAGMA table_info(tasks)")
            .context("failed to prepare tasks table_info query")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .context("failed to query tasks table_info")?;

        let mut has_title_source = false;
        let mut has_title_locked = false;
        let mut has_selected_model = false;
        for column in columns {
            let column = column.context("failed to decode tasks table_info row")?;
            if column == "title_source" {
                has_title_source = true;
            }
            if column == "title_locked" {
                has_title_locked = true;
            }
            if column == "selected_model" {
                has_selected_model = true;
            }
        }

        if !has_title_source {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN title_source TEXT NOT NULL DEFAULT 'default'",
                    [],
                )
                .context("failed to add tasks.title_source column")?;
        }

        if !has_title_locked {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN title_locked INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .context("failed to add tasks.title_locked column")?;
        }

        if !has_selected_model {
            self.connection
                .execute("ALTER TABLE tasks ADD COLUMN selected_model TEXT", [])
                .context("failed to add tasks.selected_model column")?;
        }

        Ok(())
    }

    fn load_conversation_history(&self, task_id: i64) -> Result<ConversationHistory> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT role, content, tool_summaries, created_at
                 FROM conversation_turns
                 WHERE task_id = ?1
                 ORDER BY created_at ASC, id ASC",
            )
            .context("failed to prepare conversation query")?;

        let rows = statement
            .query_map(params![task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })
            .context("failed to query conversation history")?;

        let mut turns = Vec::new();
        for row in rows {
            let (role, content, tool_summaries_json, created_at) =
                row.context("failed to decode conversation row")?;
            turns.push(DisplayTurn {
                role: role_from_db(&role)?,
                content,
                tool_calls: decode_tool_summaries(tool_summaries_json.as_deref())?,
                timestamp: system_time_from_unix(created_at)?,
            });
        }
        Ok(ConversationHistory { turns })
    }

    fn load_notes(&self, task_id: i64) -> Result<IndexMap<String, NoteEntry>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT note_id, content
                 FROM notes
                 WHERE task_id = ?1
                 ORDER BY position ASC, note_id ASC",
            )
            .context("failed to prepare notes query")?;

        let rows = statement
            .query_map(params![task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    NoteEntry::new(row.get::<_, String>(1)?),
                ))
            })
            .context("failed to query notes")?;

        let mut notes = IndexMap::new();
        for row in rows {
            let (id, note) = row.context("failed to decode note row")?;
            notes.insert(id, note);
        }
        Ok(notes)
    }

    fn load_open_files(&self, task_id: i64) -> Result<Vec<PersistedOpenFile>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT path, locked
                 FROM open_files
                 WHERE task_id = ?1
                 ORDER BY position ASC, path ASC",
            )
            .context("failed to prepare open_files query")?;

        let rows = statement
            .query_map(params![task_id], |row| {
                Ok(PersistedOpenFile {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    locked: row.get::<_, i64>(1)? != 0,
                })
            })
            .context("failed to query open files")?;

        let mut open_files = Vec::new();
        for row in rows {
            open_files.push(row.context("failed to decode open_file row")?);
        }
        Ok(open_files)
    }

    fn load_hints(&self) -> Result<Vec<Hint>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT content, expires_at, turns_remaining
                 FROM hints
                 ORDER BY created_at ASC, id ASC",
            )
            .context("failed to prepare hints query")?;

        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<u32>>(2)?,
                ))
            })
            .context("failed to query hints")?;

        let mut hints = Vec::new();
        for row in rows {
            let (content, expires_at, turns_remaining) =
                row.context("failed to decode hint row")?;
            hints.push(Hint::new(
                content,
                optional_system_time(expires_at)?,
                turns_remaining,
            ));
        }
        Ok(hints)
    }
}

fn update_task_last_active(
    transaction: &Transaction<'_>,
    task_id: i64,
    last_active: SystemTime,
) -> Result<()> {
    transaction
        .execute(
            "UPDATE tasks SET last_active = ?2 WHERE id = ?1",
            params![task_id, unix_timestamp(last_active)?],
        )
        .context("failed to update task last_active")?;
    Ok(())
}

fn replace_conversation_history(
    transaction: &Transaction<'_>,
    task_id: i64,
    history: &ConversationHistory,
) -> Result<()> {
    transaction
        .execute(
            "DELETE FROM conversation_turns WHERE task_id = ?1",
            params![task_id],
        )
        .context("failed to clear conversation history")?;

    let mut insert = transaction
        .prepare(
            "INSERT INTO conversation_turns
             (task_id, role, content, tool_summaries, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .context("failed to prepare conversation insert")?;

    for turn in &history.turns {
        let summaries = encode_tool_summaries(&turn.tool_calls)?;
        insert
            .execute(params![
                task_id,
                role_to_db(turn.role),
                turn.content,
                summaries,
                unix_timestamp(turn.timestamp)?,
            ])
            .context("failed to insert conversation turn")?;
    }
    Ok(())
}

fn replace_notes(
    transaction: &Transaction<'_>,
    task_id: i64,
    notes: &IndexMap<String, NoteEntry>,
) -> Result<()> {
    transaction
        .execute("DELETE FROM notes WHERE task_id = ?1", params![task_id])
        .context("failed to clear notes")?;

    let mut insert = transaction
        .prepare(
            "INSERT INTO notes (task_id, note_id, content, position)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .context("failed to prepare note insert")?;

    for (position, (id, note)) in notes.iter().enumerate() {
        insert
            .execute(params![task_id, id, note.content, position as i64])
            .context("failed to insert note")?;
    }
    Ok(())
}

fn replace_open_files(
    transaction: &Transaction<'_>,
    task_id: i64,
    open_files: &[PersistedOpenFile],
) -> Result<()> {
    transaction
        .execute(
            "DELETE FROM open_files WHERE task_id = ?1",
            params![task_id],
        )
        .context("failed to clear open files")?;

    let mut insert = transaction
        .prepare(
            "INSERT INTO open_files (task_id, path, position, locked)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .context("failed to prepare open_file insert")?;

    for (position, open_file) in open_files.iter().enumerate() {
        insert
            .execute(params![
                task_id,
                open_file.path.to_string_lossy().to_string(),
                position as i64,
                if open_file.locked { 1 } else { 0 },
            ])
            .context("failed to insert open_file")?;
    }
    Ok(())
}

fn replace_hints(transaction: &Transaction<'_>, hints: &[Hint]) -> Result<()> {
    transaction
        .execute("DELETE FROM hints", [])
        .context("failed to clear hints")?;

    let mut insert = transaction
        .prepare(
            "INSERT INTO hints (content, expires_at, turns_remaining, created_at)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .context("failed to prepare hint insert")?;

    let now_ts = unix_timestamp(SystemTime::now())?;
    for hint in hints {
        insert
            .execute(params![
                hint.content,
                optional_unix_timestamp(hint.expires_at)?,
                hint.turns_remaining.map(i64::from),
                now_ts,
            ])
            .context("failed to insert hint")?;
    }
    Ok(())
}

fn unix_timestamp(time: SystemTime) -> Result<i64> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .context("system time is before unix epoch")?;
    i64::try_from(duration.as_secs()).context("unix timestamp overflow")
}

fn optional_unix_timestamp(time: Option<SystemTime>) -> Result<Option<i64>> {
    time.map(unix_timestamp).transpose()
}

fn system_time_from_unix(timestamp: i64) -> Result<SystemTime> {
    let seconds = u64::try_from(timestamp).context("negative unix timestamp in database")?;
    Ok(UNIX_EPOCH + Duration::from_secs(seconds))
}

fn optional_system_time(timestamp: Option<i64>) -> Result<Option<SystemTime>> {
    timestamp.map(system_time_from_unix).transpose()
}

impl TaskTitleSource {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }

    pub fn from_db_value(value: &str) -> Result<Self> {
        match value {
            "default" => Ok(Self::Default),
            "auto" => Ok(Self::Auto),
            "manual" => Ok(Self::Manual),
            other => bail!("unknown task title source in database: {}", other),
        }
    }
}

fn role_to_db(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

fn role_from_db(role: &str) -> Result<Role> {
    match role {
        "system" => Ok(Role::System),
        "user" => Ok(Role::User),
        "assistant" => Ok(Role::Assistant),
        "tool" => Ok(Role::Tool),
        other => bail!("unknown role in database: {}", other),
    }
}

fn encode_tool_summaries(tool_summaries: &[ToolSummary]) -> Result<Option<String>> {
    if tool_summaries.is_empty() {
        return Ok(None);
    }

    serde_json::to_string(tool_summaries)
        .map(Some)
        .context("failed to encode tool summaries as json")
}

fn decode_tool_summaries(raw: Option<&str>) -> Result<Vec<ToolSummary>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    serde_json::from_str(raw).context("failed to decode tool summaries from json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_roundtrips_task_state() {
        let workdir = temp_workspace();
        let mut storage = MaStorage::open(&workdir).expect("open storage");
        let task = storage.create_task("demo").expect("create task");

        let mut notes = IndexMap::new();
        notes.insert("target".to_string(), NoteEntry::new("实现上下文持久化"));
        notes.insert("plan".to_string(), NoteEntry::new("先落库再恢复"));

        storage
            .save_task_state(
                task.id,
                &PersistedTaskState {
                    history: ConversationHistory::new(vec![
                        DisplayTurn {
                            role: Role::User,
                            content: "继续".to_string(),
                            tool_calls: Vec::new(),
                            timestamp: UNIX_EPOCH + Duration::from_secs(1),
                        },
                        DisplayTurn {
                            role: Role::Assistant,
                            content: "开始实现持久化".to_string(),
                            tool_calls: vec![ToolSummary {
                                name: "write_file".to_string(),
                                summary: "创建了 storage 模块".to_string(),
                            }],
                            timestamp: UNIX_EPOCH + Duration::from_secs(2),
                        },
                    ]),
                    notes,
                    open_files: vec![
                        PersistedOpenFile {
                            path: workdir.join("src").join("main.rs"),
                            locked: false,
                        },
                        PersistedOpenFile {
                            path: workdir.join("src").join("context.rs"),
                            locked: true,
                        },
                    ],
                    hints: vec![Hint::new("外部通知", None, Some(2))],
                    last_active: UNIX_EPOCH + Duration::from_secs(3),
                },
            )
            .expect("save task state");

        let loaded = storage.load_task(task.id).expect("load task");

        assert_eq!(loaded.task.name, "demo");
        assert_eq!(loaded.history.turns.len(), 2);
        assert_eq!(loaded.notes.len(), 2);
        assert_eq!(loaded.open_files.len(), 2);
        assert!(loaded.open_files[1].locked);
        assert_eq!(loaded.hints.len(), 1);
    }

    #[test]
    fn opening_storage_creates_ma_directory_and_db() {
        let workdir = temp_workspace();
        let storage = MaStorage::open(&workdir).expect("open storage");

        assert!(workdir.join(".ma").is_dir());
        assert!(storage.database_path().is_file());
    }

    #[test]
    fn task_title_metadata_roundtrips() {
        let workdir = temp_workspace();
        let storage = MaStorage::open(&workdir).expect("open storage");
        let task = storage
            .create_task_with_metadata("检查 main.rs 问题", TaskTitleSource::Auto, false)
            .expect("create task");

        let loaded = storage.load_task(task.id).expect("load task");
        assert_eq!(loaded.task.name, "检查 main.rs 问题");
        assert_eq!(loaded.task.title_source, TaskTitleSource::Auto);
        assert!(!loaded.task.title_locked);
    }

    #[test]
    fn delete_task_cleans_up_legacy_child_rows_without_cascade() {
        let workdir = temp_workspace();
        let ma_dir = workdir.join(".ma");
        fs::create_dir_all(&ma_dir).expect("create .ma dir");
        let db_path = ma_dir.join("ma.db");
        let connection = Connection::open(&db_path).expect("open legacy db");

        connection
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;

                CREATE TABLE tasks (
                    id          INTEGER PRIMARY KEY,
                    name        TEXT    NOT NULL,
                    created_at  INTEGER NOT NULL,
                    last_active INTEGER NOT NULL
                );

                CREATE TABLE conversation_turns (
                    id             INTEGER PRIMARY KEY,
                    task_id        INTEGER NOT NULL REFERENCES tasks(id),
                    role           TEXT    NOT NULL,
                    content        TEXT    NOT NULL,
                    tool_summaries TEXT,
                    created_at     INTEGER NOT NULL
                );

                CREATE TABLE notes (
                    task_id  INTEGER NOT NULL REFERENCES tasks(id),
                    note_id  TEXT    NOT NULL,
                    content  TEXT    NOT NULL,
                    position INTEGER NOT NULL,
                    PRIMARY KEY (task_id, note_id)
                );

                CREATE TABLE open_files (
                    task_id  INTEGER NOT NULL REFERENCES tasks(id),
                    path     TEXT    NOT NULL,
                    position INTEGER NOT NULL,
                    locked   INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (task_id, path)
                );
                ",
            )
            .expect("create legacy schema");

        connection
            .execute(
                "INSERT INTO tasks (id, name, created_at, last_active) VALUES (1, 'legacy', 1, 1)",
                [],
            )
            .expect("insert task");
        connection
            .execute(
                "INSERT INTO conversation_turns (task_id, role, content, created_at) VALUES (1, 'user', 'hello', 1)",
                [],
            )
            .expect("insert turn");
        connection
            .execute(
                "INSERT INTO notes (task_id, note_id, content, position) VALUES (1, 'target', 'keep', 0)",
                [],
            )
            .expect("insert note");
        connection
            .execute(
                "INSERT INTO open_files (task_id, path, position, locked) VALUES (1, 'src/main.rs', 0, 0)",
                [],
            )
            .expect("insert open file");
        drop(connection);

        let storage = MaStorage::open(&workdir).expect("open migrated storage");
        storage.delete_task(1).expect("delete legacy task");

        assert!(storage.list_tasks().expect("list tasks").is_empty());
        let verification = Connection::open(&db_path).expect("reopen db");
        let turn_count: i64 = verification
            .query_row("SELECT COUNT(*) FROM conversation_turns", [], |row| {
                row.get(0)
            })
            .expect("count turns");
        let note_count: i64 = verification
            .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
            .expect("count notes");
        let open_file_count: i64 = verification
            .query_row("SELECT COUNT(*) FROM open_files", [], |row| row.get(0))
            .expect("count open files");
        assert_eq!(turn_count, 0);
        assert_eq!(note_count, 0);
        assert_eq!(open_file_count, 0);
    }

    fn temp_workspace() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "ma-storage-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(root.join("src").join("main.rs"), "fn main() {}\n").expect("write main");
        fs::write(root.join("src").join("context.rs"), "// context\n").expect("write context");
        root
    }
}
