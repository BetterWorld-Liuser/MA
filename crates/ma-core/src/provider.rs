use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::context::{AgentContext, render_file_snapshot_for_prompt};
use crate::tools::{ToolDefinition, ToolParameter};

/// OpenAI-compatible provider 的最小运行时配置。
/// 当前先保持环境变量注入，避免把 provider 选择和密钥管理耦合进 session 层。
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAiCompatibleConfig {
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("MA_OPENAI_BASE_URL")
            .context("missing MA_OPENAI_BASE_URL environment variable")?;
        let api_key = std::env::var("MA_OPENAI_API_KEY")
            .context("missing MA_OPENAI_API_KEY environment variable")?;
        let model = std::env::var("MA_OPENAI_MODEL")
            .context("missing MA_OPENAI_MODEL environment variable")?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
        })
    }
}

/// Provider 层只负责把 Ma 构建好的上下文翻译成请求，并把模型输出还原成可执行结构。
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    http: Client,
    config: OpenAiCompatibleConfig,
}

impl OpenAiCompatibleClient {
    pub fn new(config: OpenAiCompatibleConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>> {
        Ok(self
            .list_model_descriptors()
            .await?
            .into_iter()
            .map(|model| model.id)
            .collect())
    }

    /// 尽力从 provider 的 `/models` 元数据里读取真实上下文窗口。
    /// OpenAI-compatible 生态并不统一，因此这里只做 best-effort 解析：
    /// - 若供应商返回 `context_window` / `max_input_tokens` 等字段，则直接采用
    /// - 若没有这些字段，则由调用侧决定是否使用本地 fallback
    pub async fn resolve_model_context_window(&self, model_id: &str) -> Result<Option<usize>> {
        let descriptors = self.list_model_descriptors().await?;
        Ok(descriptors
            .into_iter()
            .find(|model| model.id == model_id)
            .and_then(|model| model.context_window_tokens))
    }

    async fn list_model_descriptors(&self) -> Result<Vec<ModelDescriptor>> {
        let response = self
            .http
            .get(format!("{}/models", self.config.base_url))
            .bearer_auth(&self.config.api_key)
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

    /// 这是最小 agent loop 的核心入口：
    /// Ma 先把上下文拆成 messages 与 tools，再逐轮把 provider 响应翻译成文本或 tool calls。
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
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: conversation,
            temperature: Some(0.2),
            stream: true,
            tools: if context.tools.is_empty() {
                None
            } else {
                Some(
                    context
                        .tools
                        .iter()
                        .map(translate_tool_definition)
                        .collect(),
                )
            },
            tool_choice: if context.tools.is_empty() {
                None
            } else {
                Some(ApiToolChoice::Auto)
            },
        };

        let request_json =
            serde_json::to_string_pretty(&request).context("failed to encode chat request")?;
        let mut response = self.send_chat_completion(request, &mut on_event).await?;
        response.request_json = request_json;
        Ok(response)
    }

    async fn send_chat_completion(
        &self,
        request: ChatCompletionRequest,
        on_event: &mut impl FnMut(ProviderProgressEvent) -> Result<()>,
    ) -> Result<ProviderResponse> {
        let response = self
            .http
            .post(format!("{}/chat/completions", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await
            .context("failed to request chat completion")?
            .error_for_status()
            .context("chat completion request failed")?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_string();

        if content_type.contains("text/event-stream") {
            return self
                .read_streaming_chat_completion(response, on_event)
                .await;
        }

        let raw_response = response
            .text()
            .await
            .context("failed to read chat completion response body")?;
        let payload: ChatCompletionResponse = serde_json::from_str(&raw_response)
            .context("failed to decode chat completion response")?;
        provider_response_from_payload(payload, raw_response)
    }

    async fn read_streaming_chat_completion(
        &self,
        response: reqwest::Response,
        on_event: &mut impl FnMut(ProviderProgressEvent) -> Result<()>,
    ) -> Result<ProviderResponse> {
        let mut stream = response.bytes_stream();
        let mut raw_response = String::new();
        let mut buffer = String::new();
        let mut collector = StreamResponseCollector::default();

        while let Some(chunk) = stream.next().await {
            let chunk = chunk.context("failed to read chat completion stream chunk")?;
            let chunk_text = std::str::from_utf8(&chunk)
                .context("chat completion stream chunk was not valid UTF-8")?;
            raw_response.push_str(chunk_text);
            buffer.push_str(chunk_text);

            while let Some(event) = pop_sse_event(&mut buffer) {
                match event {
                    SseEvent::Done => {
                        return collector.finish(raw_response);
                    }
                    SseEvent::Data(data) => {
                        let payload: ChatCompletionChunk = serde_json::from_str(&data)
                            .with_context(|| {
                                format!("failed to decode chat completion stream event: {}", data)
                            })?;
                        let progress = collector.ingest_chunk(payload)?;
                        if !progress.content_delta.is_empty() {
                            on_event(ProviderProgressEvent::ContentDelta(progress.content_delta))?;
                        }
                        if progress.tool_calls_updated {
                            on_event(ProviderProgressEvent::ToolCallsUpdated(
                                collector.tool_call_deltas(),
                            ))?;
                        }
                    }
                }
            }
        }

        if !buffer.trim().is_empty() {
            bail!("chat completion stream ended with incomplete SSE event");
        }

        collector.finish(raw_response)
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

        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: vec![
                RequestMessage::system(
                    "You generate concise task titles for a coding workspace.\n\
                     Return only the title text.\n\
                     Rules:\n\
                     - Prefer Simplified Chinese when the user writes Chinese.\n\
                     - Use 8-18 characters when possible.\n\
                     - Keep the concrete object, such as a file, module, or bug.\n\
                     - Remove filler like '帮我', '请你', '看一下', '继续'.\n\
                     - Do not use quotes, numbering, or trailing punctuation.",
                ),
                RequestMessage::user(format!("First user message:\n{}", trimmed)),
            ],
            temperature: Some(0.1),
            stream: false,
            tools: None,
            tool_choice: None,
        };

        let response = self.send_chat_completion(request, &mut |_| Ok(())).await?;
        Ok(response
            .content
            .as_deref()
            .and_then(sanitize_task_title)
            .or_else(|| fallback_task_title(trimmed)))
    }
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

pub fn format_provider_response_for_debug(raw_response: &str) -> String {
    if raw_response.trim().is_empty() {
        return "(empty response)".to_string();
    }

    if let Ok(payload) = serde_json::from_str::<ChatCompletionResponse>(raw_response) {
        let structured = debug_structured_response_from_provider(
            provider_response_from_payload(payload, String::new()).ok(),
        );
        return serde_json::to_string_pretty(&structured)
            .unwrap_or_else(|_| raw_response.to_string());
    }

    if let Some(response) = parse_sse_response_for_debug(raw_response) {
        let structured = debug_structured_response_from_provider(Some(response));
        return serde_json::to_string_pretty(&structured)
            .unwrap_or_else(|_| raw_response.to_string());
    }

    raw_response.to_string()
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

#[cfg(test)]
#[cfg(test)]
fn translate_tool_runtime(tool_runtime: &crate::tools::ToolRuntime) -> Vec<ApiToolDefinition> {
    tool_runtime
        .tools
        .iter()
        .map(translate_tool_definition)
        .collect()
}

fn translate_tool_definition(tool: &ToolDefinition) -> ApiToolDefinition {
    ApiToolDefinition {
        tool_type: "function".to_string(),
        function: ApiFunctionDefinition {
            name: tool.name.to_string(),
            description: Some(render_tool_description(tool)),
            parameters: build_parameters_schema(&tool.parameters),
        },
    }
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

fn build_parameters_schema(parameters: &[ToolParameter]) -> ApiJsonSchema {
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

    ApiJsonSchema {
        schema_type: "object".to_string(),
        properties: serde_json::Value::Object(properties),
        required,
        additional_properties: false,
    }
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

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<RequestMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ApiToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<ApiToolChoice>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum ApiToolChoice {
    Auto,
}

#[derive(Debug, Serialize)]
struct ApiToolDefinition {
    #[serde(rename = "type")]
    tool_type: String,
    function: ApiFunctionDefinition,
}

#[derive(Debug, Serialize)]
struct ApiFunctionDefinition {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: ApiJsonSchema,
}

#[derive(Debug, Serialize)]
struct ApiJsonSchema {
    #[serde(rename = "type")]
    schema_type: String,
    properties: serde_json::Value,
    required: Vec<String>,
    additional_properties: bool,
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


#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChatChunkChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChunkChoice {
    #[serde(default)]
    delta: ResponseDelta,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    #[serde(default)]
    content: Option<ResponseContent>,
    #[serde(default)]
    tool_calls: Vec<ApiToolCallResponse>,
}

#[derive(Debug, Default, Deserialize)]
struct ResponseDelta {
    #[serde(default)]
    content: Option<ResponseContent>,
    #[serde(default)]
    tool_calls: Vec<ApiToolCallDelta>,
}

impl ResponseMessage {
    fn content_text(&self) -> Option<String> {
        match &self.content {
            None => None,
            Some(ResponseContent::Text(text)) => Some(text.clone()),
            Some(ResponseContent::Parts(parts)) => {
                let text = parts
                    .iter()
                    .filter_map(|part| match part {
                        ResponseContentPart::Text { text } => Some(text.as_str()),
                        ResponseContentPart::Other => None,
                    })
                    .collect::<Vec<_>>()
                    .join("");

                if text.is_empty() { None } else { Some(text) }
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ResponseContent {
    Text(String),
    Parts(Vec<ResponseContentPart>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
enum ResponseContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize)]
struct ApiToolCallResponse {
    id: String,
    function: ApiToolFunctionCallResponse,
}

#[derive(Debug, Deserialize)]
struct ApiToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<ApiToolFunctionCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ApiToolFunctionCallResponse {
    name: String,
    arguments: String,
}

#[derive(Debug, Default, Deserialize)]
struct ApiToolFunctionCallDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Default)]
struct StreamResponseCollector {
    content: String,
    tool_calls: Vec<StreamToolCallAccumulator>,
}

#[derive(Debug, Default)]
struct StreamToolCallAccumulator {
    id: Option<String>,
    name: String,
    arguments_json: String,
}

#[derive(Debug, Default)]
struct ChunkProgress {
    content_delta: String,
    tool_calls_updated: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum SseEvent {
    Data(String),
    Done,
}

fn provider_response_from_payload(
    payload: ChatCompletionResponse,
    raw_response: String,
) -> Result<ProviderResponse> {
    let Some(choice) = payload.choices.into_iter().next() else {
        bail!("chat completion returned no choices");
    };

    Ok(ProviderResponse {
        content: choice.message.content_text(),
        tool_calls: choice
            .message
            .tool_calls
            .into_iter()
            .map(|tool_call| ProviderToolCall {
                id: tool_call.id,
                name: tool_call.function.name,
                arguments_json: tool_call.function.arguments,
            })
            .collect(),
        request_json: String::new(),
        raw_response,
    })
}

impl StreamResponseCollector {
    fn ingest_chunk(&mut self, chunk: ChatCompletionChunk) -> Result<ChunkProgress> {
        let mut progress = ChunkProgress::default();

        for choice in chunk.choices {
            if let Some(text) = choice.delta.content_text() {
                self.content.push_str(&text);
                progress.content_delta.push_str(&text);
            }

            for tool_call in choice.delta.tool_calls {
                progress.tool_calls_updated = true;
                while self.tool_calls.len() <= tool_call.index {
                    self.tool_calls.push(StreamToolCallAccumulator::default());
                }

                let accumulator = self
                    .tool_calls
                    .get_mut(tool_call.index)
                    .context("tool call index out of range while collecting stream")?;

                if let Some(id) = tool_call.id {
                    accumulator.id = Some(id);
                }

                if let Some(function) = tool_call.function {
                    if let Some(name) = function.name {
                        accumulator.name.push_str(&name);
                    }
                    if let Some(arguments) = function.arguments {
                        accumulator.arguments_json.push_str(&arguments);
                    }
                }
            }
        }

        Ok(progress)
    }

    fn finish(self, raw_response: String) -> Result<ProviderResponse> {
        let tool_calls = self
            .tool_calls
            .into_iter()
            .map(|tool_call| {
                let id = tool_call
                    .id
                    .filter(|value| !value.trim().is_empty())
                    .context("streamed tool call missing id")?;
                let name = if tool_call.name.trim().is_empty() {
                    bail!("streamed tool call missing function name");
                } else {
                    tool_call.name
                };

                Ok(ProviderToolCall {
                    id,
                    name,
                    arguments_json: tool_call.arguments_json,
                })
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(ProviderResponse {
            content: if self.content.is_empty() {
                None
            } else {
                Some(self.content)
            },
            tool_calls,
            request_json: String::new(),
            raw_response,
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

impl ResponseDelta {
    fn content_text(&self) -> Option<String> {
        ResponseMessage {
            content: self.content.clone(),
            tool_calls: Vec::new(),
        }
        .content_text()
    }
}

fn pop_sse_event(buffer: &mut String) -> Option<SseEvent> {
    let delimiter_len = if let Some(index) = buffer.find("\r\n\r\n") {
        Some((index, 4))
    } else {
        buffer.find("\n\n").map(|index| (index, 2))
    }?;

    let (index, separator_width) = delimiter_len;
    let raw_event = buffer[..index].to_string();
    buffer.drain(..index + separator_width);

    let mut data_lines = Vec::new();
    for line in raw_event.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(payload) = line.strip_prefix("data:") {
            data_lines.push(payload.trim_start().to_string());
        }
    }

    if data_lines.is_empty() {
        return pop_sse_event(buffer);
    }

    let data = data_lines.join("\n");
    if data == "[DONE]" {
        Some(SseEvent::Done)
    } else {
        Some(SseEvent::Data(data))
    }
}

fn parse_sse_response_for_debug(raw_response: &str) -> Option<ProviderResponse> {
    // Debug 面板更关心“这条流最终拼出了什么”，所以这里直接重建最终响应结构。
    let mut buffer = raw_response.to_string();
    let mut collector = StreamResponseCollector::default();

    while let Some(event) = pop_sse_event(&mut buffer) {
        match event {
            SseEvent::Done => {
                return collector.finish(String::new()).ok();
            }
            SseEvent::Data(data) => {
                let payload: ChatCompletionChunk = serde_json::from_str(&data).ok()?;
                collector.ingest_chunk(payload).ok()?;
            }
        }
    }

    None
}

fn debug_structured_response_from_provider(
    response: Option<ProviderResponse>,
) -> DebugStructuredProviderResponse {
    let response = response.unwrap_or(ProviderResponse {
        content: None,
        tool_calls: Vec::new(),
        request_json: String::new(),
        raw_response: String::new(),
    });

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
        let id = value
            .get("id")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .context("provider model entry missing string id")?;

        Ok(Self {
            context_window_tokens: extract_context_window_tokens(&value),
            id,
        })
    }
}

fn extract_context_window_tokens(value: &Value) -> Option<usize> {
    for key in [
        "context_window",
        "context_length",
        "max_context_tokens",
        "max_input_tokens",
        "input_token_limit",
    ] {
        if let Some(tokens) = value.get(key).and_then(parse_token_limit_value) {
            return Some(tokens);
        }
    }

    for container in ["capabilities", "limits", "architecture", "metadata"] {
        let Some(nested) = value.get(container) else {
            continue;
        };
        for key in [
            "context_window",
            "context_length",
            "max_context_tokens",
            "max_input_tokens",
            "input_token_limit",
        ] {
            if let Some(tokens) = nested.get(key).and_then(parse_token_limit_value) {
                return Some(tokens);
            }
        }
    }

    None
}

fn parse_token_limit_value(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        Value::String(text) => parse_human_token_limit(text),
        _ => None,
    }
}

fn parse_human_token_limit(text: &str) -> Option<usize> {
    let normalized = text.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if let Some(stripped) = normalized.strip_suffix('k') {
        return stripped
            .trim()
            .parse::<usize>()
            .ok()
            .map(|value| value * 1_000);
    }

    if let Some(stripped) = normalized.strip_suffix('m') {
        return stripped
            .trim()
            .parse::<usize>()
            .ok()
            .map(|value| value * 1_000_000);
    }

    normalized.parse::<usize>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{
        AgentContext, ChatTurn, FileSnapshot, Injection, ModifiedBy, NoteEntry, Role,
        SessionStatus, SystemStatus,
    };
    use indexmap::IndexMap;

    #[test]
    fn build_messages_preserves_injections_and_context_layers() {
        let mut open_files = IndexMap::new();
        open_files.insert(
            "src/main.rs".into(),
            FileSnapshot::available(
                "src/main.rs",
                "fn main() {}",
                std::time::SystemTime::UNIX_EPOCH,
                ModifiedBy::Unknown,
            ),
        );

        let mut notes = IndexMap::new();
        notes.insert("target".to_string(), NoteEntry::new("demo"));

        let context = AgentContext {
            system_core: "system core".to_string(),
            injections: vec![Injection {
                id: "skill:test".to_string(),
                content: "injection body".to_string(),
            }],
            tools: Vec::new(),
            open_files,
            notes,
            session_status: SessionStatus {
                workspace_root: "D:/playground/MA".into(),
                platform: "Windows".to_string(),
                shell: "powershell".to_string(),
                available_shells: vec!["powershell".to_string(), "cmd".to_string()],
                workspace_entries: vec!["design/".to_string(), "src/".to_string()],
            },
            runtime_status: SystemStatus::default(),
            hints: Vec::new(),
            recent_chat: vec![ChatTurn {
                role: Role::User,
                content: "hello".to_string(),
                timestamp: std::time::SystemTime::UNIX_EPOCH,
            }],
        };

        let messages = build_messages(&context);

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content.as_deref(), Some("system core"));
        assert!(
            messages[1]
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("skill:test")
        );
        assert!(
            messages[2]
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("# Session Status")
        );
        assert!(
            messages[2]
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("workspace_root: D:/playground/MA")
        );
        assert!(
            messages[2]
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("# Open Files")
        );
        assert!(
            messages[2]
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("watcher-backed snapshots are the authoritative current file contents")
        );
        assert!(
            messages[2]
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("# Notes")
        );
        assert!(
            messages[2]
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("# Runtime Status")
        );
        assert!(
            messages[2]
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("# Hints")
        );
        assert!(
            messages[2]
                .content
                .as_deref()
                .unwrap_or_default()
                .contains("# Recent Chat")
        );
    }

    #[test]
    fn tool_request_schema_is_serializable() {
        let runtime = crate::tools::ToolRuntime { tools: Vec::new() };
        let payload = serde_json::to_value(translate_tool_runtime(&runtime)).expect("json");

        assert!(payload.is_array());
    }

    #[test]
    fn chat_completion_request_serializes_tool_choice_when_tools_exist() {
        let request = ChatCompletionRequest {
            model: "gpt-5.3-codex".to_string(),
            messages: vec![RequestMessage::user("hello")],
            temperature: Some(0.2),
            stream: true,
            tools: Some(vec![ApiToolDefinition {
                tool_type: "function".to_string(),
                function: ApiFunctionDefinition {
                    name: "open_file".to_string(),
                    description: Some("Open a file".to_string()),
                    parameters: ApiJsonSchema {
                        schema_type: "object".to_string(),
                        properties: serde_json::json!({}),
                        required: Vec::new(),
                        additional_properties: false,
                    },
                },
            }]),
            tool_choice: Some(ApiToolChoice::Auto),
        };

        let payload = serde_json::to_value(request).expect("serialize request");

        assert_eq!(payload["tool_choice"], "auto");
        assert_eq!(payload["tools"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn response_message_extracts_text_from_parts() {
        let message = ResponseMessage {
            content: Some(ResponseContent::Parts(vec![ResponseContentPart::Text {
                text: "hello".to_string(),
            }])),
            tool_calls: Vec::new(),
        };

        assert_eq!(message.content_text().as_deref(), Some("hello"));
    }

    #[test]
    fn assistant_tool_call_message_can_carry_text_and_tools() {
        let message = RequestMessage::assistant_tool_calls_with_text(
            Some("Working on it".to_string()),
            vec![ApiToolCallRequest {
                id: "call_1".to_string(),
                tool_type: "function".to_string(),
                function: ApiToolFunctionCallRequest {
                    name: "reply".to_string(),
                    arguments: "{\"message\":\"hi\",\"wait\":true}".to_string(),
                },
            }],
        );

        let payload = serde_json::to_value(message).expect("serialize request message");

        assert_eq!(payload["role"], "assistant");
        assert_eq!(payload["content"], "Working on it");
        assert_eq!(payload["tool_calls"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn stream_parser_collects_text_across_sse_events() {
        let mut buffer = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\n",
            "data: [DONE]\n\n"
        )
        .to_string();
        let mut collector = StreamResponseCollector::default();

        while let Some(event) = pop_sse_event(&mut buffer) {
            match event {
                SseEvent::Done => break,
                SseEvent::Data(data) => {
                    let payload: ChatCompletionChunk =
                        serde_json::from_str(&data).expect("stream chunk json");
                    collector
                        .ingest_chunk(payload)
                        .expect("ingest stream chunk");
                }
            }
        }

        let response = collector.finish(String::new()).expect("finish stream");
        assert_eq!(response.content.as_deref(), Some("Hello"));
        assert!(response.tool_calls.is_empty());
    }

    #[test]
    fn stream_parser_collects_tool_call_arguments_across_sse_events() {
        let mut buffer = concat!(
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"reply\",\"arguments\":\"{\\\"message\\\":\\\"he\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"llo\\\",\\\"wait\\\":true}\"}}]}}]}\n\n",
            "data: [DONE]\n\n"
        )
        .to_string();
        let mut collector = StreamResponseCollector::default();

        while let Some(event) = pop_sse_event(&mut buffer) {
            match event {
                SseEvent::Done => break,
                SseEvent::Data(data) => {
                    let payload: ChatCompletionChunk =
                        serde_json::from_str(&data).expect("stream chunk json");
                    collector
                        .ingest_chunk(payload)
                        .expect("ingest stream chunk");
                }
            }
        }

        let response = collector.finish(String::new()).expect("finish stream");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_1");
        assert_eq!(response.tool_calls[0].name, "reply");
        assert_eq!(
            response.tool_calls[0].arguments_json,
            "{\"message\":\"hello\",\"wait\":true}"
        );
    }

    #[test]
    fn debug_formatter_converts_sse_stream_into_structured_json() {
        let raw = concat!(
            "data: {\"choices\":[{\"delta\":{\"content\":\"你\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"好\"}}]}\n\n",
            "data: [DONE]\n\n"
        );

        let formatted = format_provider_response_for_debug(raw);
        let payload: Value = serde_json::from_str(&formatted).expect("formatted debug response json");

        assert_eq!(payload["content"], "你好");
        assert_eq!(payload["tool_calls"].as_array().map(Vec::len), Some(0));
    }

    #[test]
    fn fallback_task_title_removes_filler_prefix() {
        assert_eq!(
            fallback_task_title("帮我看看 main.rs 这里有没有问题").as_deref(),
            Some("看看 main.rs 这里有没有问题")
        );
    }

    #[test]
    fn sanitize_task_title_trims_decoration() {
        assert_eq!(
            sanitize_task_title("标题：重构登录模块并补测试。").as_deref(),
            Some("重构登录模块并补测试")
        );
    }
}
