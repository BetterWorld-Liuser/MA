use crate::context::AgentContext;
use crate::settings::ProviderType;
use anyhow::{Context, Result};
use genai::chat::{ChatOptions, ChatRequest};
use genai::resolver::ServiceTargetResolver;
use genai::{Client as GenAiClient, ServiceTarget};
use reqwest::Client as HttpClient;

mod delivery;
mod messages;
mod title;
mod transport;

use delivery::{
    DeliveryPath, remember_stream_failure, remember_stream_success, stream_preference_for,
};
use messages::build_chat_request;
use title::{sanitize_task_title, summarize_probe_reply};
use transport::{build_service_target, list_model_descriptors};

pub use messages::{
    ApiToolCallRequest, ApiToolFunctionCallRequest, RequestMessage, build_messages,
};
pub use title::{fallback_task_title, format_provider_response_for_debug};

#[derive(Debug, Clone)]
pub struct RuntimeProviderConfig {
    pub provider_type: ProviderType,
    pub base_url: Option<String>,
    pub api_key: String,
    pub model: String,
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
        })
    }
}

#[derive(Clone)]
pub struct ProviderClient {
    http: HttpClient,
    client: GenAiClient,
    config: RuntimeProviderConfig,
}

pub type OpenAiCompatibleClient = ProviderClient;
pub type OpenAiCompatibleConfig = RuntimeProviderConfig;

impl ProviderClient {
    pub fn new(config: RuntimeProviderConfig) -> Self {
        let resolver_config = config.clone();
        let target_resolver = ServiceTargetResolver::from_resolver_fn(
            move |_target: ServiceTarget| -> Result<ServiceTarget, genai::resolver::Error> {
                Ok(build_service_target(&resolver_config))
            },
        );

        let client = GenAiClient::builder()
            .with_service_target_resolver(target_resolver)
            .build();

        Self {
            http: HttpClient::new(),
            client,
            config,
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        match self.config.provider_type {
            ProviderType::OpenAiCompat => Ok(self
                .list_model_descriptors()
                .await?
                .into_iter()
                .map(|model| model.id)
                .collect()),
            other => {
                let mut models = self
                    .client
                    .all_model_names(transport_adapter_kind(other, &self.config.model))
                    .await
                    .context("failed to load provider model list")?;
                models.sort();
                models.dedup();
                Ok(models)
            }
        }
    }

    pub async fn resolve_model_context_window(&self, model_id: &str) -> Result<Option<usize>> {
        if self.config.provider_type != ProviderType::OpenAiCompat {
            return Ok(None);
        }

        let descriptors = self.list_model_descriptors().await?;
        Ok(descriptors
            .into_iter()
            .find(|model| model.id == model_id)
            .and_then(|model| model.context_window_tokens))
    }

    async fn list_model_descriptors(&self) -> Result<Vec<transport::ModelDescriptor>> {
        list_model_descriptors(&self.http, &self.config).await
    }

    pub async fn complete_context(
        &self,
        context: &AgentContext,
        conversation: Vec<RequestMessage>,
    ) -> Result<ProviderResponse> {
        self.complete_context_with_events(context, conversation, |_| Ok(()))
            .await
    }

    pub async fn complete_context_with_events<F>(
        &self,
        context: &AgentContext,
        conversation: Vec<RequestMessage>,
        mut on_event: F,
    ) -> Result<ProviderResponse>
    where
        F: FnMut(ProviderProgressEvent) -> Result<()>,
    {
        let request = build_chat_request(context, &conversation)?;
        let request_json =
            serde_json::to_string_pretty(&request).context("failed to encode provider request")?;
        let mode = stream_preference_for(&self.config);
        if mode == delivery::ProviderDeliveryMode::NonStreaming {
            return self
                .complete_non_streaming(request, request_json, DeliveryPath::NonStreamingCached)
                .await;
        }

        match self
            .complete_via_stream(request.clone(), &request_json, &mut on_event)
            .await
        {
            Ok(response) => {
                remember_stream_success(&self.config);
                Ok(response)
            }
            Err(stream_failure) => {
                remember_stream_failure(&self.config);
                match self
                    .complete_non_streaming(
                        request,
                        request_json,
                        DeliveryPath::NonStreamingFallback {
                            stream_failure: stream_failure.summary(),
                        },
                    )
                    .await
                {
                    Ok(response) => Ok(response),
                    Err(fallback_error) => Err(fallback_error.context(format!(
                        "provider streaming failed ({}) and fallback non-stream request also failed",
                        stream_failure.summary()
                    ))),
                }
            }
        }
    }

    pub async fn respond_to_context(
        &self,
        context: &AgentContext,
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

    pub async fn suggest_task_title(&self, first_user_message: &str) -> Result<Option<String>> {
        let trimmed = first_user_message.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let request = ChatRequest::from_user(format!("First user message:\n{}", trimmed))
            .with_system(
                "You generate concise task titles for a coding workspace.\n\
                 Return only the title text.\n\
                 Rules:\n\
                 - Prefer Simplified Chinese when the user writes Chinese.\n\
                 - Use 8-18 characters when possible.\n\
                 - Keep the concrete object, such as a file, module, or bug.\n\
                 - Remove filler like '帮我', '请你', '看一下', '继续'.\n\
                 - Do not use quotes, numbering, or trailing punctuation.",
            );
        let options = ChatOptions::default().with_temperature(0.1);
        let response = self
            .client
            .exec_chat(&self.config.model, request, Some(&options))
            .await
            .context("failed to request suggested title")?;

        Ok(response
            .first_text()
            .and_then(sanitize_task_title)
            .or_else(|| fallback_task_title(trimmed)))
    }

    pub async fn test_connection(&self) -> Result<String> {
        let probe_model = self.resolve_probe_model_for_connection().await?;
        let reply = self.run_probe_request(&probe_model).await?;
        Ok(format!(
            "连接成功，模型 {} 已完成最小消息往返：{}",
            probe_model, reply
        ))
    }

    async fn resolve_probe_model_for_connection(&self) -> Result<String> {
        let configured_model = self.config.model.trim();

        match self.config.provider_type {
            ProviderType::OpenAiCompat | ProviderType::Ollama => match self.list_models().await {
                Ok(models) => {
                    if let Some(model) = models
                        .iter()
                        .find(|model| model.as_str() == configured_model)
                    {
                        return Ok(model.clone());
                    }
                    if let Some(model) = models.into_iter().find(|model| !model.trim().is_empty()) {
                        return Ok(model);
                    }
                    if !configured_model.is_empty() {
                        return Ok(configured_model.to_string());
                    }
                    anyhow::bail!("provider 没有返回可用模型，无法完成真实对话测试")
                }
                Err(error) => {
                    if !configured_model.is_empty() {
                        Ok(configured_model.to_string())
                    } else {
                        Err(error.context(
                            "failed to determine probe model for provider connection test",
                        ))
                    }
                }
            },
            _ if !configured_model.is_empty() => Ok(configured_model.to_string()),
            _ => anyhow::bail!("provider probe model is empty"),
        }
    }

    async fn run_probe_request(&self, model: &str) -> Result<String> {
        let request = ChatRequest::from_user(
            "Return exactly `MARCH_OK` and nothing else. Do not call tools.",
        );
        let options = ChatOptions::default()
            .with_temperature(0.0)
            .with_max_tokens(16)
            .with_capture_content(true);
        let response = self
            .client
            .exec_chat(model, request, Some(&options))
            .await
            .context("failed to run provider probe request")?;
        let reply = response
            .first_text()
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .ok_or_else(|| anyhow::anyhow!("provider probe response did not contain text"))?;

        Ok(summarize_probe_reply(reply))
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

fn transport_adapter_kind(provider_type: ProviderType, model: &str) -> genai::adapter::AdapterKind {
    match provider_type {
        ProviderType::OpenAiCompat => genai::adapter::AdapterKind::OpenAI,
        ProviderType::OpenAi => genai::adapter::AdapterKind::from_model(model)
            .unwrap_or(genai::adapter::AdapterKind::OpenAI),
        ProviderType::Anthropic => genai::adapter::AdapterKind::Anthropic,
        ProviderType::Gemini => genai::adapter::AdapterKind::Gemini,
        ProviderType::Fireworks => genai::adapter::AdapterKind::Fireworks,
        ProviderType::Together => genai::adapter::AdapterKind::Together,
        ProviderType::Groq => genai::adapter::AdapterKind::Groq,
        ProviderType::Mimo => genai::adapter::AdapterKind::Mimo,
        ProviderType::Nebius => genai::adapter::AdapterKind::Nebius,
        ProviderType::Xai => genai::adapter::AdapterKind::Xai,
        ProviderType::DeepSeek => genai::adapter::AdapterKind::DeepSeek,
        ProviderType::Zai => genai::adapter::AdapterKind::Zai,
        ProviderType::BigModel => genai::adapter::AdapterKind::BigModel,
        ProviderType::Cohere => genai::adapter::AdapterKind::Cohere,
        ProviderType::Ollama => genai::adapter::AdapterKind::Ollama,
    }
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
