use std::time::SystemTime;

use anyhow::{Context, Result};
use indexmap::IndexMap;
use rusqlite::{Transaction, params};

use crate::context::{ConversationHistory, Hint, NoteEntry};

use super::PersistedOpenFile;
use super::codec::{encode_tool_summaries, optional_unix_timestamp, role_to_db, unix_timestamp};

pub fn update_task_last_active(
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

pub fn replace_conversation_history(
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

pub fn replace_notes(
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

pub fn replace_open_files(
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

pub fn replace_hints(transaction: &Transaction<'_>, hints: &[Hint]) -> Result<()> {
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
