use serde::{Deserialize, Serialize};

use super::runtime::{UiDebugRoundView, UiRuntimeSnapshot, UiTaskSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiAgentProgressEvent {
    UserMessageAppended {
        task_id: i64,
        seq: u64,
        user_message_id: String,
        content: String,
        ts: i64,
        mentions: Vec<String>,
        replies: Vec<UiReplyRef>,
    },
    TurnStarted {
        task_id: i64,
        seq: u64,
        turn_id: String,
        agent: String,
        agent_display_name: String,
        trigger: UiTurnTrigger,
    },
    MessageStarted {
        task_id: i64,
        seq: u64,
        turn_id: String,
        message_id: String,
        runtime: UiRuntimeSnapshot,
    },
    ToolStarted {
        task_id: i64,
        seq: u64,
        turn_id: String,
        message_id: String,
        tool_call_id: String,
        tool_name: String,
        summary: String,
        runtime: UiRuntimeSnapshot,
    },
    ToolFinished {
        task_id: i64,
        seq: u64,
        turn_id: String,
        message_id: String,
        tool_call_id: String,
        status: UiAgentToolStatus,
        summary: String,
        preview: Option<String>,
        detail: Option<String>,
        runtime: UiRuntimeSnapshot,
    },
    AssistantStreamDelta {
        task_id: i64,
        seq: u64,
        turn_id: String,
        message_id: String,
        field: UiAssistantStreamField,
        delta: String,
        tool_call_id: Option<String>,
        runtime: UiRuntimeSnapshot,
    },
    MessageFinished {
        task_id: i64,
        seq: u64,
        turn_id: String,
        message_id: String,
        runtime: UiRuntimeSnapshot,
    },
    TurnFinished {
        task_id: i64,
        seq: u64,
        turn_id: String,
        reason: UiTurnFinishedReason,
        error_message: Option<String>,
        task: UiTaskSnapshot,
    },
    RoundComplete {
        task_id: i64,
        seq: u64,
        turn_id: String,
        debug_round: UiDebugRoundView,
        task: UiTaskSnapshot,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiReplyRef {
    Turn { id: String },
    UserMessage { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiTurnTrigger {
    User { id: String },
    Turn { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAgentToolStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAssistantStreamField {
    Reasoning,
    Content,
    ToolCallArguments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiTurnFinishedReason {
    Idle,
    Failed,
    Cancelled,
}
