use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiProviderModelsView {
    pub current_model: String,
    pub available_models: Vec<String>,
    pub suggested_models: Vec<String>,
    pub provider_cache_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiTaskModelItemView {
    pub model_config_id: i64,
    pub provider_id: i64,
    pub provider_name: String,
    pub provider_type: String,
    pub display_name: String,
    pub model_id: String,
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
    pub current_model_config_id: Option<i64>,
    pub current_model: String,
    pub current_temperature: Option<f32>,
    pub current_top_p: Option<f32>,
    pub current_presence_penalty: Option<f32>,
    pub current_frequency_penalty: Option<f32>,
    pub current_max_output_tokens: Option<u32>,
    pub current_model_capabilities: UiModelCapabilitiesView,
    pub models: Vec<UiTaskModelItemView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiProviderSettingsView {
    pub database_path: PathBuf,
    pub providers: Vec<UiProviderView>,
    pub agents: Vec<UiAgentProfileView>,
    pub default_model_config_id: Option<i64>,
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
    pub api_key: String,
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
    pub probed_at: Option<i64>,
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
pub struct UiProbeProviderModelCapabilitiesView {
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub supports_tool_use: bool,
    pub supports_vision: bool,
    pub supports_audio: bool,
    pub supports_pdf: bool,
    pub warnings: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTestProviderConnectionResult {
    pub success: bool,
    pub message: String,
    pub suggested_model: Option<String>,
}
