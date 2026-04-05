use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use rusqlite::{Connection, OptionalExtension, params};

use crate::context::{ConversationHistory, DisplayTurn, Hint, NoteEntry};

mod codec;
mod persist;

use codec::{
    decode_tool_summaries, decode_working_directory, optional_system_time,
    normalize_working_directory, role_from_db, system_time_from_unix, unix_timestamp,
};
use persist::{
    replace_conversation_history, replace_hints, replace_notes, replace_open_files,
    update_task_last_active,
};

/// `.ma/ma.db` 的薄封装。
/// 这一层只负责把设计文档里的持久化结构稳定落盘，不参与运行时决策。
pub struct MaStorage {
    workspace_root: PathBuf,
    db_path: PathBuf,
    connection: Connection,
}

#[derive(Debug, Clone)]
pub struct TaskRecord {
    pub id: i64,
    pub name: String,
    pub title_source: TaskTitleSource,
    pub title_locked: bool,
    pub working_directory: PathBuf,
    pub selected_provider_id: Option<i64>,
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
            workspace_root: workdir.to_path_buf(),
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
        self.create_task_with_metadata_and_selection(
            name,
            TaskTitleSource::Default,
            false,
            self.workspace_root.clone(),
            None,
            None,
        )
    }

    pub fn create_task_with_metadata(
        &self,
        name: impl AsRef<str>,
        title_source: TaskTitleSource,
        title_locked: bool,
    ) -> Result<TaskRecord> {
        self.create_task_with_metadata_and_selection(
            name,
            title_source,
            title_locked,
            self.workspace_root.clone(),
            None,
            None,
        )
    }

    pub fn create_task_with_metadata_and_selection(
        &self,
        name: impl AsRef<str>,
        title_source: TaskTitleSource,
        title_locked: bool,
        working_directory: PathBuf,
        selected_provider_id: Option<i64>,
        selected_model: Option<String>,
    ) -> Result<TaskRecord> {
        let name = name.as_ref().trim();
        if name.is_empty() {
            bail!("task name cannot be empty");
        }
        let normalized_model = selected_model.and_then(|model| {
            let trimmed = model.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });
        let working_directory = normalize_working_directory(&working_directory)?;

        let now = SystemTime::now();
        let now_ts = unix_timestamp(now)?;
        self.connection
            .execute(
                "INSERT INTO tasks (
                    name,
                    title_source,
                    title_locked,
                    working_directory,
                    selected_provider_id,
                    selected_model,
                    created_at,
                    last_active
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    name,
                    title_source.as_db_value(),
                    if title_locked { 1 } else { 0 },
                    working_directory.to_string_lossy().to_string(),
                    selected_provider_id,
                    normalized_model,
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
            working_directory,
            selected_provider_id,
            selected_model: normalized_model,
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

    pub fn update_task_selection(
        &self,
        task_id: i64,
        selected_provider_id: Option<i64>,
        selected_model: Option<String>,
    ) -> Result<()> {
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
                 SET selected_provider_id = ?2, selected_model = ?3
                 WHERE id = ?1",
                params![task_id, selected_provider_id, normalized],
            )
            .context("failed to update task selection")?;

        if affected == 0 {
            bail!("task {} not found", task_id);
        }

        Ok(())
    }

    pub fn update_task_working_directory(
        &self,
        task_id: i64,
        working_directory: PathBuf,
    ) -> Result<()> {
        let working_directory = normalize_working_directory(&working_directory)?;

        let affected = self
            .connection
            .execute(
                "UPDATE tasks
                 SET working_directory = ?2
                 WHERE id = ?1",
                params![task_id, working_directory.to_string_lossy().to_string()],
            )
            .context("failed to update task working_directory")?;

        if affected == 0 {
            bail!("task {} not found", task_id);
        }

        Ok(())
    }

    pub fn backfill_missing_task_defaults(
        &self,
        selected_provider_id: Option<i64>,
        selected_model: Option<String>,
    ) -> Result<()> {
        let normalized_model = selected_model.and_then(|model| {
            let trimmed = model.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

        self.connection
            .execute(
                "UPDATE tasks
                 SET selected_provider_id = COALESCE(selected_provider_id, ?1),
                     selected_model = COALESCE(selected_model, ?2)",
                params![selected_provider_id, normalized_model],
            )
            .context("failed to backfill task defaults")?;

        Ok(())
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, title_source, title_locked, working_directory, created_at, last_active
                 , selected_provider_id, selected_model
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
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            })
            .context("failed to query tasks")?;

        let mut tasks = Vec::new();
        for row in rows {
            let (
                id,
                name,
                title_source,
                title_locked,
                working_directory,
                created_at,
                last_active,
                selected_provider_id,
                selected_model,
            ) = row.context("failed to decode task row")?;
            tasks.push(TaskRecord {
                id,
                name,
                title_source: TaskTitleSource::from_db_value(&title_source)?,
                title_locked: title_locked != 0,
                working_directory: decode_working_directory(
                    working_directory,
                    &self.workspace_root,
                )?,
                selected_provider_id,
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
                    working_directory TEXT,
                    selected_provider_id INTEGER,
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
        self.ensure_task_columns()
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
                "SELECT id, name, title_source, title_locked, working_directory, created_at, last_active
                 , selected_provider_id, selected_model
                 FROM tasks
                 WHERE id = ?1",
                params![task_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, i64>(6)?,
                        row.get::<_, Option<i64>>(7)?,
                        row.get::<_, Option<String>>(8)?,
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
            working_directory: decode_working_directory(raw.4, &self.workspace_root)?,
            selected_provider_id: raw.7,
            selected_model: raw.8,
            created_at: system_time_from_unix(raw.5)?,
            last_active: system_time_from_unix(raw.6)?,
        })
    }

    fn ensure_task_columns(&self) -> Result<()> {
        let mut statement = self
            .connection
            .prepare("PRAGMA table_info(tasks)")
            .context("failed to prepare tasks table_info query")?;
        let columns = statement
            .query_map([], |row| row.get::<_, String>(1))
            .context("failed to query tasks table_info")?;

        let mut has_title_source = false;
        let mut has_title_locked = false;
        let mut has_working_directory = false;
        let mut has_selected_provider_id = false;
        let mut has_selected_model = false;
        for column in columns {
            let column = column.context("failed to decode tasks table_info row")?;
            if column == "title_source" {
                has_title_source = true;
            }
            if column == "title_locked" {
                has_title_locked = true;
            }
            if column == "working_directory" {
                has_working_directory = true;
            }
            if column == "selected_provider_id" {
                has_selected_provider_id = true;
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

        if !has_working_directory {
            self.connection
                .execute("ALTER TABLE tasks ADD COLUMN working_directory TEXT", [])
                .context("failed to add tasks.working_directory column")?;
        }

        self.connection
            .execute(
                "UPDATE tasks
                 SET working_directory = ?1
                 WHERE working_directory IS NULL OR TRIM(working_directory) = ''",
                params![self.workspace_root.to_string_lossy().to_string()],
            )
            .context("failed to backfill tasks.working_directory column")?;

        if !has_selected_provider_id {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN selected_provider_id INTEGER",
                    [],
                )
                .context("failed to add tasks.selected_provider_id column")?;
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
    fn task_selection_roundtrips() {
        let workdir = temp_workspace();
        let storage = MaStorage::open(&workdir).expect("open storage");
        let task = storage
            .create_task_with_metadata_and_selection(
                "demo",
                TaskTitleSource::Manual,
                true,
                workdir.clone(),
                Some(7),
                Some("gpt-5.4".to_string()),
            )
            .expect("create task");

        let loaded = storage.load_task(task.id).expect("load task");
        assert_eq!(loaded.task.selected_provider_id, Some(7));
        assert_eq!(loaded.task.selected_model.as_deref(), Some("gpt-5.4"));
    }

    #[test]
    fn backfill_only_updates_missing_task_defaults() {
        let workdir = temp_workspace();
        let storage = MaStorage::open(&workdir).expect("open storage");
        let inherited = storage
            .create_task("inherited")
            .expect("create inherited task");
        let explicit = storage
            .create_task_with_metadata_and_selection(
                "explicit",
                TaskTitleSource::Manual,
                true,
                workdir.clone(),
                Some(9),
                Some("custom-model".to_string()),
            )
            .expect("create explicit task");

        storage
            .backfill_missing_task_defaults(Some(3), Some("gpt-5.3-codex".to_string()))
            .expect("backfill defaults");

        let inherited_loaded = storage.load_task(inherited.id).expect("load inherited");
        assert_eq!(inherited_loaded.task.selected_provider_id, Some(3));
        assert_eq!(
            inherited_loaded.task.selected_model.as_deref(),
            Some("gpt-5.3-codex")
        );

        let explicit_loaded = storage.load_task(explicit.id).expect("load explicit");
        assert_eq!(explicit_loaded.task.selected_provider_id, Some(9));
        assert_eq!(
            explicit_loaded.task.selected_model.as_deref(),
            Some("custom-model")
        );
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
