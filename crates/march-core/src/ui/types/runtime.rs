use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{UiReplyRef, UiTurnTrigger};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiWorkspaceSnapshot {
    pub workspace_path: PathBuf,
    pub database_path: PathBuf,
    pub tasks: Vec<UiTaskSummary>,
    pub active_task: Option<UiTaskSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTaskSummary {
    pub id: i64,
    pub name: String,
    pub title_source: String,
    pub title_locked: bool,
    pub working_directory: PathBuf,
    pub selected_model: Option<String>,
    pub model_temperature: Option<f32>,
    pub model_top_p: Option<f32>,
    pub model_presence_penalty: Option<f32>,
    pub model_frequency_penalty: Option<f32>,
    pub model_max_output_tokens: Option<u32>,
    pub created_at: i64,
    pub last_active: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTaskSnapshot {
    pub task: UiTaskSummary,
    pub active_agent: String,
    pub last_seq: u64,
    pub timeline: Vec<UiTaskTimelineEntry>,
    pub notes: Vec<UiNoteView>,
    pub open_files: Vec<UiOpenFileView>,
    pub hints: Vec<UiHintView>,
    pub system_status: UiSystemStatusView,
    pub runtime: Option<UiRuntimeSnapshot>,
    pub debug_trace: Option<UiDebugTraceView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiTaskTimelineEntry {
    UserMessage(UiUserMessageView),
    Turn(UiTimelineTurnView),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTaskHistoryView {
    pub timeline: Vec<UiTaskTimelineEntry>,
    pub last_seq: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiUserMessageView {
    pub user_message_id: String,
    pub content: String,
    pub images: Vec<UiImageAttachmentView>,
    pub mentions: Vec<String>,
    pub replies: Vec<UiReplyRef>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTimelineTurnView {
    pub turn_id: String,
    pub agent_id: String,
    pub agent_display_name: String,
    pub trigger: UiTurnTrigger,
    pub state: String,
    pub error_message: Option<String>,
    pub timestamp: i64,
    pub messages: Vec<UiAssistantMessageView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiAssistantMessageView {
    pub message_id: String,
    pub turn_id: String,
    pub state: String,
    pub reasoning: String,
    pub timeline: Vec<UiAssistantTimelineEntryView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiAssistantTimelineEntryView {
    Text {
        text: String,
    },
    Tool {
        tool_call_id: String,
        tool_name: String,
        arguments: String,
        status: String,
        preview: Option<String>,
        duration_ms: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiRuntimeSnapshot {
    pub working_directory: PathBuf,
    pub available_shells: Vec<UiShellView>,
    pub open_files: Vec<UiFileSnapshotView>,
    pub skills: Vec<UiSkillView>,
    pub memories: Vec<UiMemoryEntryView>,
    pub memory_warnings: Vec<String>,
    pub system_status: UiSystemStatusView,
    pub context_usage: UiContextUsageView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiDebugTraceView {
    pub rounds: Vec<UiDebugRoundView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiDebugRoundView {
    pub iteration: usize,
    pub context_preview: String,
    pub provider_request_json: String,
    pub provider_response_json: String,
    pub provider_response_raw: String,
    pub tool_calls: Vec<UiDebugToolCallView>,
    pub tool_results: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiDebugToolCallView {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiShellView {
    pub kind: String,
    pub program: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSkillView {
    pub name: String,
    pub path: PathBuf,
    pub description: String,
    pub opened: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiMemoryEntryView {
    pub id: String,
    pub memory_type: String,
    pub topic: String,
    pub title: String,
    pub level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiMemoryDetailView {
    pub id: String,
    pub memory_type: String,
    pub topic: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub scope: String,
    pub level: String,
    pub access_count: u32,
    pub skip_count: u32,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSkillSearchView {
    pub kind: String,
    pub name: String,
    pub path: PathBuf,
    pub description: String,
    pub opened: bool,
    pub auto_triggered: bool,
    pub trigger_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiImageAttachmentView {
    pub id: String,
    pub name: String,
    pub media_type: String,
    pub data_url: String,
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiToolSummaryView {
    pub name: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiNoteView {
    pub scope: String,
    pub id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiHintView {
    pub content: String,
    pub expires_at: Option<i64>,
    pub turns_remaining: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiOpenFileView {
    pub scope: String,
    pub path: PathBuf,
    pub locked: bool,
    pub snapshot: Option<UiFileSnapshotView>,
}

impl UiOpenFileView {
    pub fn from_persisted(open_file: crate::storage::PersistedOpenFile) -> Self {
        open_file.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiFileSnapshotView {
    Available {
        path: PathBuf,
        content: String,
        last_modified_at: i64,
        modified_by: UiModifiedByView,
    },
    Deleted {
        path: PathBuf,
        last_seen_at: i64,
        modified_by: UiModifiedByView,
    },
    Moved {
        path: PathBuf,
        new_path: PathBuf,
        last_seen_at: i64,
        modified_by: UiModifiedByView,
    },
}

impl UiFileSnapshotView {
    pub fn path(&self) -> &Path {
        match self {
            Self::Available { path, .. }
            | Self::Deleted { path, .. }
            | Self::Moved { path, .. } => path.as_path(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiModifiedByView {
    Agent,
    User,
    External,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSystemStatusView {
    pub locked_files: Vec<PathBuf>,
    pub context_pressure: Option<UiContextPressureView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiContextPressureView {
    pub used_percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiContextUsageView {
    pub used_tokens: usize,
    pub budget_tokens: usize,
    pub used_percent: u8,
    pub sections: Vec<UiContextUsageSectionView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiContextUsageSectionView {
    pub name: String,
    pub tokens: usize,
}
