use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::agents::MARCH_AGENT_NAME;
use crate::context::{ConversationHistory, DisplayTurn, Hint, NoteEntry};

use super::codec::{
    decode_content_blocks, decode_tool_summaries, decode_working_directory,
    normalize_working_directory, optional_system_time, role_from_db, system_time_from_unix,
    unix_timestamp,
};
use super::{
    MaStorage, PersistedNote, PersistedOpenFile, PersistedTask, TaskRecord, TaskTitleSource,
};

impl MaStorage {
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
                    active_agent,
                    created_at,
                    last_active
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    name,
                    title_source.as_db_value(),
                    if title_locked { 1 } else { 0 },
                    working_directory.to_string_lossy().to_string(),
                    selected_provider_id,
                    normalized_model,
                    MARCH_AGENT_NAME,
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
            active_agent: MARCH_AGENT_NAME.to_string(),
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

        if affected == 0 {
            bail!("task {} not found", task_id);
        }

        Ok(())
    }

    pub fn list_tasks(&self) -> Result<Vec<TaskRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, title_source, title_locked, working_directory, created_at, last_active
                 , selected_provider_id, selected_model, active_agent
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
                    row.get::<_, String>(9)?,
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
                active_agent,
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
                active_agent,
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
            active_agent: task.active_agent.clone(),
            task,
            history,
            notes,
            open_files,
            hints,
        })
    }

    fn load_task_record(&self, task_id: i64) -> Result<TaskRecord> {
        let raw = self
            .connection
            .query_row(
                "SELECT id, name, title_source, title_locked, working_directory, created_at, last_active
                 , selected_provider_id, selected_model, active_agent
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
                        row.get::<_, String>(9)?,
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
            active_agent: raw.9,
            created_at: system_time_from_unix(raw.5)?,
            last_active: system_time_from_unix(raw.6)?,
        })
    }

    fn load_conversation_history(&self, task_id: i64) -> Result<ConversationHistory> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT role, agent, content, tool_summaries, created_at
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
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .context("failed to query conversation history")?;

        let mut turns = Vec::new();
        for row in rows {
            let (role, agent, content, tool_summaries_json, created_at) =
                row.context("failed to decode conversation row")?;
            turns.push(DisplayTurn {
                role: role_from_db(&role)?,
                agent,
                content: decode_content_blocks(&content)?,
                tool_calls: decode_tool_summaries(tool_summaries_json.as_deref())?,
                timestamp: system_time_from_unix(created_at)?,
            });
        }
        Ok(ConversationHistory { turns })
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
