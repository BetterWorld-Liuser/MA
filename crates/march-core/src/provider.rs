use crate::agent::TurnCancellation;
use crate::settings::{ProviderType, ServerToolConfig};
use anyhow::{Context, Result};
use reqwest::Client as HttpClient;

mod delivery;
mod execution;
mod messages;
mod probe;
mod title;
mod transport;
mod wire;

use delivery::{
    DeliveryPath, is_rate_limit_error, remember_stream_failure, remember_stream_success,
    stream_preference_for,
};
use messages::{RequestOptions, function_tools_for_context};
use transport::list_model_descriptors;

pub use messages::{
    ApiToolCallRequest, ApiToolFunctionCallRequest, MessageContent, RequestMessage, build_messages,
};
pub use title::{fallback_task_title, format_provider_response_for_debug};

#[derive(Debug, Clone)]
pub struct RuntimeProviderConfig {
    pub provider_type: ProviderType,
    pub base_url: Option<String>,
    pub api_key: String,
    pub model: String,
    pub server_tools: Vec<ServerToolConfig>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub max_output_tokens: Option<u32>,
}

impl RuntimeProviderConfig {
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("MA_OPENAI_BASE_URL")
            .context("missing MA_OPENAI_BASE_URL environment variable")?;
        let api_key = std::env::var("MA_OPENAI_API_KEY")
            .context("missing MA_OPENAI_API_KEY environment variable")?;
        let model = std::env::var("MA_OPENAI_MODEL")
            .context("missing MA_OPENAI_MODEL environment variable")?;

        Ok(Self {
            provider_type: ProviderType::OpenAiCompat,
            base_url: Some(base_url.trim_end_matches('/').to_string()),
            api_key,
            model,
            server_tools: Vec::new(),
            temperature: None,
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            max_output_tokens: None,
        })
    }
}

#[derive(Clone)]
pub struct ProviderClient {
    http: HttpClient,
    config: RuntimeProviderConfig,
    function_tools: Vec<messages::FunctionToolDefinition>,
}

pub type OpenAiCompatibleClient = ProviderClient;
pub type OpenAiCompatibleConfig = RuntimeProviderConfig;

impl ProviderClient {
    pub fn new(config: RuntimeProviderConfig) -> Self {
        Self {
            http: HttpClient::new(),
            config,
            function_tools: Vec::new(),
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        let mut models = self
            .list_model_descriptors()
            .await?
            .into_iter()
            .map(|model| model.id)
            .collect::<Vec<_>>();

        if models.is_empty() && !self.config.model.trim().is_empty() {
            models.push(self.config.model.clone());
        }

        models.sort();
        models.dedup();
        Ok(models)
    }

    pub async fn resolve_model_context_window(&self, model_id: &str) -> Result<Option<usize>> {
        let descriptors = self.list_model_descriptors().await?;
        Ok(descriptors
            .into_iter()
            .find(|model| model.id == model_id)
            .and_then(|model| model.context_window_tokens))
    }

    async fn list_model_descriptors(&self) -> Result<Vec<transport::ModelDescriptor>> {
        list_model_descriptors(&self.http, &self.config).await
    }

    pub async fn respond_to_context(
        &self,
        context: &crate::context::AgentContext,
        transcript: &[ConversationDelta],
    ) -> Result<ModelResponse> {
        let mut messages = build_messages(context);

        for delta in transcript {
            match delta {
                ConversationDelta::AssistantToolCalls(tool_calls) => {
                    messages.push(RequestMessage::assistant_tool_calls(
                        tool_calls
                            .iter()
                            .map(|tool_call| ApiToolCallRequest {
                                id: tool_call.id.clone(),
                                tool_type: "function".to_string(),
                                function: ApiToolFunctionCallRequest {
                                    name: tool_call.name.clone(),
                                    arguments: tool_call.arguments.clone(),
                                },
                            })
                            .collect(),
                    ));
                }
                ConversationDelta::ToolResult {
                    tool_call_id,
                    content,
                } => messages.push(RequestMessage::tool(tool_call_id.clone(), content.clone())),
            }
        }

        let response = self.complete_context(context, messages).await?;
        if !response.tool_calls.is_empty() {
            return Ok(ModelResponse::ToolCalls(
                response
                    .tool_calls
                    .into_iter()
                    .map(|tool_call| RequestedToolCall {
                        id: tool_call.id,
                        name: tool_call.name,
                        arguments: tool_call.arguments_json,
                    })
                    .collect(),
            ));
        }

        Ok(ModelResponse::AssistantMessage(
            response.content.unwrap_or_default(),
        ))
    }

}

#[derive(Debug, Clone)]
pub struct ProviderResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ProviderToolCall>,
    pub request_json: String,
    pub raw_response: String,
}

#[derive(Debug, Clone)]
pub struct ProviderToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone)]
pub struct ProviderToolCallDelta {
    pub id: Option<String>,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone)]
pub enum ProviderProgressEvent {
    ContentDelta(String),
    ToolCallsUpdated(Vec<ProviderToolCallDelta>),
}

#[derive(Debug, Clone)]
pub struct RequestedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub enum ConversationDelta {
    AssistantToolCalls(Vec<RequestedToolCall>),
    ToolResult {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Debug, Clone)]
pub enum ModelResponse {
    AssistantMessage(String),
    ToolCalls(Vec<RequestedToolCall>),
}

#[cfg(test)]
mod tests {
    use super::{DeliveryPath, RuntimeProviderConfig, build_messages, fallback_task_title};
    use crate::context::{
        AgentContext, ChatTurn, ContentBlock, ContextPressure, FileSnapshot, Hint, Injection,
        ModifiedBy, NoteEntry, Role, RuntimeStatus, SessionStatus,
    };
    use crate::provider::delivery;
    use crate::settings::ProviderType;
    use indexmap::IndexMap;
    use std::path::PathBuf;
    use std::time::SystemTime;

    #[test]
    fn fallback_task_title_removes_prefix_and_punctuation() {
        assert_eq!(
            fallback_task_title("帮我修一下登录 bug。"),
            Some("修一下登录 bug".to_string())
        );
    }

    #[test]
    fn build_messages_preserves_injections_and_context_layers() {
        let mut notes = IndexMap::new();
        notes.insert(
            "goal".to_string(),
            NoteEntry {
                content: "整理 provider 抽象".to_string(),
            },
        );
        let mut open_files = IndexMap::new();
        open_files.insert(
            PathBuf::from("src/main.rs"),
            FileSnapshot::available(
                "src/main.rs",
                "fn main() {}",
                SystemTime::now(),
                ModifiedBy::Unknown,
            ),
        );
        let context = AgentContext {
            system_core: "system core".to_string(),
            injections: vec![Injection {
                id: "skill".to_string(),
                content: "Do the thing".to_string(),
            }],
            session_status: SessionStatus {
                workspace_root: PathBuf::from("D:/playground/MA"),
                platform: "windows".to_string(),
                shell: "powershell".to_string(),
                available_shells: vec!["powershell".to_string()],
                workspace_entries: vec!["src/main.rs".to_string()],
            },
            open_files,
            notes,
            memory_index: None,
            runtime_status: RuntimeStatus {
                locked_files: vec![PathBuf::from("AGENTS.md")],
                context_pressure: Some(ContextPressure {
                    used_percent: 10,
                    message: "safe".to_string(),
                }),
            },
            hints: vec![Hint::new("tests red", None, None)],
            recent_chat: vec![ChatTurn {
                role: Role::User,
                agent: "march".to_string(),
                content: vec![ContentBlock::text("hello")],
                timestamp: SystemTime::now(),
            }],
            tools: Vec::new(),
        };

        let messages = build_messages(&context);
        assert_eq!(messages.len(), 4);
    }

    #[test]
    fn provider_capability_key_includes_provider_endpoint_and_model() {
        let config = RuntimeProviderConfig {
            provider_type: ProviderType::OpenAiCompat,
            base_url: Some("http://localhost:11434/v1".to_string()),
            api_key: String::new(),
            model: "qwen2.5-coder:32b".to_string(),
            server_tools: Vec::new(),
            temperature: None,
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            max_output_tokens: None,
        };

        assert_eq!(
            delivery::provider_capability_key(&config),
            "openai_compat|http://localhost:11434/v1|qwen2.5-coder:32b"
        );
    }

    #[test]
    fn delivery_path_only_reports_stream_failure_for_fallback_mode() {
        assert_eq!(DeliveryPath::Streaming.stream_failure(), None);
        assert_eq!(DeliveryPath::NonStreamingCached.stream_failure(), None);
        assert_eq!(
            DeliveryPath::NonStreamingFallback {
                stream_failure: "provider stream ended unexpectedly".to_string(),
            }
            .stream_failure(),
            Some("provider stream ended unexpectedly".to_string())
        );
    }
}
