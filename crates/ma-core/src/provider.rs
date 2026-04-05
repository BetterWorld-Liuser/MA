use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use genai::adapter::AdapterKind;
use genai::chat::{
    ChatOptions, ChatRequest, ChatResponse, ChatStreamEvent, MessageContent,
    ToolCall as GenAiToolCall,
};
use genai::resolver::{AuthData, Endpoint, ServiceTargetResolver};
use genai::{Client as GenAiClient, ModelIden, ServiceTarget};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::context::AgentContext;
use crate::settings::ProviderType;

mod messages;

pub use messages::{
    ApiToolCallRequest, ApiToolFunctionCallRequest, RequestMessage, build_messages,
};
use messages::build_chat_request;

/// RuntimeProviderConfig 是 provider 运行时唯一需要的配置。
/// March 自己负责上下文构建，这里只保留调用目标、鉴权和模型选择。
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

/// ProviderClient 统一承接 genai 的多 provider 调用。
/// OpenAI-compatible 仍然保留为一个显式类型，用于第三方代理和自定义端点。
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
                    .all_model_names(adapter_kind_for_provider(other, &self.config.model))
                    .await
                    .context("failed to load provider model list")?;
                models.sort();
                models.dedup();
                Ok(models)
            }
        }
    }

    /// 对 OpenAI-compatible 继续尝试从 `/models` 扩展字段里读上下文窗口。
    /// 其他 provider 先交给本地能力表兜底，避免把不同协议混成同一套脆弱解析。
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

    async fn list_model_descriptors(&self) -> Result<Vec<ModelDescriptor>> {
        let base_url = self
            .config
            .base_url
            .as_deref()
            .context("provider base url is required for model list")?;
        let mut request = self.http.get(format!("{}/models", base_url));
        if !self.config.api_key.trim().is_empty() {
            request = request.bearer_auth(&self.config.api_key);
        }

        let response = request
            .send()
            .await
            .context("failed to request model list")?
            .error_for_status()
            .context("model list request failed")?;

        let payload: ModelListResponse = response
            .json()
            .await
            .context("failed to decode model list response")?;

        payload
            .data
            .into_iter()
            .map(ModelDescriptor::from_value)
            .collect()
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
        if mode == ProviderDeliveryMode::NonStreaming {
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
                // Provider capability is messy in practice, especially for OpenAI-compatible
                // endpoints. We probe streaming optimistically once, then pin this provider/model
                // to the safer non-streaming path after a stream failure so later turns stay stable.
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

    /// 连通性测试需要验证“这个入口真的能完成一次最小对话”，而不只是 `/models`
    /// 或鉴权端点可达。对于 OpenAI-compatible / Ollama，优先探测一遍模型列表，
    /// 避免拿一个根本不存在的默认模型去误判“连不通”。
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

fn summarize_probe_reply(reply: &str) -> String {
    const MAX_REPLY_CHARS: usize = 48;

    let compact = reply.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated = chars.by_ref().take(MAX_REPLY_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{}…", truncated)
    } else {
        truncated
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderDeliveryMode {
    Streaming,
    NonStreaming,
}

#[derive(Debug, Clone)]
enum DeliveryPath {
    Streaming,
    NonStreamingCached,
    NonStreamingFallback { stream_failure: String },
}

#[derive(Debug)]
struct StreamAttemptFailure {
    kind: StreamFailureKind,
    source: anyhow::Error,
}

#[derive(Debug, Clone, Copy)]
enum StreamFailureKind {
    Start,
    ReadEvent,
    UnexpectedEnd,
}

impl StreamAttemptFailure {
    fn summary(&self) -> String {
        match self.kind {
            StreamFailureKind::Start => {
                format!("failed to start provider stream: {}", self.source)
            }
            StreamFailureKind::ReadEvent => {
                format!("failed to read provider stream event: {}", self.source)
            }
            StreamFailureKind::UnexpectedEnd => "provider stream ended unexpectedly".to_string(),
        }
    }
}

impl ProviderClient {
    fn chat_options() -> ChatOptions {
        ChatOptions::default()
            .with_temperature(0.2)
            .with_capture_content(true)
            .with_capture_tool_calls(true)
    }

    async fn complete_via_stream<F>(
        &self,
        request: ChatRequest,
        request_json: &str,
        on_event: &mut F,
    ) -> std::result::Result<ProviderResponse, StreamAttemptFailure>
    where
        F: FnMut(ProviderProgressEvent) -> Result<()>,
    {
        let options = Self::chat_options();
        let mut stream = self
            .client
            .exec_chat_stream(&self.config.model, request, Some(&options))
            .await
            .map_err(|error| StreamAttemptFailure {
                kind: StreamFailureKind::Start,
                source: error.into(),
            })?;

        let mut collector = StreamCollector::default();
        while let Some(event) = stream.stream.next().await {
            let event = event.map_err(|error| StreamAttemptFailure {
                kind: StreamFailureKind::ReadEvent,
                source: error.into(),
            })?;
            match event {
                ChatStreamEvent::Start => {}
                ChatStreamEvent::Chunk(chunk) => {
                    if !chunk.content.is_empty() {
                        collector.content.push_str(&chunk.content);
                        on_event(ProviderProgressEvent::ContentDelta(chunk.content)).map_err(
                            |error| StreamAttemptFailure {
                                kind: StreamFailureKind::ReadEvent,
                                source: error,
                            },
                        )?;
                    }
                }
                ChatStreamEvent::ToolCallChunk(chunk) => {
                    collector
                        .ingest_tool_call(chunk.tool_call)
                        .map_err(|error| StreamAttemptFailure {
                            kind: StreamFailureKind::ReadEvent,
                            source: error,
                        })?;
                    on_event(ProviderProgressEvent::ToolCallsUpdated(
                        collector.tool_call_deltas(),
                    ))
                    .map_err(|error| StreamAttemptFailure {
                        kind: StreamFailureKind::ReadEvent,
                        source: error,
                    })?;
                }
                ChatStreamEvent::ReasoningChunk(_) | ChatStreamEvent::ThoughtSignatureChunk(_) => {}
                ChatStreamEvent::End(end) => {
                    collector.absorb_captured_content(end.captured_content);
                    let mut response =
                        collector.finish().map_err(|error| StreamAttemptFailure {
                            kind: StreamFailureKind::ReadEvent,
                            source: error,
                        })?;
                    response.request_json = request_json.to_string();
                    response.raw_response = serde_json::to_string_pretty(
                        &debug_structured_response(&response, DeliveryPath::Streaming, None),
                    )
                    .unwrap_or_else(|_| "(failed to format provider response)".to_string());
                    return Ok(response);
                }
            }
        }

        Err(StreamAttemptFailure {
            kind: StreamFailureKind::UnexpectedEnd,
            source: anyhow::anyhow!("provider stream ended unexpectedly"),
        })
    }

    async fn complete_non_streaming(
        &self,
        request: ChatRequest,
        request_json: String,
        delivery_path: DeliveryPath,
    ) -> Result<ProviderResponse> {
        let options = Self::chat_options().with_capture_raw_body(true);
        let response = self
            .client
            .exec_chat(&self.config.model, request, Some(&options))
            .await
            .context("failed to request provider non-stream response")?;
        build_provider_response_from_chat_response(response, request_json, delivery_path)
    }
}

fn build_service_target(config: &RuntimeProviderConfig) -> ServiceTarget {
    let adapter_kind = adapter_kind_for_provider(config.provider_type, &config.model);
    let endpoint = config
        .base_url
        .as_ref()
        .map(|url| Endpoint::from_owned(endpoint_base_url_for_genai(url)))
        .unwrap_or_else(|| default_endpoint_for_provider(config.provider_type));
    let auth = if config.api_key.trim().is_empty() {
        AuthData::from_single("ollama")
    } else {
        AuthData::from_single(config.api_key.clone())
    };

    ServiceTarget {
        endpoint,
        auth,
        model: ModelIden::new(adapter_kind, config.model.clone()),
    }
}

/// `genai` 内部会用 URL join 追加 `chat/completions`、`embeddings` 等后缀。
/// 如果用户填写的是 `https://host/v1` 这种没有尾随 `/` 的目录 URL，
/// join 时会把最后一段 `v1` 当作“文件名”替换掉，最终错误地变成 `https://host/chat/completions`。
/// 这里统一补成目录语义，保证 `/v1/` 这类前缀路径在 OpenAI-compatible 网关上不被吞掉。
fn endpoint_base_url_for_genai(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

fn adapter_kind_for_provider(provider_type: ProviderType, model: &str) -> AdapterKind {
    match provider_type {
        ProviderType::OpenAiCompat => AdapterKind::OpenAI,
        ProviderType::OpenAi => AdapterKind::from_model(model).unwrap_or(AdapterKind::OpenAI),
        ProviderType::Anthropic => AdapterKind::Anthropic,
        ProviderType::Gemini => AdapterKind::Gemini,
        ProviderType::Fireworks => AdapterKind::Fireworks,
        ProviderType::Together => AdapterKind::Together,
        ProviderType::Groq => AdapterKind::Groq,
        ProviderType::Mimo => AdapterKind::Mimo,
        ProviderType::Nebius => AdapterKind::Nebius,
        ProviderType::Xai => AdapterKind::Xai,
        ProviderType::DeepSeek => AdapterKind::DeepSeek,
        ProviderType::Zai => AdapterKind::Zai,
        ProviderType::BigModel => AdapterKind::BigModel,
        ProviderType::Cohere => AdapterKind::Cohere,
        ProviderType::Ollama => AdapterKind::Ollama,
    }
}

fn default_endpoint_for_provider(provider_type: ProviderType) -> Endpoint {
    match provider_type {
        ProviderType::OpenAiCompat | ProviderType::OpenAi => {
            Endpoint::from_static("https://api.openai.com/v1/")
        }
        ProviderType::Anthropic => Endpoint::from_static("https://api.anthropic.com/v1/"),
        ProviderType::Gemini => {
            Endpoint::from_static("https://generativelanguage.googleapis.com/v1beta/")
        }
        ProviderType::Fireworks => Endpoint::from_static("https://api.fireworks.ai/inference/v1/"),
        ProviderType::Together => Endpoint::from_static("https://api.together.xyz/v1/"),
        ProviderType::Groq => Endpoint::from_static("https://api.groq.com/openai/v1/"),
        ProviderType::Mimo => Endpoint::from_static("https://api.mimo.org/v1/"),
        ProviderType::Nebius => Endpoint::from_static("https://api.studio.nebius.com/v1/"),
        ProviderType::Xai => Endpoint::from_static("https://api.x.ai/v1/"),
        ProviderType::DeepSeek => Endpoint::from_static("https://api.deepseek.com/v1/"),
        ProviderType::Zai => Endpoint::from_static("https://api.z.ai/api/paas/v4/"),
        ProviderType::BigModel => Endpoint::from_static("https://open.bigmodel.cn/api/paas/v4/"),
        ProviderType::Cohere => Endpoint::from_static("https://api.cohere.com/v2/"),
        ProviderType::Ollama => Endpoint::from_static("http://localhost:11434/v1/"),
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

#[derive(Debug, Serialize)]
struct DebugStructuredProviderResponse {
    delivery_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_failure: Option<String>,
    content: Option<String>,
    tool_calls: Vec<DebugStructuredToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    captured_raw_body: Option<Value>,
}

#[derive(Debug, Serialize)]
struct DebugStructuredToolCall {
    id: String,
    name: String,
    arguments_json: String,
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

pub fn fallback_task_title(first_user_message: &str) -> Option<String> {
    let trimmed = first_user_message.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut title = trimmed
        .replace('\n', " ")
        .replace('\r', " ")
        .replace('“', "")
        .replace('”', "")
        .replace('"', "")
        .replace('：', " ")
        .replace(':', " ");

    for prefix in [
        "帮我",
        "请你",
        "麻烦",
        "看下",
        "看一下",
        "继续",
        "这个",
        "这里",
        "我想",
        "我需要",
    ] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.trim().to_string();
        }
    }

    sanitize_task_title(&title)
}

fn sanitize_task_title(raw: &str) -> Option<String> {
    let first_line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;

    let mut title = first_line
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '“' | '”' | '`'))
        .trim_end_matches(|ch: char| {
            matches!(
                ch,
                '。' | '.' | '！' | '!' | '?' | '？' | '；' | ';' | '：' | ':'
            )
        })
        .trim()
        .to_string();

    for prefix in ["标题：", "标题:", "task:", "Task:", "任务名：", "任务名:"] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.trim().to_string();
        }
    }

    title = title.split_whitespace().collect::<Vec<_>>().join(" ");

    if title.is_empty() {
        return None;
    }

    let char_count = title.chars().count();
    if char_count <= 2 {
        return None;
    }

    if char_count > 24 {
        title = title
            .chars()
            .take(24)
            .collect::<String>()
            .trim()
            .to_string();
    }

    if title.is_empty() { None } else { Some(title) }
}

pub fn format_provider_response_for_debug(raw_response: &str) -> String {
    if raw_response.trim().is_empty() {
        return "(empty response)".to_string();
    }

    serde_json::from_str::<serde_json::Value>(raw_response)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| raw_response.to_string())
}


#[derive(Debug, Default)]
struct StreamCollector {
    content: String,
    tool_calls: Vec<StreamToolCallAccumulator>,
}

#[derive(Debug, Default)]
struct StreamToolCallAccumulator {
    id: Option<String>,
    name: String,
    arguments_json: String,
}

impl StreamCollector {
    fn ingest_tool_call(&mut self, tool_call: GenAiToolCall) -> Result<()> {
        let arguments_json = serde_json::to_string(&tool_call.fn_arguments)
            .context("failed to encode streamed tool call arguments")?;
        if let Some(existing) = self
            .tool_calls
            .iter_mut()
            .find(|existing| existing.id.as_deref() == Some(tool_call.call_id.as_str()))
        {
            existing.name = tool_call.fn_name;
            existing.arguments_json = arguments_json;
            existing.id = Some(tool_call.call_id);
            return Ok(());
        }

        self.tool_calls.push(StreamToolCallAccumulator {
            id: Some(tool_call.call_id),
            name: tool_call.fn_name,
            arguments_json,
        });
        Ok(())
    }

    fn absorb_captured_content(&mut self, content: Option<MessageContent>) {
        let Some(content) = content else {
            return;
        };

        if let Some(text) = content.joined_texts() {
            self.content = text;
        }

        let captured_tool_calls = content.into_tool_calls();
        if captured_tool_calls.is_empty() {
            return;
        }

        self.tool_calls.clear();
        for tool_call in captured_tool_calls {
            let arguments_json =
                serde_json::to_string(&tool_call.fn_arguments).unwrap_or_else(|_| "{}".to_string());
            self.tool_calls.push(StreamToolCallAccumulator {
                id: Some(tool_call.call_id),
                name: tool_call.fn_name,
                arguments_json,
            });
        }
    }

    fn finish(self) -> Result<ProviderResponse> {
        let tool_calls = self
            .tool_calls
            .into_iter()
            .map(|tool_call| {
                let id = tool_call
                    .id
                    .filter(|value| !value.trim().is_empty())
                    .context("streamed tool call missing id")?;
                if tool_call.name.trim().is_empty() {
                    bail!("streamed tool call missing function name");
                }
                Ok(ProviderToolCall {
                    id,
                    name: tool_call.name,
                    arguments_json: tool_call.arguments_json,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ProviderResponse {
            content: if self.content.trim().is_empty() {
                None
            } else {
                Some(self.content)
            },
            tool_calls,
            request_json: String::new(),
            raw_response: String::new(),
        })
    }

    fn tool_call_deltas(&self) -> Vec<ProviderToolCallDelta> {
        self.tool_calls
            .iter()
            .map(|tool_call| ProviderToolCallDelta {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments_json: tool_call.arguments_json.clone(),
            })
            .collect()
    }
}

fn debug_structured_response(
    response: &ProviderResponse,
    delivery_path: DeliveryPath,
    captured_raw_body: Option<Value>,
) -> DebugStructuredProviderResponse {
    DebugStructuredProviderResponse {
        delivery_path: delivery_path.label().to_string(),
        stream_failure: delivery_path.stream_failure(),
        content: response.content.clone(),
        tool_calls: response
            .tool_calls
            .clone()
            .into_iter()
            .map(|tool_call| DebugStructuredToolCall {
                id: tool_call.id,
                name: tool_call.name,
                arguments_json: tool_call.arguments_json,
            })
            .collect(),
        captured_raw_body,
    }
}

impl DeliveryPath {
    fn label(&self) -> &'static str {
        match self {
            DeliveryPath::Streaming => "streaming",
            DeliveryPath::NonStreamingCached => "non_streaming_cached",
            DeliveryPath::NonStreamingFallback { .. } => "non_streaming_fallback",
        }
    }

    fn stream_failure(&self) -> Option<String> {
        match self {
            DeliveryPath::NonStreamingFallback { stream_failure } => Some(stream_failure.clone()),
            _ => None,
        }
    }
}

fn build_provider_response_from_chat_response(
    response: ChatResponse,
    request_json: String,
    delivery_path: DeliveryPath,
) -> Result<ProviderResponse> {
    let content = response
        .content
        .joined_texts()
        .filter(|text| !text.trim().is_empty());
    let tool_calls = response
        .content
        .tool_calls()
        .into_iter()
        .map(|tool_call| {
            Ok(ProviderToolCall {
                id: tool_call.call_id.clone(),
                name: tool_call.fn_name.clone(),
                arguments_json: serde_json::to_string(&tool_call.fn_arguments)
                    .context("failed to encode provider tool call arguments")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let provider_response = ProviderResponse {
        content,
        tool_calls,
        request_json,
        raw_response: String::new(),
    };
    let debug_payload = debug_structured_response(
        &provider_response,
        delivery_path,
        response.captured_raw_body,
    );

    Ok(ProviderResponse {
        raw_response: serde_json::to_string_pretty(&debug_payload)
            .unwrap_or_else(|_| "(failed to format provider response)".to_string()),
        ..provider_response
    })
}

fn stream_capability_cache() -> &'static Mutex<HashMap<String, ProviderDeliveryMode>> {
    static CACHE: OnceLock<Mutex<HashMap<String, ProviderDeliveryMode>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn stream_preference_for(config: &RuntimeProviderConfig) -> ProviderDeliveryMode {
    stream_capability_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&provider_capability_key(config)).copied())
        .unwrap_or(ProviderDeliveryMode::Streaming)
}

fn remember_stream_success(config: &RuntimeProviderConfig) {
    if let Ok(mut cache) = stream_capability_cache().lock() {
        cache.insert(
            provider_capability_key(config),
            ProviderDeliveryMode::Streaming,
        );
    }
}

fn remember_stream_failure(config: &RuntimeProviderConfig) {
    if let Ok(mut cache) = stream_capability_cache().lock() {
        cache.insert(
            provider_capability_key(config),
            ProviderDeliveryMode::NonStreaming,
        );
    }
}

fn provider_capability_key(config: &RuntimeProviderConfig) -> String {
    format!(
        "{}|{}|{}",
        config.provider_type.as_db_value(),
        config.base_url.as_deref().unwrap_or("(default)"),
        config.model,
    )
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<Value>,
}

#[derive(Debug, Clone)]
struct ModelDescriptor {
    id: String,
    context_window_tokens: Option<usize>,
}

impl ModelDescriptor {
    fn from_value(value: Value) -> Result<Self> {
        let object = value
            .as_object()
            .context("provider model entry was not an object")?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .context("provider model entry missing string id")?
            .to_string();

        let context_window_tokens = [
            object.get("context_window"),
            object.get("max_input_tokens"),
            object.get("input_token_limit"),
        ]
        .into_iter()
        .flatten()
        .find_map(parse_context_window_value);

        Ok(Self {
            id,
            context_window_tokens,
        })
    }
}

fn parse_context_window_value(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DeliveryPath, RuntimeProviderConfig, build_messages, fallback_task_title,
        provider_capability_key,
    };
    use crate::context::{
        AgentContext, ChatTurn, ContextPressure, FileSnapshot, Hint, Injection, ModifiedBy,
        NoteEntry, Role, RuntimeStatus, SessionStatus,
    };
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
                content: "hello".to_string(),
                timestamp: SystemTime::now(),
            }],
            tools: Vec::new(),
        };

        let messages = build_messages(&context);
        assert_eq!(messages.len(), 3);
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
            provider_capability_key(&config),
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
