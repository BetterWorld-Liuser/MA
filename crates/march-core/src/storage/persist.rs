use std::time::SystemTime;

use anyhow::{Context, Result};
use rusqlite::{Transaction, params};

use crate::context::Hint;

use super::codec::{encode_content_blocks, optional_unix_timestamp, unix_timestamp};
use super::{
    PersistedAssistantMessageState, PersistedAssistantTimelineEntry, PersistedNote,
    PersistedOpenFile, PersistedTaskTimelineEntry, PersistedToolCallState, PersistedTurnState,
};

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

pub fn replace_notes(
    transaction: &Transaction<'_>,
    task_id: i64,
    notes: &[PersistedNote],
) -> Result<()> {
    transaction
        .execute("DELETE FROM notes WHERE task_id = ?1", params![task_id])
        .context("failed to clear notes")?;

    let mut insert = transaction
        .prepare(
            "INSERT INTO notes (task_id, scope, note_id, content, position)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .context("failed to prepare note insert")?;

    for (position, note) in notes.iter().enumerate() {
        insert
            .execute(params![
                task_id,
                note.scope,
                note.id,
                note.entry.content,
                position as i64
            ])
            .context("failed to insert note")?;
    }
    Ok(())
}

pub fn replace_task_timeline(
    transaction: &Transaction<'_>,
    task_id: i64,
    timeline: &[PersistedTaskTimelineEntry],
) -> Result<()> {
    transaction
        .execute(
            "DELETE FROM task_message_timeline_entries WHERE task_id = ?1",
            params![task_id],
        )
        .context("failed to clear task message timeline entries")?;
    transaction
        .execute(
            "DELETE FROM task_turn_messages WHERE task_id = ?1",
            params![task_id],
        )
        .context("failed to clear task turn messages")?;
    transaction
        .execute(
            "DELETE FROM task_timeline_entries WHERE task_id = ?1",
            params![task_id],
        )
        .context("failed to clear task timeline entries")?;

    let mut insert_entry = transaction
        .prepare(
            "INSERT INTO task_timeline_entries
             (task_id, position, kind, user_message_id, turn_id, content, mentions_json, replies_json, agent_id, trigger_json, state, error_message, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .context("failed to prepare timeline entry insert")?;
    let mut insert_message = transaction
        .prepare(
            "INSERT INTO task_turn_messages
             (task_id, turn_id, message_id, state, reasoning, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .context("failed to prepare turn message insert")?;
    let mut insert_message_entry = transaction
        .prepare(
            "INSERT INTO task_message_timeline_entries
             (task_id, message_id, kind, text, tool_call_id, tool_name, arguments, status, preview, duration_ms, position)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )
        .context("failed to prepare message timeline entry insert")?;

    for (position, entry) in timeline.iter().enumerate() {
        match entry {
            PersistedTaskTimelineEntry::UserMessage(message) => {
                insert_entry
                    .execute(params![
                        task_id,
                        position as i64,
                        "user_message",
                        message.user_message_id,
                        Option::<String>::None,
                        encode_content_blocks(&message.content)?,
                        serde_json::to_string(&message.mentions)
                            .context("failed to encode mentions")?,
                        serde_json::to_string(&message.replies)
                            .context("failed to encode replies")?,
                        Option::<String>::None,
                        Option::<String>::None,
                        Option::<String>::None,
                        Option::<String>::None,
                        unix_timestamp(message.timestamp)?,
                    ])
                    .context("failed to insert user timeline entry")?;
            }
            PersistedTaskTimelineEntry::Turn(turn) => {
                insert_entry
                    .execute(params![
                        task_id,
                        position as i64,
                        "turn",
                        Option::<String>::None,
                        turn.turn_id,
                        Option::<String>::None,
                        Option::<String>::None,
                        Option::<String>::None,
                        turn.agent_id,
                        serde_json::to_string(&turn.trigger)
                            .context("failed to encode turn trigger")?,
                        encode_turn_state(turn.state),
                        turn.error_message,
                        unix_timestamp(turn.timestamp)?,
                    ])
                    .context("failed to insert turn timeline entry")?;

                for (message_position, message) in turn.messages.iter().enumerate() {
                    insert_message
                        .execute(params![
                            task_id,
                            turn.turn_id,
                            message.message_id,
                            encode_message_state(message.state),
                            message.reasoning,
                            message_position as i64,
                        ])
                        .context("failed to insert turn message")?;

                    for (timeline_position, timeline_entry) in message.timeline.iter().enumerate() {
                        match timeline_entry {
                            PersistedAssistantTimelineEntry::Text { text } => {
                                insert_message_entry
                                    .execute(params![
                                        task_id,
                                        message.message_id,
                                        "text",
                                        text,
                                        Option::<String>::None,
                                        Option::<String>::None,
                                        Option::<String>::None,
                                        Option::<String>::None,
                                        Option::<String>::None,
                                        Option::<i64>::None,
                                        timeline_position as i64,
                                    ])
                                    .context("failed to insert text timeline entry")?;
                            }
                            PersistedAssistantTimelineEntry::Tool {
                                tool_call_id,
                                tool_name,
                                arguments,
                                status,
                                preview,
                                duration_ms,
                            } => {
                                insert_message_entry
                                    .execute(params![
                                        task_id,
                                        message.message_id,
                                        "tool",
                                        Option::<String>::None,
                                        tool_call_id,
                                        tool_name,
                                        arguments,
                                        encode_tool_call_state(*status),
                                        preview,
                                        duration_ms.map(|value| value as i64),
                                        timeline_position as i64,
                                    ])
                                    .context("failed to insert tool timeline entry")?;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn encode_message_state(state: PersistedAssistantMessageState) -> &'static str {
    match state {
        PersistedAssistantMessageState::Streaming => "streaming",
        PersistedAssistantMessageState::Done => "done",
    }
}

fn encode_turn_state(state: PersistedTurnState) -> &'static str {
    match state {
        PersistedTurnState::Streaming => "streaming",
        PersistedTurnState::Done => "done",
        PersistedTurnState::Failed => "failed",
        PersistedTurnState::Cancelled => "cancelled",
    }
}

fn encode_tool_call_state(state: PersistedToolCallState) -> &'static str {
    match state {
        PersistedToolCallState::Running => "running",
        PersistedToolCallState::Ok => "ok",
        PersistedToolCallState::Error => "error",
    }
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
            "INSERT INTO open_files (task_id, scope, path, position, locked)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .context("failed to prepare open_file insert")?;

    for (position, open_file) in open_files.iter().enumerate() {
        insert
            .execute(params![
                task_id,
                open_file.scope,
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
