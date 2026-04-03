use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use genai::adapter::AdapterKind;
use genai::chat::{
    ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart, MessageContent,
    Tool as GenAiTool, ToolCall as GenAiToolCall, ToolResponse as GenAiToolResponse,
};
use genai::resolver::{AuthData, Endpoint, ServiceTargetResolver};
use genai::{Client as GenAiClient, ModelIden, ServiceTarget};
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::{AgentContext, render_file_snapshot_for_prompt};
use crate::settings::ProviderType;
use crate::tools::{ToolDefinition, ToolParameter};

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
        let options = ChatOptions::default()
            .with_temperature(0.2)
            .with_capture_content(true)
            .with_capture_tool_calls(true);
        let mut stream = self
            .client
            .exec_chat_stream(&self.config.model, request, Some(&options))
            .await
            .context("failed to start provider stream")?;

        let mut collector = StreamCollector::default();
        while let Some(event) = stream.stream.next().await {
            match event.context("failed to read provider stream event")? {
                ChatStreamEvent::Start => {}
                ChatStreamEvent::Chunk(chunk) => {
                    if !chunk.content.is_empty() {
                        collector.content.push_str(&chunk.content);
                        on_event(ProviderProgressEvent::ContentDelta(chunk.content))?;
                    }
                }
                ChatStreamEvent::ToolCallChunk(chunk) => {
                    collector.ingest_tool_call(chunk.tool_call)?;
                    on_event(ProviderProgressEvent::ToolCallsUpdated(
                        collector.tool_call_deltas(),
                    ))?;
                }
                ChatStreamEvent::ReasoningChunk(_) | ChatStreamEvent::ThoughtSignatureChunk(_) => {}
                ChatStreamEvent::End(end) => {
                    collector.absorb_captured_content(end.captured_content);
                    let mut response = collector.finish()?;
                    response.request_json = request_json;
                    response.raw_response = serde_json::to_string_pretty(
                        &debug_structured_response_from_provider(Some(response.clone())),
                    )
                    .unwrap_or_else(|_| "(failed to format provider response)".to_string());
                    return Ok(response);
                }
            }
        }

        bail!("provider stream ended unexpectedly")
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
        match self.config.provider_type {
            ProviderType::OpenAiCompat | ProviderType::Ollama => {
                let models = self.list_models().await?;
                let model_count = models.len();
                let sample = models.into_iter().take(3).collect::<Vec<_>>().join(", ");
                if sample.is_empty() {
                    Ok("连接成功，但 provider 没有返回模型列表。".to_string())
                } else {
                    Ok(format!(
                        "连接成功，已读取 {} 个模型，例如：{}",
                        model_count, sample
                    ))
                }
            }
            _ => {
                let request = ChatRequest::from_user("Reply with OK only.");
                let options = ChatOptions::default()
                    .with_temperature(0.0)
                    .with_max_tokens(8);
                let response = self
                    .client
                    .exec_chat(&self.config.model, request, Some(&options))
                    .await
                    .context("failed to run provider probe request")?;
                let reply = response.first_text().unwrap_or("OK").trim();
                Ok(format!(
                    "连接成功，模型 {} 已响应：{}",
                    self.config.model, reply
                ))
            }
        }
    }
}

fn build_service_target(config: &RuntimeProviderConfig) -> ServiceTarget {
    let adapter_kind = adapter_kind_for_provider(config.provider_type, &config.model);
    let endpoint = config
        .base_url
        .as_ref()
        .map(|url| Endpoint::from_owned(url.clone()))
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
        ProviderType::Fireworks => {
            Endpoint::from_static("https://api.fireworks.ai/inference/v1/")
        }
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
    content: Option<String>,
    tool_calls: Vec<DebugStructuredToolCall>,
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

/// RequestMessage 保持显式结构，方便 tool loop 在同一轮里累积 assistant/tool 消息。
#[derive(Debug, Clone, Serialize)]
pub struct RequestMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    tool_calls: Vec<ApiToolCallRequest>,
}

impl RequestMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant_text(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant_tool_calls_with_text(
        content: Option<String>,
        tool_calls: Vec<ApiToolCallRequest>,
    ) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            tool_call_id: None,
            tool_calls,
        }
    }

    pub fn assistant_tool_calls(tool_calls: Vec<ApiToolCallRequest>) -> Self {
        Self::assistant_tool_calls_with_text(None, tool_calls)
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(content.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }
}

pub fn build_messages(context: &AgentContext) -> Vec<RequestMessage> {
    let mut messages = vec![RequestMessage::system(context.system_core.clone())];

    if !context.injections.is_empty() {
        messages.push(RequestMessage::system(render_injections(context)));
    }

    messages.push(RequestMessage::user(render_context_body(context)));
    messages
}

fn render_injections(context: &AgentContext) -> String {
    let mut output = String::from("# Injections\n");

    for injection in &context.injections {
        output.push_str(&format!("## {}\n{}\n", injection.id, injection.content));
    }

    output
}

fn render_context_body(context: &AgentContext) -> String {
    let mut output = String::new();
    output.push_str("# Session Status\n");
    if context.session_status.is_empty() {
        output.push_str("(none)\n");
    } else {
        output.push_str(&format!(
            "workspace_root: {}\nplatform: {}\ndefault_shell: {}\n",
            context.session_status.workspace_root.display(),
            context.session_status.platform,
            context.session_status.shell
        ));

        if context.session_status.available_shells.is_empty() {
            output.push_str("available_shells: (none)\n");
        } else {
            output.push_str(&format!(
                "available_shells: {}\n",
                context.session_status.available_shells.join(", ")
            ));
        }

        if context.session_status.workspace_entries.is_empty() {
            output.push_str("workspace_entries: (none)\n");
        } else {
            output.push_str("workspace_entries:\n");
            for entry in &context.session_status.workspace_entries {
                output.push_str(&format!("- {entry}\n"));
            }
        }
    }

    output.push_str("\n# Open Files\n");
    if context.open_files.is_empty() {
        output.push_str("(none)\n");
    } else {
        output.push_str(
            "These watcher-backed snapshots are the authoritative current file contents for this turn. Reuse them instead of re-reading the same files via shell commands unless you need a different external view.\n\n",
        );
        for snapshot in context.open_files_in_prompt_order() {
            output.push_str(&render_file_snapshot_for_prompt(snapshot));
            output.push('\n');
        }
    }

    output.push_str("# Notes\n");
    if context.notes.is_empty() {
        output.push_str("(none)\n");
    } else {
        for (id, note) in &context.notes {
            output.push_str(&format!("{id}: {}\n", note.content));
        }
    }

    output.push_str("\n# Runtime Status\n");
    if context.runtime_status.is_empty() {
        output.push_str("(none)\n");
    } else {
        if context.runtime_status.locked_files.is_empty() {
            output.push_str("locked_files: (none)\n");
        } else {
            output.push_str("locked_files:\n");
            for path in &context.runtime_status.locked_files {
                output.push_str(&format!("- {}\n", path.display()));
            }
        }

        if let Some(pressure) = &context.runtime_status.context_pressure {
            output.push_str(&format!(
                "context_pressure: {}% - {}\n",
                pressure.used_percent, pressure.message
            ));
        }
    }

    output.push_str("\n# Hints\n");
    if context.hints.is_empty() {
        output.push_str("(none)\n");
    } else {
        for hint in &context.hints {
            output.push_str(&format!("- {}\n", hint.content));
        }
    }

    output.push_str("\n# Recent Chat\n");
    for turn in &context.recent_chat {
        output.push_str(&format!("{:?}: {}\n", turn.role, turn.content));
    }

    output
}

fn build_chat_request(
    context: &AgentContext,
    conversation: &[RequestMessage],
) -> Result<ChatRequest> {
    let mut request = ChatRequest::default();

    for message in conversation {
        let content = message.content.clone().unwrap_or_default();
        match message.role.as_str() {
            "system" => {
                if request.system.is_none() {
                    request = request.with_system(content);
                } else {
                    request = request.append_message(ChatMessage::system(content));
                }
            }
            "user" => request = request.append_message(ChatMessage::user(content)),
            "assistant" => {
                let assistant_message = build_assistant_message(&content, &message.tool_calls)?;
                request = request.append_message(assistant_message);
            }
            "tool" => {
                let tool_call_id = message
                    .tool_call_id
                    .clone()
                    .context("tool message missing tool_call_id")?;
                request = request.append_message(ChatMessage::from(GenAiToolResponse::new(
                    tool_call_id, content,
                )));
            }
            other => bail!("unsupported request role {other}"),
        }
    }

    if !context.tools.is_empty() {
        request = request.with_tools(context.tools.iter().map(translate_tool_definition));
    }

    Ok(request)
}

fn build_assistant_message(
    content: &str,
    tool_calls: &[ApiToolCallRequest],
) -> Result<ChatMessage> {
    if tool_calls.is_empty() {
        return Ok(ChatMessage::assistant(content.to_string()));
    }

    let mut parts = Vec::new();
    if !content.trim().is_empty() {
        parts.push(ContentPart::Text(content.to_string()));
    }
    for tool_call in tool_calls {
        parts.push(ContentPart::ToolCall(GenAiToolCall {
            call_id: tool_call.id.clone(),
            fn_name: tool_call.function.name.clone(),
            fn_arguments: parse_tool_arguments(&tool_call.function.arguments),
            thought_signatures: None,
        }));
    }

    Ok(ChatMessage::assistant(MessageContent::from_parts(parts)))
}

fn parse_tool_arguments(arguments_json: &str) -> Value {
    serde_json::from_str(arguments_json)
        .unwrap_or_else(|_| Value::String(arguments_json.to_string()))
}

fn translate_tool_definition(tool: &ToolDefinition) -> GenAiTool {
    GenAiTool::new(tool.name.to_string())
        .with_description(render_tool_description(tool))
        .with_schema(build_parameters_schema(&tool.parameters))
}

fn render_tool_description(tool: &ToolDefinition) -> String {
    if tool.notes.is_empty() {
        return tool.description.to_string();
    }

    format!(
        "{}\n\nUsage notes:\n- {}",
        tool.description,
        tool.notes.join("\n- ")
    )
}

fn build_parameters_schema(parameters: &[ToolParameter]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for parameter in parameters {
        properties.insert(
            parameter.name.to_string(),
            serde_json::json!({
                "type": json_type_for_parameter(parameter),
                "description": parameter.description,
            }),
        );

        if parameter.required {
            required.push(parameter.name.to_string());
        }
    }

    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn json_type_for_parameter(parameter: &ToolParameter) -> &'static str {
    match parameter.kind {
        "boolean" => "boolean",
        "integer" => "integer",
        "enum" => "string",
        "path" => "string",
        _ => "string",
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct ApiToolCallRequest {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ApiToolFunctionCallRequest,
}

#[derive(Debug, Serialize, Clone)]
pub struct ApiToolFunctionCallRequest {
    pub name: String,
    pub arguments: String,
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

fn debug_structured_response_from_provider(
    response: Option<ProviderResponse>,
) -> DebugStructuredProviderResponse {
    let Some(response) = response else {
        return DebugStructuredProviderResponse {
            content: None,
            tool_calls: Vec::new(),
        };
    };

    DebugStructuredProviderResponse {
        content: response.content,
        tool_calls: response
            .tool_calls
            .into_iter()
            .map(|tool_call| DebugStructuredToolCall {
                id: tool_call.id,
                name: tool_call.name,
                arguments_json: tool_call.arguments_json,
            })
            .collect(),
    }
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
        Value::Number(number) => number.as_u64().and_then(|value| usize::try_from(value).ok()),
        Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_messages, fallback_task_title};
    use crate::context::{
        AgentContext, ContextPressure, DisplayTurn, FileSnapshot, Hint, Injection, NoteEntry, Role,
        RuntimeStatus, SessionStatus,
    };
    use indexmap::IndexMap;
    use std::path::PathBuf;

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
            open_files: vec![FileSnapshot {
                path: PathBuf::from("src/main.rs"),
                content: "fn main() {}".to_string(),
                token_estimate: 4,
                modified_by: None,
            }],
            notes,
            runtime_status: RuntimeStatus {
                locked_files: vec![PathBuf::from("AGENTS.md")],
                context_pressure: Some(ContextPressure {
                    used_tokens: 100,
                    max_tokens: 1000,
                    used_percent: 10,
                    message: "safe".to_string(),
                }),
            },
            hints: vec![Hint {
                source: "CI".to_string(),
                content: "tests red".to_string(),
                expires_in_turns: None,
                expires_at_unix_ms: None,
            }],
            recent_chat: vec![DisplayTurn {
                role: Role::User,
                content: "hello".to_string(),
            }],
            tools: Vec::new(),
        };

        let messages = build_messages(&context);
        assert_eq!(messages.len(), 3);
    }
}
