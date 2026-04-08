use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::provider::UiServerToolView;

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
    pub model_config_id: i64,
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
pub struct UiListMemoriesRequest {
    pub task_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiGetMemoryRequest {
    pub task_id: Option<i64>,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiUpsertMemoryRequest {
    pub task_id: Option<i64>,
    pub id: String,
    pub memory_type: String,
    pub topic: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub scope: Option<String>,
    pub level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDeleteMemoryRequest {
    pub task_id: Option<i64>,
    pub id: String,
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
pub struct UiSetDefaultModelRequest {
    pub model_config_id: Option<i64>,
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
#[serde(rename_all = "camelCase")]
pub struct UiProbeProviderModelCapabilitiesRequest {
    pub provider_id: i64,
    pub model_id: String,
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
