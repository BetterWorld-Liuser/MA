use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, Row, params};

use crate::agents::MARCH_AGENT_NAME;
use crate::context::{Hint, NoteEntry};

use super::codec::{
    decode_content_blocks, decode_working_directory, normalize_working_directory,
    optional_system_time, system_time_from_unix, unix_timestamp,
};
use super::{
    MarchStorage, PersistedAssistantMessage, PersistedAssistantMessageState,
    PersistedAssistantTimelineEntry, PersistedNote, PersistedOpenFile, PersistedReplyRef,
    PersistedTask, PersistedTaskTimeline, PersistedTaskTimelineEntry, PersistedToolCallState,
    PersistedTurn, PersistedTurnState, PersistedTurnTrigger, PersistedUserMessage, TaskRecord,
    TaskTitleSource,
};

#[derive(Debug, Clone)]
pub struct TaskCreateOptions {
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
}

impl TaskCreateOptions {
    pub fn new(working_directory: PathBuf) -> Self {
        Self {
            title_source: TaskTitleSource::Default,
            title_locked: false,
            working_directory,
            selected_model_config_id: None,
            selected_model: None,
            model_temperature: None,
            model_top_p: None,
            model_presence_penalty: None,
            model_frequency_penalty: None,
            model_max_output_tokens: None,
        }
    }
}

impl MarchStorage {
    pub fn create_task(&self, name: impl AsRef<str>) -> Result<TaskRecord> {
        self.create_task_with_options(name, TaskCreateOptions::new(self.workspace_root.clone()))
    }

    pub fn create_task_with_metadata(
        &self,
        name: impl AsRef<str>,
        title_source: TaskTitleSource,
        title_locked: bool,
    ) -> Result<TaskRecord> {
        let mut options = TaskCreateOptions::new(self.workspace_root.clone());
        options.title_source = title_source;
        options.title_locked = title_locked;
        self.create_task_with_options(name, options)
    }

    pub fn create_task_with_options(
        &self,
        name: impl AsRef<str>,
        options: TaskCreateOptions,
    ) -> Result<TaskRecord> {
        let name = name.as_ref().trim();
        if name.is_empty() {
            bail!("task name cannot be empty");
        }
        let TaskCreateOptions {
            title_source,
            title_locked,
            working_directory,
            selected_model_config_id,
            selected_model,
            model_temperature,
            model_top_p,
            model_presence_penalty,
            model_frequency_penalty,
            model_max_output_tokens,
        } = options;
        let normalized_model = normalize_model_id(selected_model);
        let working_directory = normalize_working_directory(&working_directory)?;
        let normalized_temperature = normalize_model_temperature(model_temperature)?;
        let normalized_top_p = normalize_model_top_p(model_top_p)?;
        let normalized_presence_penalty =
            normalize_model_penalty("model_presence_penalty", model_presence_penalty)?;
        let normalized_frequency_penalty =
            normalize_model_penalty("model_frequency_penalty", model_frequency_penalty)?;
        let normalized_max_output_tokens =
            normalize_model_max_output_tokens(model_max_output_tokens)?;

        let now = SystemTime::now();
        let now_ts = unix_timestamp(now)?;
        self.connection
            .execute(
                "INSERT INTO tasks (
                    name,
                    title_source,
                    title_locked,
                    working_directory,
                    selected_model_config_id,
                    selected_model,
                    model_temperature,
                    model_top_p,
                    model_presence_penalty,
                    model_frequency_penalty,
                    model_max_output_tokens,
                    active_agent,
                    last_event_seq,
                    created_at,
                    last_active
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    name,
                    title_source.as_db_value(),
                    if title_locked { 1 } else { 0 },
                    working_directory.to_string_lossy().to_string(),
                    selected_model_config_id,
                    normalized_model,
                    normalized_temperature,
                    normalized_top_p,
                    normalized_presence_penalty,
                    normalized_frequency_penalty,
                    normalized_max_output_tokens.map(i64::from),
                    MARCH_AGENT_NAME,
                    0_i64,
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
            selected_model_config_id,
            selected_model: normalized_model,
            model_temperature: normalized_temperature,
            model_top_p: normalized_top_p,
            model_presence_penalty: normalized_presence_penalty,
            model_frequency_penalty: normalized_frequency_penalty,
            model_max_output_tokens: normalized_max_output_tokens,
            active_agent: MARCH_AGENT_NAME.to_string(),
            last_event_seq: 0,
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

        require_task_found(affected, task_id)?;

        Ok(())
    }

    pub fn delete_task(&self, task_id: i64) -> Result<()> {
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("failed to start delete_task transaction")?;

        transaction
            .execute(
                "DELETE FROM task_message_timeline_entries WHERE task_id = ?1",
                params![task_id],
            )
            .context("failed to delete task message timeline entries")?;
        transaction
            .execute(
                "DELETE FROM task_turn_messages WHERE task_id = ?1",
                params![task_id],
            )
            .context("failed to delete task turn messages")?;
        transaction
            .execute(
                "DELETE FROM task_timeline_entries WHERE task_id = ?1",
                params![task_id],
            )
            .context("failed to delete task timeline entries")?;
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
        selected_model_config_id: Option<i64>,
        selected_model: Option<String>,
    ) -> Result<()> {
        let normalized = normalize_model_id(selected_model);

        let affected = self
            .connection
            .execute(
                "UPDATE tasks
                 SET selected_model_config_id = ?2, selected_model = ?3
                 WHERE id = ?1",
                params![task_id, selected_model_config_id, normalized],
            )
            .context("failed to update task selection")?;

        require_task_found(affected, task_id)?;

        Ok(())
    }

    pub fn load_task_last_event_seq(&self, task_id: i64) -> Result<u64> {
        let raw = self
            .connection
            .query_row(
                "SELECT last_event_seq FROM tasks WHERE id = ?1",
                params![task_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .context("failed to load task last_event_seq")?
            .with_context(|| format!("task {} not found", task_id))?;
        u64::try_from(raw).context("task last_event_seq must be non-negative")
    }

    pub fn update_task_last_event_seq(&self, task_id: i64, seq: u64) -> Result<()> {
        let affected = self
            .connection
            .execute(
                "UPDATE tasks SET last_event_seq = ?2 WHERE id = ?1",
                params![
                    task_id,
                    i64::try_from(seq).context("task event seq overflow")?
                ],
            )
            .context("failed to update task last_event_seq")?;
        require_task_found(affected, task_id)
    }

    pub fn update_task_model_settings(
        &self,
        task_id: i64,
        model_temperature: Option<f32>,
        model_top_p: Option<f32>,
        model_presence_penalty: Option<f32>,
        model_frequency_penalty: Option<f32>,
        model_max_output_tokens: Option<u32>,
    ) -> Result<()> {
        let normalized_temperature = normalize_model_temperature(model_temperature)?;
        let normalized_top_p = normalize_model_top_p(model_top_p)?;
        let normalized_presence_penalty =
            normalize_model_penalty("model_presence_penalty", model_presence_penalty)?;
        let normalized_frequency_penalty =
            normalize_model_penalty("model_frequency_penalty", model_frequency_penalty)?;
        let normalized_max_output_tokens =
            normalize_model_max_output_tokens(model_max_output_tokens)?;

        let affected = self
            .connection
            .execute(
                "UPDATE tasks
                 SET model_temperature = ?2,
                     model_top_p = ?3,
                     model_presence_penalty = ?4,
                     model_frequency_penalty = ?5,
                     model_max_output_tokens = ?6
                 WHERE id = ?1",
                params![
                    task_id,
                    normalized_temperature,
                    normalized_top_p,
                    normalized_presence_penalty,
                    normalized_frequency_penalty,
                    normalized_max_output_tokens.map(i64::from)
                ],
            )
            .context("failed to update task model settings")?;

        require_task_found(affected, task_id)?;

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

        require_task_found(affected, task_id)?;

        Ok(())
    }

    pub fn backfill_missing_task_defaults(
        &self,
        selected_model_config_id: Option<i64>,
        selected_model: Option<String>,
    ) -> Result<()> {
        let normalized_model = normalize_model_id(selected_model);

        self.connection
            .execute(
                "UPDATE tasks
                 SET selected_model_config_id = COALESCE(selected_model_config_id, ?1),
                     selected_model = COALESCE(selected_model, ?2)",
                params![selected_model_config_id, normalized_model],
            )
            .context("failed to backfill task defaults")?;

        Ok(())
    }

    pub fn update_task_active_agent(&self, task_id: i64, active_agent: &str) -> Result<()> {
        let active_agent = active_agent.trim();
        if active_agent.is_empty() {
            bail!("active_agent cannot be empty");
        }

        let affected = self
            .connection
            .execute(
                "UPDATE tasks
                 SET active_agent = ?2
                 WHERE id = ?1",
                params![task_id, active_agent],
            )
            .context("failed to update task active_agent")?;

        require_task_found(affected, task_id)?;

        Ok(())
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskRecord>> {
        let mut statement = self
            .connection
            .prepare(&format!(
                "SELECT {}
                     FROM tasks
                     ORDER BY last_active DESC, id DESC",
                TASK_RECORD_SELECT_COLUMNS
            ))
            .context("failed to prepare list_tasks query")?;

        let rows = statement
            .query_map([], RawTaskRecord::from_row)
            .context("failed to query tasks")?;

        let mut tasks = Vec::new();
        for row in rows {
            let raw = row.context("failed to decode task row")?;
            tasks.push(raw.decode(&self.workspace_root)?);
        }
        Ok(tasks)
    }

    pub fn load_task(&self, task_id: i64) -> Result<PersistedTask> {
        let task = self.load_task_record(task_id)?;
        let timeline = self.load_task_timeline(task_id)?;
        let notes = self.load_notes(task_id)?;
        let open_files = self.load_open_files(task_id)?;
        let hints = self.load_hints()?;

        Ok(PersistedTask {
            active_agent: task.active_agent.clone(),
            task,
            timeline,
            notes,
            open_files,
            hints,
        })
    }

    fn load_task_record(&self, task_id: i64) -> Result<TaskRecord> {
        let raw = self
            .connection
            .query_row(
                &format!(
                    "SELECT {}
                     FROM tasks
                     WHERE id = ?1",
                    TASK_RECORD_SELECT_COLUMNS
                ),
                params![task_id],
                RawTaskRecord::from_row,
            )
            .optional()
            .context("failed to load task row")?
            .with_context(|| format!("task {} not found", task_id))?;

        raw.decode(&self.workspace_root)
    }

    fn load_task_timeline(&self, task_id: i64) -> Result<PersistedTaskTimeline> {
        let mut entry_statement = self
            .connection
            .prepare(
                "SELECT kind, user_message_id, turn_id, content, mentions_json, replies_json, agent_id, trigger_json, state, error_message, created_at
                 FROM task_timeline_entries
                 WHERE task_id = ?1
                 ORDER BY position ASC, id ASC",
            )
            .context("failed to prepare task timeline query")?;

        let rows = entry_statement
            .query_map(params![task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, i64>(10)?,
                ))
            })
            .context("failed to query task timeline entries")?;

        let mut timeline = Vec::new();
        for row in rows {
            let (
                kind,
                user_message_id,
                turn_id,
                content,
                mentions_json,
                replies_json,
                agent_id,
                trigger_json,
                state,
                error_message,
                created_at,
            ) = row.context("failed to decode task timeline row")?;

            match kind.as_str() {
                "user_message" => {
                    timeline.push(PersistedTaskTimelineEntry::UserMessage(
                        PersistedUserMessage {
                            user_message_id: user_message_id.with_context(|| {
                                format!("missing user_message_id for task {}", task_id)
                            })?,
                            content: decode_content_blocks(content.as_deref().unwrap_or("[]"))?,
                            mentions: decode_json_list(mentions_json.as_deref())?,
                            replies: decode_reply_refs(replies_json.as_deref())?,
                            timestamp: system_time_from_unix(created_at)?,
                        },
                    ));
                }
                "turn" => {
                    let turn_id =
                        turn_id.with_context(|| format!("missing turn_id for task {}", task_id))?;
                    timeline.push(PersistedTaskTimelineEntry::Turn(PersistedTurn {
                        turn_id: turn_id.clone(),
                        agent_id: agent_id.unwrap_or_else(|| MARCH_AGENT_NAME.to_string()),
                        trigger: decode_turn_trigger(trigger_json.as_deref())?,
                        state: decode_turn_state(state.as_deref())?,
                        error_message,
                        timestamp: system_time_from_unix(created_at)?,
                        messages: self.load_turn_messages(task_id, &turn_id)?,
                    }));
                }
                other => bail!("unknown task timeline entry kind in database: {}", other),
            }
        }

        Ok(timeline)
    }

    fn load_turn_messages(
        &self,
        task_id: i64,
        turn_id: &str,
    ) -> Result<Vec<PersistedAssistantMessage>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT message_id, state, reasoning
                 FROM task_turn_messages
                 WHERE task_id = ?1 AND turn_id = ?2
                 ORDER BY position ASC, id ASC",
            )
            .context("failed to prepare turn messages query")?;

        let rows = statement
            .query_map(params![task_id, turn_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .context("failed to query turn messages")?;

        let mut messages = Vec::new();
        for row in rows {
            let (message_id, state, reasoning) =
                row.context("failed to decode turn message row")?;
            messages.push(PersistedAssistantMessage {
                message_id: message_id.clone(),
                turn_id: turn_id.to_string(),
                state: decode_message_state(&state)?,
                reasoning,
                timeline: self.load_message_timeline_entries(task_id, &message_id)?,
            });
        }

        Ok(messages)
    }

    fn load_message_timeline_entries(
        &self,
        task_id: i64,
        message_id: &str,
    ) -> Result<Vec<PersistedAssistantTimelineEntry>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT kind, text, tool_call_id, tool_name, arguments, status, preview, duration_ms
                 FROM task_message_timeline_entries
                 WHERE task_id = ?1 AND message_id = ?2
                 ORDER BY position ASC, id ASC",
            )
            .context("failed to prepare message timeline entries query")?;

        let rows = statement
            .query_map(params![task_id, message_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            })
            .context("failed to query message timeline entries")?;

        let mut entries = Vec::new();
        for row in rows {
            let (kind, text, tool_call_id, tool_name, arguments, status, preview, duration_ms) =
                row.context("failed to decode message timeline entry row")?;
            match kind.as_str() {
                "text" => entries.push(PersistedAssistantTimelineEntry::Text {
                    text: text.unwrap_or_default(),
                }),
                "tool" => entries.push(PersistedAssistantTimelineEntry::Tool {
                    tool_call_id: tool_call_id.unwrap_or_default(),
                    tool_name: tool_name.unwrap_or_default(),
                    arguments: arguments.unwrap_or_default(),
                    status: decode_tool_call_state(status.as_deref())?,
                    preview,
                    duration_ms: duration_ms.and_then(|value| u64::try_from(value).ok()),
                }),
                other => bail!("unknown message timeline entry kind in database: {}", other),
            }
        }

        Ok(entries)
    }

    fn load_notes(&self, task_id: i64) -> Result<Vec<PersistedNote>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT scope, note_id, content
                 FROM notes
                 WHERE task_id = ?1
                 ORDER BY scope = 'shared' DESC, position ASC, note_id ASC",
            )
            .context("failed to prepare notes query")?;

        let rows = statement
            .query_map(params![task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .context("failed to query notes")?;

        let mut notes = Vec::new();
        for row in rows {
            let (scope, note_id, content) = row.context("failed to decode note row")?;
            notes.push(PersistedNote {
                scope,
                id: note_id,
                entry: NoteEntry::new(content),
            });
        }
        Ok(notes)
    }

    fn load_open_files(&self, task_id: i64) -> Result<Vec<PersistedOpenFile>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT scope, path, locked
                 FROM open_files
                 WHERE task_id = ?1
                 ORDER BY scope = 'shared' DESC, position ASC, path ASC",
            )
            .context("failed to prepare open_files query")?;

        let rows = statement
            .query_map(params![task_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .context("failed to query open files")?;

        let mut open_files = Vec::new();
        for row in rows {
            let (scope, path, locked) = row.context("failed to decode open file row")?;
            open_files.push(PersistedOpenFile {
                scope,
                path: PathBuf::from(path),
                locked: locked != 0,
            });
        }
        Ok(open_files)
    }

    pub(super) fn load_hints(&self) -> Result<Vec<Hint>> {
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

const TASK_RECORD_SELECT_COLUMNS: &str = "id, name, title_source, title_locked, working_directory, created_at, last_active, last_event_seq, \
     selected_model_config_id, selected_model, model_temperature, model_top_p, \
     model_presence_penalty, model_frequency_penalty, model_max_output_tokens, active_agent";

struct RawTaskRecord {
    id: i64,
    name: String,
    title_source: String,
    title_locked: i64,
    working_directory: Option<String>,
    created_at: i64,
    last_active: i64,
    last_event_seq: i64,
    selected_model_config_id: Option<i64>,
    selected_model: Option<String>,
    model_temperature: Option<f32>,
    model_top_p: Option<f32>,
    model_presence_penalty: Option<f32>,
    model_frequency_penalty: Option<f32>,
    model_max_output_tokens: Option<i64>,
    active_agent: String,
}

impl RawTaskRecord {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get("id")?,
            name: row.get("name")?,
            title_source: row.get("title_source")?,
            title_locked: row.get("title_locked")?,
            working_directory: row.get("working_directory")?,
            created_at: row.get("created_at")?,
            last_active: row.get("last_active")?,
            last_event_seq: row.get("last_event_seq")?,
            selected_model_config_id: row.get("selected_model_config_id")?,
            selected_model: row.get("selected_model")?,
            model_temperature: row.get("model_temperature")?,
            model_top_p: row.get("model_top_p")?,
            model_presence_penalty: row.get("model_presence_penalty")?,
            model_frequency_penalty: row.get("model_frequency_penalty")?,
            model_max_output_tokens: row.get("model_max_output_tokens")?,
            active_agent: row.get("active_agent")?,
        })
    }

    fn decode(self, workspace_root: &PathBuf) -> Result<TaskRecord> {
        Ok(TaskRecord {
            id: self.id,
            name: self.name,
            title_source: TaskTitleSource::from_db_value(&self.title_source)?,
            title_locked: self.title_locked != 0,
            working_directory: decode_working_directory(self.working_directory, workspace_root)?,
            selected_model_config_id: self.selected_model_config_id,
            selected_model: self.selected_model,
            model_temperature: self.model_temperature,
            model_top_p: self.model_top_p,
            model_presence_penalty: self.model_presence_penalty,
            model_frequency_penalty: self.model_frequency_penalty,
            model_max_output_tokens: self
                .model_max_output_tokens
                .and_then(|value| u32::try_from(value).ok()),
            active_agent: self.active_agent,
            last_event_seq: u64::try_from(self.last_event_seq)
                .context("task last_event_seq must be non-negative")?,
            created_at: system_time_from_unix(self.created_at)?,
            last_active: system_time_from_unix(self.last_active)?,
        })
    }
}

fn normalize_model_id(model: Option<String>) -> Option<String> {
    model.and_then(|model| {
        let trimmed = model.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn decode_json_list(raw: Option<&str>) -> Result<Vec<String>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    serde_json::from_str(raw).context("failed to decode string list from json")
}

fn decode_reply_refs(raw: Option<&str>) -> Result<Vec<PersistedReplyRef>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    serde_json::from_str(raw).context("failed to decode reply refs from json")
}

fn decode_turn_trigger(raw: Option<&str>) -> Result<PersistedTurnTrigger> {
    let Some(raw) = raw else {
        return Ok(PersistedTurnTrigger::User {
            id: "legacy-missing-trigger".to_string(),
        });
    };
    serde_json::from_str(raw).context("failed to decode turn trigger from json")
}

fn decode_message_state(raw: &str) -> Result<PersistedAssistantMessageState> {
    match raw {
        "streaming" => Ok(PersistedAssistantMessageState::Streaming),
        "done" => Ok(PersistedAssistantMessageState::Done),
        other => bail!("unknown message state in database: {}", other),
    }
}

fn decode_turn_state(raw: Option<&str>) -> Result<PersistedTurnState> {
    match raw.unwrap_or("done") {
        "streaming" => Ok(PersistedTurnState::Streaming),
        "done" => Ok(PersistedTurnState::Done),
        "failed" => Ok(PersistedTurnState::Failed),
        "cancelled" => Ok(PersistedTurnState::Cancelled),
        other => bail!("unknown turn state in database: {}", other),
    }
}

fn decode_tool_call_state(raw: Option<&str>) -> Result<PersistedToolCallState> {
    match raw.unwrap_or("ok") {
        "running" => Ok(PersistedToolCallState::Running),
        "ok" => Ok(PersistedToolCallState::Ok),
        "error" => Ok(PersistedToolCallState::Error),
        other => bail!("unknown tool call state in database: {}", other),
    }
}

fn require_task_found(affected: usize, task_id: i64) -> Result<()> {
    if affected == 0 {
        bail!("task {} not found", task_id);
    }
    Ok(())
}

fn normalize_model_temperature(value: Option<f32>) -> Result<Option<f32>> {
    match value {
        Some(value) if !value.is_finite() => bail!("model_temperature must be finite"),
        Some(value) if !(0.0..=2.0).contains(&value) => {
            bail!("model_temperature must be between 0.0 and 2.0")
        }
        Some(value) => Ok(Some((value * 100.0).round() / 100.0)),
        None => Ok(None),
    }
}

fn normalize_model_max_output_tokens(value: Option<u32>) -> Result<Option<u32>> {
    match value {
        Some(0) => bail!("model_max_output_tokens must be greater than 0"),
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

fn normalize_model_top_p(value: Option<f32>) -> Result<Option<f32>> {
    match value {
        Some(value) if !value.is_finite() => bail!("model_top_p must be finite"),
        Some(value) if !(0.0..=1.0).contains(&value) => {
            bail!("model_top_p must be between 0.0 and 1.0")
        }
        Some(value) => Ok(Some((value * 100.0).round() / 100.0)),
        None => Ok(None),
    }
}

fn normalize_model_penalty(field: &str, value: Option<f32>) -> Result<Option<f32>> {
    match value {
        Some(value) if !value.is_finite() => bail!("{field} must be finite"),
        Some(value) if !(-2.0..=2.0).contains(&value) => {
            bail!("{field} must be between -2.0 and 2.0")
        }
        Some(value) => Ok(Some((value * 100.0).round() / 100.0)),
        None => Ok(None),
    }
}
