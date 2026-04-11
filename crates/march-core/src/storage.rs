use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::agents::MARCH_AGENT_NAME;
use crate::context::{Hint, NoteEntry};
use crate::paths::clean_path;

mod codec;
mod persist;
mod tasks;
mod timeline;

use codec::unix_timestamp;
use persist::{
    replace_hints, replace_notes, replace_open_files, replace_task_timeline,
    update_task_last_active,
};
pub use tasks::TaskCreateOptions;
pub use timeline::{
    PersistedAssistantMessage, PersistedAssistantMessageState, PersistedAssistantTimelineEntry,
    PersistedReplyRef, PersistedTaskTimeline, PersistedTaskTimelineEntry, PersistedToolCallState,
    PersistedTurn, PersistedTurnState, PersistedTurnTrigger, PersistedUserMessage,
    history_from_timeline, turn_agent_id,
};

pub struct MarchStorage {
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
    pub selected_model_config_id: Option<i64>,
    pub selected_model: Option<String>,
    pub model_temperature: Option<f32>,
    pub model_top_p: Option<f32>,
    pub model_presence_penalty: Option<f32>,
    pub model_frequency_penalty: Option<f32>,
    pub model_max_output_tokens: Option<u32>,
    pub active_agent: String,
    pub last_event_seq: u64,
    pub created_at: SystemTime,
    pub last_active: SystemTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTitleSource {
    Default,
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedOpenFile {
    pub scope: String,
    pub path: PathBuf,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedNote {
    pub scope: String,
    pub id: String,
    pub entry: NoteEntry,
}

#[derive(Debug, Clone)]
pub struct PersistedTask {
    pub task: TaskRecord,
    pub active_agent: String,
    pub timeline: PersistedTaskTimeline,
    pub notes: Vec<PersistedNote>,
    pub open_files: Vec<PersistedOpenFile>,
    pub hints: Vec<Hint>,
}

#[derive(Debug, Clone)]
pub struct PersistedTaskState {
    pub active_agent: String,
    pub timeline: Option<PersistedTaskTimeline>,
    pub notes: Vec<PersistedNote>,
    pub open_files: Vec<PersistedOpenFile>,
    pub hints: Vec<Hint>,
    pub last_active: SystemTime,
}

impl MarchStorage {
    pub fn open(workdir: impl AsRef<Path>) -> Result<Self> {
        let workdir = workdir.as_ref();
        let ma_dir = workdir.join(".march");
        fs::create_dir_all(&ma_dir)
            .with_context(|| format!("failed to create {}", ma_dir.display()))?;

        let db_path = ma_dir.join("march.db");
        let connection = Connection::open(&db_path)
            .with_context(|| format!("failed to open {}", db_path.display()))?;

        connection
            .pragma_update(None, "foreign_keys", "ON")
            .context("failed to enable sqlite foreign_keys")?;

        let mut storage = Self {
            workspace_root: clean_path(workdir.to_path_buf()),
            db_path: clean_path(db_path),
            connection,
        };
        storage.initialize_schema()?;
        storage.delete_expired_hints()?;
        Ok(storage)
    }

    pub fn database_path(&self) -> &Path {
        &self.db_path
    }

    pub fn save_task_state(&mut self, task_id: i64, state: &PersistedTaskState) -> Result<()> {
        let transaction = self
            .connection
            .transaction()
            .context("failed to start sqlite transaction")?;

        update_task_last_active(&transaction, task_id, state.last_active)?;
        if let Some(timeline) = state.timeline.as_deref() {
            replace_task_timeline(&transaction, task_id, timeline)?;
        }
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
                    selected_model_config_id INTEGER,
                    selected_model TEXT,
                    model_temperature REAL,
                    model_top_p REAL,
                    model_presence_penalty REAL,
                    model_frequency_penalty REAL,
                    model_max_output_tokens INTEGER,
                    active_agent TEXT NOT NULL DEFAULT 'march',
                    last_event_seq INTEGER NOT NULL DEFAULT 0,
                    created_at  INTEGER NOT NULL,
                    last_active INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS task_timeline_entries (
                    id               INTEGER PRIMARY KEY,
                    task_id          INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    position         INTEGER NOT NULL,
                    kind             TEXT    NOT NULL,
                    user_message_id  TEXT,
                    turn_id          TEXT,
                    content          TEXT,
                    mentions_json    TEXT,
                    replies_json     TEXT,
                    agent_id         TEXT,
                    trigger_json     TEXT,
                    state            TEXT,
                    error_message    TEXT,
                    created_at       INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS task_turn_messages (
                    id               INTEGER PRIMARY KEY,
                    task_id          INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    turn_id          TEXT    NOT NULL,
                    message_id       TEXT    NOT NULL,
                    state            TEXT    NOT NULL,
                    reasoning        TEXT    NOT NULL,
                    position         INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS task_message_timeline_entries (
                    id               INTEGER PRIMARY KEY,
                    task_id          INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    message_id       TEXT    NOT NULL,
                    kind             TEXT    NOT NULL,
                    text             TEXT,
                    tool_call_id     TEXT,
                    tool_name        TEXT,
                    arguments        TEXT,
                    status           TEXT,
                    preview          TEXT,
                    duration_ms      INTEGER,
                    position         INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS notes (
                    task_id  INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    scope    TEXT    NOT NULL DEFAULT 'shared',
                    note_id  TEXT    NOT NULL,
                    content  TEXT    NOT NULL,
                    position INTEGER NOT NULL,
                    PRIMARY KEY (task_id, scope, note_id)
                );

                CREATE TABLE IF NOT EXISTS open_files (
                    task_id  INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    scope    TEXT    NOT NULL DEFAULT 'shared',
                    path     TEXT    NOT NULL,
                    position INTEGER NOT NULL,
                    locked   INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (task_id, scope, path)
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
        self.ensure_task_columns()?;
        self.ensure_timeline_indexes()?;
        self.ensure_note_columns()?;
        self.ensure_open_file_columns()?;
        Ok(())
    }

    fn ensure_timeline_indexes(&self) -> Result<()> {
        self.connection
            .execute_batch(
                "
                CREATE INDEX IF NOT EXISTS idx_task_timeline_entries_task_position
                    ON task_timeline_entries(task_id, position);
                CREATE INDEX IF NOT EXISTS idx_task_timeline_entries_task_turn
                    ON task_timeline_entries(task_id, turn_id);
                CREATE INDEX IF NOT EXISTS idx_task_turn_messages_task_turn_position
                    ON task_turn_messages(task_id, turn_id, position);
                CREATE INDEX IF NOT EXISTS idx_task_message_timeline_entries_task_message_position
                    ON task_message_timeline_entries(task_id, message_id, position);
                ",
            )
            .context("failed to ensure timeline indexes")?;
        Ok(())
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

    fn ensure_task_columns(&self) -> Result<()> {
        let columns = self.table_columns("tasks")?;

        if !columns.iter().any(|column| column == "title_source") {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN title_source TEXT NOT NULL DEFAULT 'default'",
                    [],
                )
                .context("failed to add tasks.title_source column")?;
        }

        if !columns.iter().any(|column| column == "title_locked") {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN title_locked INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .context("failed to add tasks.title_locked column")?;
        }

        if !columns.iter().any(|column| column == "working_directory") {
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

        if !columns
            .iter()
            .any(|column| column == "selected_model_config_id")
        {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN selected_model_config_id INTEGER",
                    [],
                )
                .context("failed to add tasks.selected_model_config_id column")?;
        }

        if !columns.iter().any(|column| column == "selected_model") {
            self.connection
                .execute("ALTER TABLE tasks ADD COLUMN selected_model TEXT", [])
                .context("failed to add tasks.selected_model column")?;
        }

        if !columns.iter().any(|column| column == "model_temperature") {
            self.connection
                .execute("ALTER TABLE tasks ADD COLUMN model_temperature REAL", [])
                .context("failed to add tasks.model_temperature column")?;
        }

        if !columns.iter().any(|column| column == "model_top_p") {
            self.connection
                .execute("ALTER TABLE tasks ADD COLUMN model_top_p REAL", [])
                .context("failed to add tasks.model_top_p column")?;
        }

        if !columns
            .iter()
            .any(|column| column == "model_presence_penalty")
        {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN model_presence_penalty REAL",
                    [],
                )
                .context("failed to add tasks.model_presence_penalty column")?;
        }

        if !columns
            .iter()
            .any(|column| column == "model_frequency_penalty")
        {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN model_frequency_penalty REAL",
                    [],
                )
                .context("failed to add tasks.model_frequency_penalty column")?;
        }

        if !columns
            .iter()
            .any(|column| column == "model_max_output_tokens")
        {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN model_max_output_tokens INTEGER",
                    [],
                )
                .context("failed to add tasks.model_max_output_tokens column")?;
        }

        if !columns.iter().any(|column| column == "active_agent") {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN active_agent TEXT NOT NULL DEFAULT 'march'",
                    [],
                )
                .context("failed to add tasks.active_agent column")?;
        }

        self.connection
            .execute(
                "UPDATE tasks
                 SET active_agent = ?1
                 WHERE active_agent IS NULL OR TRIM(active_agent) = ''",
                params![MARCH_AGENT_NAME],
            )
            .context("failed to backfill tasks.active_agent column")?;

        if !columns.iter().any(|column| column == "last_event_seq") {
            self.connection
                .execute(
                    "ALTER TABLE tasks ADD COLUMN last_event_seq INTEGER NOT NULL DEFAULT 0",
                    [],
                )
                .context("failed to add tasks.last_event_seq column")?;
        }
        self.connection
            .execute(
                "UPDATE tasks
                 SET last_event_seq = 0
                 WHERE last_event_seq IS NULL OR last_event_seq < 0",
                [],
            )
            .context("failed to backfill tasks.last_event_seq column")?;

        Ok(())
    }

    fn ensure_note_columns(&self) -> Result<()> {
        let columns = self.table_columns("notes")?;
        if !columns.iter().any(|column| column == "scope") {
            self.connection
                .execute(
                    "ALTER TABLE notes ADD COLUMN scope TEXT NOT NULL DEFAULT 'shared'",
                    [],
                )
                .context("failed to add notes.scope column")?;
        }
        self.connection
            .execute(
                "UPDATE notes
                 SET scope = 'shared'
                 WHERE scope IS NULL OR TRIM(scope) = ''",
                [],
            )
            .context("failed to backfill notes.scope column")?;
        Ok(())
    }

    fn ensure_open_file_columns(&self) -> Result<()> {
        let columns = self.table_columns("open_files")?;
        if !columns.iter().any(|column| column == "scope") {
            self.connection
                .execute(
                    "ALTER TABLE open_files ADD COLUMN scope TEXT NOT NULL DEFAULT 'shared'",
                    [],
                )
                .context("failed to add open_files.scope column")?;
        }
        self.connection
            .execute(
                "UPDATE open_files
                 SET scope = 'shared'
                 WHERE scope IS NULL OR TRIM(scope) = ''",
                [],
            )
            .context("failed to backfill open_files.scope column")?;
        Ok(())
    }

    fn table_columns(&self, table: &str) -> Result<Vec<String>> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .with_context(|| format!("failed to prepare {table} table_info query"))?;
        statement
            .query_map([], |row| row.get::<_, String>(1))
            .with_context(|| format!("failed to query {table} table_info"))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .with_context(|| format!("failed to decode {table} table_info rows"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::SHARED_SCOPE;
    use indexmap::IndexMap;
    use std::time::{Duration, UNIX_EPOCH};

    use crate::context::ContentBlock;

    #[test]
    fn storage_roundtrips_task_state() {
        let workdir = temp_workspace();
        let mut storage = MarchStorage::open(&workdir).expect("open storage");
        let task = storage.create_task("demo").expect("create task");

        let mut notes = IndexMap::new();
        notes.insert("target".to_string(), NoteEntry::new("实现上下文持久化"));
        notes.insert("plan".to_string(), NoteEntry::new("先落库再恢复"));

        storage
            .save_task_state(
                task.id,
                &PersistedTaskState {
                    active_agent: MARCH_AGENT_NAME.to_string(),
                    timeline: Some(vec![
                        PersistedTaskTimelineEntry::UserMessage(PersistedUserMessage {
                            user_message_id: "user-1".to_string(),
                            content: vec![ContentBlock::text("继续")],
                            mentions: Vec::new(),
                            replies: Vec::new(),
                            timestamp: UNIX_EPOCH + Duration::from_secs(1),
                        }),
                        PersistedTaskTimelineEntry::Turn(PersistedTurn {
                            turn_id: "turn-1".to_string(),
                            agent_id: MARCH_AGENT_NAME.to_string(),
                            trigger: PersistedTurnTrigger::User {
                                id: "user-1".to_string(),
                            },
                            state: PersistedTurnState::Done,
                            error_message: None,
                            timestamp: UNIX_EPOCH + Duration::from_secs(2),
                            messages: vec![PersistedAssistantMessage {
                                message_id: "message-1".to_string(),
                                turn_id: "turn-1".to_string(),
                                state: PersistedAssistantMessageState::Done,
                                reasoning: String::new(),
                                timeline: vec![PersistedAssistantTimelineEntry::Text {
                                    text: "开始实现持久化".to_string(),
                                }],
                            }],
                        }),
                    ]),
                    notes: notes
                        .into_iter()
                        .map(|(id, entry)| PersistedNote {
                            scope: SHARED_SCOPE.to_string(),
                            id,
                            entry,
                        })
                        .collect(),
                    open_files: vec![
                        PersistedOpenFile {
                            scope: SHARED_SCOPE.to_string(),
                            path: workdir.join("src").join("main.rs"),
                            locked: false,
                        },
                        PersistedOpenFile {
                            scope: SHARED_SCOPE.to_string(),
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
        assert_eq!(loaded.timeline.len(), 2);
        assert_eq!(loaded.notes.len(), 2);
        assert_eq!(loaded.open_files.len(), 2);
        assert!(loaded.open_files[1].locked);
        assert_eq!(loaded.hints.len(), 1);
    }

    #[test]
    fn opening_storage_creates_march_directory_and_db() {
        let workdir = temp_workspace();
        let storage = MarchStorage::open(&workdir).expect("open storage");

        assert!(workdir.join(".march").is_dir());
        assert!(storage.database_path().is_file());
    }

    #[test]
    fn task_title_metadata_roundtrips() {
        let workdir = temp_workspace();
        let storage = MarchStorage::open(&workdir).expect("open storage");
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
        let storage = MarchStorage::open(&workdir).expect("open storage");
        let task = storage
            .create_task_with_options("demo", {
                let mut options = TaskCreateOptions::new(workdir.clone());
                options.title_source = TaskTitleSource::Manual;
                options.title_locked = true;
                options.selected_model_config_id = Some(7);
                options.selected_model = Some("gpt-5.4".to_string());
                options
            })
            .expect("create task");

        let loaded = storage.load_task(task.id).expect("load task");
        assert_eq!(loaded.task.selected_model_config_id, Some(7));
        assert_eq!(loaded.task.selected_model.as_deref(), Some("gpt-5.4"));
        assert_eq!(loaded.task.model_temperature, None);
        assert_eq!(loaded.task.model_top_p, None);
        assert_eq!(loaded.task.model_presence_penalty, None);
        assert_eq!(loaded.task.model_frequency_penalty, None);
        assert_eq!(loaded.task.model_max_output_tokens, None);
    }

    #[test]
    fn backfill_only_updates_missing_task_defaults() {
        let workdir = temp_workspace();
        let storage = MarchStorage::open(&workdir).expect("open storage");
        let inherited = storage
            .create_task("inherited")
            .expect("create inherited task");
        let explicit = storage
            .create_task_with_options("explicit", {
                let mut options = TaskCreateOptions::new(workdir.clone());
                options.title_source = TaskTitleSource::Manual;
                options.title_locked = true;
                options.selected_model_config_id = Some(9);
                options.selected_model = Some("custom-model".to_string());
                options
            })
            .expect("create explicit task");

        storage
            .backfill_missing_task_defaults(Some(3), Some("gpt-5.3-codex".to_string()))
            .expect("backfill defaults");

        let inherited_loaded = storage.load_task(inherited.id).expect("load inherited");
        assert_eq!(inherited_loaded.task.selected_model_config_id, Some(3));
        assert_eq!(
            inherited_loaded.task.selected_model.as_deref(),
            Some("gpt-5.3-codex")
        );

        let explicit_loaded = storage.load_task(explicit.id).expect("load explicit");
        assert_eq!(explicit_loaded.task.selected_model_config_id, Some(9));
        assert_eq!(
            explicit_loaded.task.selected_model.as_deref(),
            Some("custom-model")
        );
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
