use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

mod backend;
mod provider;
mod util;
mod view;
mod workspace;

pub use provider::{
    fetch_probe_models, fetch_provider_models_for_provider, fetch_provider_models_for_task,
    fetch_task_model_selector, test_provider_connection,
};

const DEFAULT_TASK_NAME: &str = "默认任务";
const UI_MAX_RECENT_TURNS: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiCreateTaskRequest {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSelectTaskRequest {
    pub task_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDeleteTaskRequest {
    pub task_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSendMessageRequest {
    pub task_id: Option<i64>,
    pub content_blocks: Vec<UiComposerContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiComposerContentBlock {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        data_base64: String,
        source_path: Option<PathBuf>,
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiOpenFilesRequest {
    pub task_id: Option<i64>,
    pub paths: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSetTaskModelRequest {
    pub task_id: Option<i64>,
    pub provider_id: Option<i64>,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSetTaskModelSettingsRequest {
    pub task_id: Option<i64>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSetTaskWorkingDirectoryRequest {
    pub task_id: Option<i64>,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSearchWorkspaceEntriesRequest {
    pub task_id: Option<i64>,
    pub query: String,
    pub kind: Option<UiWorkspaceEntryKind>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSearchSkillsRequest {
    pub task_id: Option<i64>,
    pub query: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiUpsertNoteRequest {
    pub task_id: Option<i64>,
    pub note_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDeleteNoteRequest {
    pub task_id: Option<i64>,
    pub note_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiToggleOpenFileLockRequest {
    pub task_id: Option<i64>,
    pub path: PathBuf,
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiCloseOpenFileRequest {
    pub task_id: Option<i64>,
    pub path: PathBuf,
}

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
pub struct UiProviderModelsView {
    pub current_model: String,
    pub available_models: Vec<String>,
    pub suggested_models: Vec<String>,
    pub provider_cache_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiProviderModelGroupView {
    pub provider_id: Option<i64>,
    pub provider_name: String,
    pub provider_type: String,
    pub provider_cache_key: String,
    pub available_models: Vec<String>,
    pub suggested_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiTaskModelSelectorView {
    pub current_provider_id: Option<i64>,
    pub current_model: String,
    pub current_temperature: Option<f32>,
    pub current_top_p: Option<f32>,
    pub current_presence_penalty: Option<f32>,
    pub current_frequency_penalty: Option<f32>,
    pub current_max_output_tokens: Option<u32>,
    pub current_model_capabilities: UiModelCapabilitiesView,
    pub providers: Vec<UiProviderModelGroupView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiProviderSettingsView {
    pub database_path: PathBuf,
    pub providers: Vec<UiProviderView>,
    pub agents: Vec<UiAgentProfileView>,
    pub default_provider_id: Option<i64>,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiAgentProfileView {
    pub id: Option<i64>,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub system_prompt: String,
    pub avatar_color: String,
    pub provider_id: Option<i64>,
    pub model_id: Option<String>,
    pub is_built_in: bool,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiModelCapabilitiesView {
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub supports_tool_use: bool,
    pub supports_vision: bool,
    pub supports_audio: bool,
    pub supports_pdf: bool,
    pub server_tools: Vec<UiServerToolView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiProviderView {
    pub id: i64,
    pub name: String,
    pub provider_type: String,
    pub base_url: Option<String>,
    pub api_key_hint: String,
    pub created_at: i64,
    pub models: Vec<UiProviderModelView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiProviderModelView {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub capabilities: UiModelCapabilitiesView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiServerToolView {
    pub capability: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiUpsertProviderRequest {
    pub id: Option<i64>,
    pub provider_type: String,
    pub name: String,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiUpsertProviderModelRequest {
    pub id: Option<i64>,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: String,
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub supports_tool_use: bool,
    pub supports_vision: bool,
    pub supports_audio: bool,
    pub supports_pdf: bool,
    pub server_tools: Vec<UiServerToolView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDeleteProviderRequest {
    pub provider_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDeleteProviderModelRequest {
    pub provider_model_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSetDefaultProviderRequest {
    pub provider_id: Option<i64>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiUpsertAgentRequest {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub system_prompt: String,
    pub avatar_color: Option<String>,
    pub provider_id: Option<i64>,
    pub model_id: Option<String>,
    pub use_custom_march_prompt: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDeleteAgentRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiRestoreMarchPromptRequest;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiTestProviderConnectionRequest {
    pub id: Option<i64>,
    pub provider_type: String,
    pub name: String,
    pub api_key: String,
    pub base_url: String,
    pub probe_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiProbeProviderModelsRequest {
    pub id: Option<i64>,
    pub provider_type: String,
    pub api_key: String,
    pub base_url: String,
    pub probe_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTestProviderConnectionResult {
    pub success: bool,
    pub message: String,
    pub suggested_model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiWorkspaceEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiWorkspaceEntryView {
    pub path: String,
    pub kind: UiWorkspaceEntryKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiMentionTargetView {
    Agent {
        name: String,
        display_name: String,
        description: String,
        avatar_color: String,
        source: String,
    },
    File {
        path: String,
    },
    Directory {
        path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLoadWorkspaceImageRequest {
    pub task_id: Option<i64>,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiWorkspaceImageView {
    pub path: PathBuf,
    pub media_type: String,
    pub data_url: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTaskSnapshot {
    pub task: UiTaskSummary,
    pub active_agent: String,
    pub history: Vec<UiTurnView>,
    pub notes: Vec<UiNoteView>,
    pub open_files: Vec<UiOpenFileView>,
    pub hints: Vec<UiHintView>,
    pub system_status: UiSystemStatusView,
    pub runtime: Option<UiRuntimeSnapshot>,
    pub debug_trace: Option<UiDebugTraceView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiRuntimeSnapshot {
    pub working_directory: PathBuf,
    pub available_shells: Vec<UiShellView>,
    pub open_files: Vec<UiFileSnapshotView>,
    pub skills: Vec<UiSkillView>,
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
pub struct UiTurnView {
    pub role: UiRoleView,
    pub agent: String,
    pub agent_display_name: String,
    pub content: String,
    pub images: Vec<UiImageAttachmentView>,
    pub tool_summaries: Vec<UiToolSummaryView>,
    pub timestamp: i64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiRoleView {
    System,
    User,
    Assistant,
    Tool,
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

pub struct UiAppBackend {
    workspace_path: PathBuf,
    storage: crate::storage::MaStorage,
}
