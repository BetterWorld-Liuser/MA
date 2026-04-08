use serde::{Deserialize, Serialize};

use super::runtime::{UiRuntimeSnapshot, UiTaskSnapshot};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiAgentProgressEvent {
    TurnStarted {
        task_id: i64,
        turn_id: String,
        user_message: String,
        agent: String,
        agent_display_name: String,
    },
    Status {
        task_id: i64,
        turn_id: String,
        agent: String,
        agent_display_name: String,
        phase: UiAgentStatusPhase,
        label: String,
        runtime: UiRuntimeSnapshot,
    },
    ToolStarted {
        task_id: i64,
        turn_id: String,
        tool_call_id: String,
        tool_name: String,
        summary: String,
        runtime: UiRuntimeSnapshot,
    },
    ToolFinished {
        task_id: i64,
        turn_id: String,
        tool_call_id: String,
        status: UiAgentToolStatus,
        summary: String,
        preview: Option<String>,
        runtime: UiRuntimeSnapshot,
    },
    AssistantTextPreview {
        task_id: i64,
        turn_id: String,
        agent: String,
        agent_display_name: String,
        message: String,
        runtime: UiRuntimeSnapshot,
    },
    AssistantMessageCheckpoint {
        task_id: i64,
        turn_id: String,
        agent: String,
        agent_display_name: String,
        message_id: String,
        content: String,
        checkpoint_type: UiAssistantMessageCheckpointType,
        runtime: UiRuntimeSnapshot,
    },
    FinalAssistantMessage {
        task_id: i64,
        turn_id: String,
        task: UiTaskSnapshot,
    },
    RoundComplete {
        task_id: i64,
        turn_id: String,
        task: UiTaskSnapshot,
    },
    TurnFailed {
        task_id: i64,
        turn_id: String,
        stage: UiAgentFailureStage,
        message: String,
        retryable: bool,
    },
    TurnCancelled {
        task_id: i64,
        turn_id: String,
        task: UiTaskSnapshot,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAgentStatusPhase {
    BuildingContext,
    WaitingModel,
    RunningTool,
    Streaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAgentToolStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAssistantMessageCheckpointType {
    Intermediate,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAgentFailureStage {
    Context,
    Tool,
    Provider,
    Internal,
}
