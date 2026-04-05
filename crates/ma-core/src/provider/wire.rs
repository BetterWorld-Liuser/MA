use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Map, Value, json};

use crate::settings::{ProviderType, ServerToolConfig, ServerToolFormat};

use super::messages::{
    FunctionToolDefinition, MessageContent, MessageContentPart, RequestMessage, RequestOptions,
    serialize_tool_arguments, server_tool_definition, validate_messages,
};
use super::transport::provider_base_url;
use super::RuntimeProviderConfig;

const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 16_384;

pub(super) struct WireRequest {
    pub url: String,
    pub headers: HeaderMap,
    pub body: Value,
}

#[derive(Debug, Clone)]
pub(super) struct WireResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<WireToolCall>,
}

#[derive(Debug, Clone)]
pub(super) struct WireToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone)]
pub(super) enum WireStreamDelta {
    ContentDelta(String),
    ToolCallDelta {
        index: usize,
        id: Option<String>,
        name: Option<String>,
        arguments_fragment: String,
    },
    Done,
}

pub(super) trait WireAdapter: Send + Sync {
    fn build_request(
        &self,
        config: &RuntimeProviderConfig,
        messages: &[RequestMessage],
        tools: &[FunctionToolDefinition],
        server_tools: &[ServerToolConfig],
        options: &RequestOptions,
    ) -> Result<WireRequest>;

    fn parse_response(&self, body: &Value) -> Result<WireResponse>;

    fn parse_stream_event(
        &self,
        event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<WireStreamDelta>>;

    fn is_stream_done(&self, event_name: Option<&str>, data: &str) -> bool;
}

pub(super) fn adapter_for(
    config: &RuntimeProviderConfig,
) -> Box<dyn WireAdapter + Send + Sync + 'static> {
    match config.provider_type {
        ProviderType::Anthropic => Box::new(AnthropicWire),
        ProviderType::Gemini => Box::new(GeminiWire),
        _ => Box::new(OpenAiWire),
    }
}

struct OpenAiWire;
struct AnthropicWire;
struct GeminiWire;

impl WireAdapter for OpenAiWire {
    fn build_request(
        &self,
        config: &RuntimeProviderConfig,
        messages: &[RequestMessage],
        tools: &[FunctionToolDefinition],
        server_tools: &[ServerToolConfig],
        options: &RequestOptions,
    ) -> Result<WireRequest> {
        validate_messages(messages)?;

        let mut tool_defs = tools
            .iter()
            .map(|tool| {
                json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }
                })
            })
            .collect::<Vec<_>>();
        tool_defs.extend(server_tools.iter().map(server_tool_definition));

        let mut body = json!({
            "model": options.model,
            "messages": messages
                .iter()
                .map(serialize_openai_message)
                .collect::<Result<Vec<_>>>()?,
            "temperature": options.temperature,
            "stream": options.stream,
        });
        if !tool_defs.is_empty() {
            body["tools"] = Value::Array(tool_defs);
        }
        if let Some(max_output_tokens) = options.max_output_tokens {
            body["max_tokens"] = json!(max_output_tokens);
        }

        let mut headers = json_headers();
        apply_bearer_auth(&mut headers, &config.api_key)?;

        Ok(WireRequest {
            url: format!("{}/chat/completions", provider_base_url(config)),
            headers,
            body,
        })
    }

    fn parse_response(&self, body: &Value) -> Result<WireResponse> {
        let message = body
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .context("provider response missing choices[0].message")?;

        Ok(WireResponse {
            content: openai_message_content(message.get("content")),
            tool_calls: parse_openai_tool_calls(message.get("tool_calls"))?,
        })
    }

    fn parse_stream_event(
        &self,
        _event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<WireStreamDelta>> {
        if data.trim() == "[DONE]" {
            return Ok(vec![WireStreamDelta::Done]);
        }

        let body: Value =
            serde_json::from_str(data).context("failed to decode OpenAI-compatible stream event")?;
        let Some(delta) = body
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta"))
        else {
            return Ok(Vec::new());
        };

        let mut events = Vec::new();
        if let Some(text) = delta.get("content").and_then(Value::as_str) {
            if !text.is_empty() {
                events.push(WireStreamDelta::ContentDelta(text.to_string()));
            }
        }

        if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for tool_call in tool_calls {
                let index = tool_call
                    .get("index")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(0);
                let id = tool_call
                    .get("id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let name = tool_call
                    .get("function")
                    .and_then(|value| value.get("name"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let arguments_fragment = tool_call
                    .get("function")
                    .and_then(|value| value.get("arguments"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                events.push(WireStreamDelta::ToolCallDelta {
                    index,
                    id,
                    name,
                    arguments_fragment,
                });
            }
        }

        Ok(events)
    }

    fn is_stream_done(&self, _event_name: Option<&str>, data: &str) -> bool {
        data.trim() == "[DONE]"
    }
}

impl WireAdapter for AnthropicWire {
    fn build_request(
        &self,
        config: &RuntimeProviderConfig,
        messages: &[RequestMessage],
        tools: &[FunctionToolDefinition],
        server_tools: &[ServerToolConfig],
        options: &RequestOptions,
    ) -> Result<WireRequest> {
        validate_messages(messages)?;

        let mut system_blocks = Vec::new();
        let mut body_messages = Vec::new();

        for message in messages {
            match message.role.as_str() {
                "system" => system_blocks.extend(
                    serialize_anthropic_blocks(message.content.as_ref())?,
                ),
                "user" => body_messages.push(json!({
                    "role": "user",
                    "content": serialize_anthropic_blocks(message.content.as_ref())?,
                })),
                "assistant" => {
                    let mut blocks: Vec<Value> =
                        serialize_anthropic_blocks(message.content.as_ref())?;
                    for tool_call in &message.tool_calls {
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": tool_call.id,
                            "name": tool_call.function.name,
                            "input": serialize_tool_arguments(&tool_call.function.arguments),
                        }));
                    }
                    body_messages.push(json!({
                        "role": "assistant",
                        "content": blocks,
                    }));
                }
                "tool" => {
                    let content = message
                        .content
                        .as_ref()
                        .and_then(MessageContent::joined_texts)
                        .unwrap_or_default();
                    body_messages.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": message.tool_call_id.clone().context("tool message missing tool_call_id")?,
                            "content": content,
                        }],
                    }));
                }
                other => bail!("unsupported request role {other}"),
            }
        }

        let mut tool_defs = tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.parameters,
                })
            })
            .collect::<Vec<_>>();
        tool_defs.extend(
            server_tools
                .iter()
                .filter(|tool| tool.format == ServerToolFormat::Anthropic)
                .map(server_tool_definition),
        );

        let mut body = json!({
            "model": options.model,
            "messages": body_messages,
            "temperature": options.temperature,
            "stream": options.stream,
            "max_tokens": options.max_output_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
        });
        if !system_blocks.is_empty() {
            body["system"] = Value::Array(system_blocks);
        }
        if !tool_defs.is_empty() {
            body["tools"] = Value::Array(tool_defs);
        }

        let mut headers = json_headers();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&config.api_key).context("invalid anthropic api key header")?,
        );
        headers.insert(
            "anthropic-version",
            HeaderValue::from_static("2023-06-01"),
        );

        Ok(WireRequest {
            url: format!("{}/messages", provider_base_url(config)),
            headers,
            body,
        })
    }

    fn parse_response(&self, body: &Value) -> Result<WireResponse> {
        let content_blocks = body
            .get("content")
            .and_then(Value::as_array)
            .context("anthropic response missing content blocks")?;

        let mut text = String::new();
        let mut tool_calls = Vec::new();
        for block in content_blocks {
            match block.get("type").and_then(Value::as_str).unwrap_or_default() {
                "text" => {
                    if let Some(value) = block.get("text").and_then(Value::as_str) {
                        text.push_str(value);
                    }
                }
                "tool_use" => {
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .context("anthropic tool_use missing id")?;
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .context("anthropic tool_use missing name")?;
                    let arguments_json = serde_json::to_string(
                        block.get("input").unwrap_or(&Value::Null),
                    )
                    .context("failed to encode anthropic tool_use input")?;
                    tool_calls.push(WireToolCall {
                        id: id.to_string(),
                        name: name.to_string(),
                        arguments_json,
                    });
                }
                _ => {}
            }
        }

        Ok(WireResponse {
            content: (!text.trim().is_empty()).then_some(text),
            tool_calls,
        })
    }

    fn parse_stream_event(
        &self,
        event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<WireStreamDelta>> {
        let event_name = event_name.unwrap_or_default();
        if matches!(event_name, "message_stop" | "message_delta") && data.trim().is_empty() {
            return Ok(Vec::new());
        }
        if event_name == "message_stop" {
            return Ok(vec![WireStreamDelta::Done]);
        }

        let payload: Value =
            serde_json::from_str(data).context("failed to decode anthropic stream event")?;
        match event_name {
            "content_block_start" => {
                let block = payload
                    .get("content_block")
                    .context("anthropic content_block_start missing content_block")?;
                if block.get("type").and_then(Value::as_str) == Some("tool_use") {
                    let index = payload
                        .get("index")
                        .and_then(Value::as_u64)
                        .and_then(|value| usize::try_from(value).ok())
                        .unwrap_or(0);
                    let id = block.get("id").and_then(Value::as_str).map(ToOwned::to_owned);
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    let initial_input = block
                        .get("input")
                        .filter(|value| !value.is_null())
                        .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string()))
                        .unwrap_or_default();
                    return Ok(vec![WireStreamDelta::ToolCallDelta {
                        index,
                        id,
                        name,
                        arguments_fragment: initial_input,
                    }]);
                }
                Ok(Vec::new())
            }
            "content_block_delta" => {
                let index = payload
                    .get("index")
                    .and_then(Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .unwrap_or(0);
                let delta = payload
                    .get("delta")
                    .context("anthropic content_block_delta missing delta")?;
                match delta.get("type").and_then(Value::as_str).unwrap_or_default() {
                    "text_delta" => Ok(delta
                        .get("text")
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty())
                        .map(|text| vec![WireStreamDelta::ContentDelta(text.to_string())])
                        .unwrap_or_default()),
                    "input_json_delta" => Ok(vec![WireStreamDelta::ToolCallDelta {
                        index,
                        id: None,
                        name: None,
                        arguments_fragment: delta
                            .get("partial_json")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    }]),
                    _ => Ok(Vec::new()),
                }
            }
            _ => Ok(Vec::new()),
        }
    }

    fn is_stream_done(&self, event_name: Option<&str>, _data: &str) -> bool {
        event_name == Some("message_stop")
    }
}

impl WireAdapter for GeminiWire {
    fn build_request(
        &self,
        config: &RuntimeProviderConfig,
        messages: &[RequestMessage],
        tools: &[FunctionToolDefinition],
        server_tools: &[ServerToolConfig],
        options: &RequestOptions,
    ) -> Result<WireRequest> {
        validate_messages(messages)?;

        let mut system_parts = Vec::new();
        let mut contents = Vec::new();
        let mut tool_names = std::collections::HashMap::new();

        for message in messages {
            match message.role.as_str() {
                "system" => system_parts.extend(serialize_gemini_parts(message.content.as_ref())?),
                "user" => contents.push(json!({
                    "role": "user",
                    "parts": serialize_gemini_parts(message.content.as_ref())?,
                })),
                "assistant" => {
                    let mut parts = serialize_gemini_parts(message.content.as_ref())?;
                    for tool_call in &message.tool_calls {
                        tool_names.insert(tool_call.id.clone(), tool_call.function.name.clone());
                        parts.push(json!({
                            "functionCall": {
                                "name": tool_call.function.name,
                                "args": serialize_tool_arguments(&tool_call.function.arguments),
                            }
                        }));
                    }
                    contents.push(json!({
                        "role": "model",
                        "parts": parts,
                    }));
                }
                "tool" => {
                    let name = tool_names
                        .get(
                            message
                                .tool_call_id
                                .as_deref()
                                .context("tool message missing tool_call_id")?,
                        )
                        .cloned()
                        .unwrap_or_else(|| "tool_result".to_string());
                    let content = message
                        .content
                        .as_ref()
                        .and_then(MessageContent::joined_texts)
                        .unwrap_or_default();
                    contents.push(json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": name,
                                "response": { "result": content }
                            }
                        }],
                    }));
                }
                other => bail!("unsupported request role {other}"),
            }
        }

        let mut tool_defs = Vec::new();
        if !tools.is_empty() {
            tool_defs.push(json!({
                "functionDeclarations": tools
                    .iter()
                    .map(|tool| json!({
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.parameters,
                    }))
                    .collect::<Vec<_>>(),
            }));
        }
        tool_defs.extend(
            server_tools
                .iter()
                .filter(|tool| tool.format == ServerToolFormat::Gemini)
                .map(server_tool_definition),
        );

        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": options.temperature,
                "maxOutputTokens": options.max_output_tokens.unwrap_or(DEFAULT_MAX_OUTPUT_TOKENS),
            },
        });
        if !system_parts.is_empty() {
            body["system_instruction"] = json!({ "parts": system_parts });
        }
        if !tool_defs.is_empty() {
            body["tools"] = Value::Array(tool_defs);
        }

        let action = if options.stream {
            "streamGenerateContent?alt=sse"
        } else {
            "generateContent"
        };
        let mut url = format!(
            "{}/models/{}:{}",
            provider_base_url(config),
            options.model,
            action
        );
        if !config.api_key.trim().is_empty() {
            let separator = if url.contains('?') { '&' } else { '?' };
            url.push(separator);
            url.push_str("key=");
            url.push_str(&config.api_key);
        }

        Ok(WireRequest {
            url,
            headers: json_headers(),
            body,
        })
    }

    fn parse_response(&self, body: &Value) -> Result<WireResponse> {
        let parts = body
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|candidates| candidates.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
            .context("gemini response missing candidates[0].content.parts")?;

        parse_gemini_parts(parts)
    }

    fn parse_stream_event(
        &self,
        _event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<WireStreamDelta>> {
        let body: Value =
            serde_json::from_str(data).context("failed to decode gemini stream event")?;
        let parts = body
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|candidates| candidates.first())
            .and_then(|candidate| candidate.get("content"))
            .and_then(|content| content.get("parts"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut events = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                if !text.is_empty() {
                    events.push(WireStreamDelta::ContentDelta(text.to_string()));
                }
            }
            if let Some(function_call) = part.get("functionCall") {
                let name = function_call
                    .get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                let arguments_fragment = function_call
                    .get("args")
                    .map(|value| serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string()))
                    .unwrap_or_default();
                events.push(WireStreamDelta::ToolCallDelta {
                    index,
                    id: None,
                    name,
                    arguments_fragment,
                });
            }
        }

        Ok(events)
    }

    fn is_stream_done(&self, _event_name: Option<&str>, _data: &str) -> bool {
        false
    }
}

fn serialize_openai_message(message: &RequestMessage) -> Result<Value> {
    let mut object = Map::new();
    object.insert("role".to_string(), Value::String(message.role.clone()));

    match message.role.as_str() {
        "system" | "user" | "assistant" => {
            object.insert(
                "content".to_string(),
                serialize_openai_content(message.content.as_ref()),
            );
        }
        "tool" => {
            object.insert(
                "content".to_string(),
                Value::String(
                    message
                        .content
                        .as_ref()
                        .and_then(MessageContent::joined_texts)
                        .unwrap_or_default(),
                ),
            );
            object.insert(
                "tool_call_id".to_string(),
                Value::String(
                    message
                        .tool_call_id
                        .clone()
                        .context("tool message missing tool_call_id")?,
                ),
            );
        }
        other => bail!("unsupported OpenAI message role {other}"),
    }

    if message.role == "assistant" && !message.tool_calls.is_empty() {
        object.insert(
            "tool_calls".to_string(),
            Value::Array(
                message
                    .tool_calls
                    .iter()
                    .map(|tool_call| {
                        json!({
                            "id": tool_call.id,
                            "type": tool_call.tool_type,
                            "function": {
                                "name": tool_call.function.name,
                                "arguments": tool_call.function.arguments,
                            }
                        })
                    })
                    .collect(),
            ),
        );
    }

    Ok(Value::Object(object))
}

fn serialize_openai_content(content: Option<&MessageContent>) -> Value {
    let Some(content) = content else {
        return Value::Null;
    };
    if content.parts().iter().all(|part| matches!(part, MessageContentPart::Text(_))) {
        return Value::String(content.joined_texts().unwrap_or_default());
    }

    Value::Array(
        content
            .parts()
            .iter()
            .map(|part| match part {
                MessageContentPart::Text(text) => json!({
                    "type": "text",
                    "text": text,
                }),
                MessageContentPart::Image {
                    media_type,
                    data_base64,
                    ..
                } => json!({
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:{};base64,{}", media_type, data_base64),
                    }
                }),
            })
            .collect(),
    )
}

fn serialize_anthropic_blocks(content: Option<&MessageContent>) -> Result<Vec<Value>> {
    Ok(content
        .map(|content| {
            content
                .parts()
                .iter()
                .map(|part| match part {
                    MessageContentPart::Text(text) => Ok(json!({
                        "type": "text",
                        "text": text,
                    })),
                    MessageContentPart::Image {
                        media_type,
                        data_base64,
                        ..
                    } => Ok(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data_base64,
                        }
                    })),
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default())
}

fn serialize_gemini_parts(content: Option<&MessageContent>) -> Result<Vec<Value>> {
    Ok(content
        .map(|content| {
            content
                .parts()
                .iter()
                .map(|part| match part {
                    MessageContentPart::Text(text) => Ok(json!({ "text": text })),
                    MessageContentPart::Image {
                        media_type,
                        data_base64,
                        ..
                    } => Ok(json!({
                        "inlineData": {
                            "mimeType": media_type,
                            "data": data_base64,
                        }
                    })),
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default())
}

fn openai_message_content(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => (!text.trim().is_empty()).then_some(text.clone()),
        Some(Value::Array(parts)) => {
            let joined = parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            (!joined.trim().is_empty()).then_some(joined)
        }
        _ => None,
    }
}

fn parse_openai_tool_calls(value: Option<&Value>) -> Result<Vec<WireToolCall>> {
    value
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|tool_call| {
            let id = tool_call
                .get("id")
                .and_then(Value::as_str)
                .context("OpenAI tool call missing id")?;
            let function = tool_call
                .get("function")
                .context("OpenAI tool call missing function")?;
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .context("OpenAI tool call missing function name")?;
            let arguments_json = function
                .get("arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}")
                .to_string();
            Ok(WireToolCall {
                id: id.to_string(),
                name: name.to_string(),
                arguments_json,
            })
        })
        .collect()
}

fn parse_gemini_parts(parts: &[Value]) -> Result<WireResponse> {
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    for (index, part) in parts.iter().enumerate() {
        if let Some(value) = part.get("text").and_then(Value::as_str) {
            text.push_str(value);
        }
        if let Some(function_call) = part.get("functionCall") {
            let name = function_call
                .get("name")
                .and_then(Value::as_str)
                .context("gemini functionCall missing name")?;
            let arguments_json = serde_json::to_string(
                function_call
                    .get("args")
                    .unwrap_or(&Value::Null),
            )
            .context("failed to encode gemini functionCall args")?;
            tool_calls.push(WireToolCall {
                id: format!("gemini-tool-{index}"),
                name: name.to_string(),
                arguments_json,
            });
        }
    }

    Ok(WireResponse {
        content: (!text.trim().is_empty()).then_some(text),
        tool_calls,
    })
}

fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers
}

fn apply_bearer_auth(headers: &mut HeaderMap, api_key: &str) -> Result<()> {
    if api_key.trim().is_empty() {
        return Ok(());
    }
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", api_key))
            .context("invalid bearer authorization header")?,
    );
    Ok(())
}
