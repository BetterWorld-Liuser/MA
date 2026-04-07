use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Map, Value, json};

use crate::settings::{ServerToolConfig, ServerToolFormat};

use super::RuntimeProviderConfig;
use super::messages::{
    FunctionToolDefinition, MessageContent, MessageContentPart, RequestMessage, RequestOptions,
    serialize_tool_arguments, server_tool_definition, validate_messages,
};
use super::transport::provider_base_url;

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
    if config.provider_type.uses_anthropic_api() {
        Box::new(AnthropicWire)
    } else if config.provider_type.uses_gemini_api() {
        Box::new(GeminiWire)
    } else {
        Box::new(OpenAiWire)
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

        if should_use_openai_responses_api(config, server_tools) {
            return build_openai_responses_request(config, messages, tools, server_tools, options);
        }

        let mut tool_defs = tools
            .iter()
            .map(openai_chat_function_tool)
            .collect::<Vec<_>>();
        tool_defs.extend(
            server_tools
                .iter()
                .filter(|tool| tool.format == ServerToolFormat::OpenAiChatCompletions)
                .map(server_tool_definition),
        );

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
        insert_optional_json_field(&mut body, "top_p", options.top_p);
        insert_optional_json_field(&mut body, "presence_penalty", options.presence_penalty);
        insert_optional_json_field(&mut body, "frequency_penalty", options.frequency_penalty);
        insert_optional_json_field(&mut body, "max_tokens", options.max_output_tokens);

        let mut headers = json_headers();
        apply_bearer_auth(&mut headers, &config.api_key)?;

        Ok(WireRequest {
            url: format!("{}/chat/completions", provider_base_url(config)),
            headers,
            body,
        })
    }

    fn parse_response(&self, body: &Value) -> Result<WireResponse> {
        if body.get("object").and_then(Value::as_str) == Some("response") {
            return parse_openai_responses_response(body);
        }

        let message = body
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .context("provider response missing choices[0].message")?;

        Ok(WireResponse {
            content: openai_message_text(message),
            tool_calls: parse_openai_tool_calls(message.get("tool_calls"))?,
        })
    }

    fn parse_stream_event(
        &self,
        event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<WireStreamDelta>> {
        if matches!(
            event_name,
            Some("response.output_item.added")
                | Some("response.function_call_arguments.delta")
                | Some("response.function_call_arguments.done")
                | Some("response.output_text.delta")
                | Some("response.completed")
        ) {
            return parse_openai_responses_stream_event(data);
        }

        if data.trim() == "[DONE]" {
            return Ok(vec![WireStreamDelta::Done]);
        }

        let body: Value = serde_json::from_str(data)
            .context("failed to decode OpenAI-compatible stream event")?;
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

    fn is_stream_done(&self, event_name: Option<&str>, data: &str) -> bool {
        matches!(event_name, Some("response.completed")) || data.trim() == "[DONE]"
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
                "system" => {
                    system_blocks.extend(serialize_anthropic_blocks(message.content.as_ref())?)
                }
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
        if let Some(top_p) = options.top_p {
            body["top_p"] = json!(top_p);
        }
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
        headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

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
            match block
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
            {
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
                    let arguments_json =
                        serde_json::to_string(block.get("input").unwrap_or(&Value::Null))
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
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned);
                    let initial_input = block
                        .get("input")
                        .filter(|value| !value.is_null())
                        .map(|value| {
                            serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
                        })
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
                match delta
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                {
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
        if let Some(top_p) = options.top_p {
            body["generationConfig"]["topP"] = json!(top_p);
        }
        if let Some(presence_penalty) = options.presence_penalty {
            body["generationConfig"]["presencePenalty"] = json!(presence_penalty);
        }
        if let Some(frequency_penalty) = options.frequency_penalty {
            body["generationConfig"]["frequencyPenalty"] = json!(frequency_penalty);
        }
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
            let content = serialize_openai_content(message.content.as_ref());
            let should_omit_assistant_content = message.role == "assistant"
                && message.tool_calls.len() > 0
                && matches!(content, Value::Null);
            if !should_omit_assistant_content {
                object.insert("content".to_string(), content);
            }
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

fn should_use_openai_responses_api(
    config: &RuntimeProviderConfig,
    _server_tools: &[ServerToolConfig],
) -> bool {
    config.provider_type.uses_openai_responses_api()
}

fn build_openai_responses_request(
    config: &RuntimeProviderConfig,
    messages: &[RequestMessage],
    tools: &[FunctionToolDefinition],
    server_tools: &[ServerToolConfig],
    options: &RequestOptions,
) -> Result<WireRequest> {
    let mut instructions = Vec::new();
    let mut input = Vec::new();

    for message in messages {
        match message.role.as_str() {
            "system" => {
                if let Some(text) = message
                    .content
                    .as_ref()
                    .and_then(MessageContent::joined_texts)
                {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        instructions.push(trimmed.to_string());
                    }
                }
            }
            "user" | "assistant" => {
                let mut item = json!({
                    "type": "message",
                    "role": message.role,
                    "content": serialize_openai_responses_content(message.content.as_ref()),
                });

                if let Some(content) = message.content.as_ref() {
                    if content.parts().is_empty() {
                        item["content"] = Value::Array(Vec::new());
                    }
                }

                input.push(item);
                for tool_call in &message.tool_calls {
                    input.push(json!({
                        "type": "function_call",
                        "call_id": tool_call.id,
                        "name": tool_call.function.name,
                        "arguments": tool_call.function.arguments,
                    }));
                }
            }
            "tool" => {
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": message
                        .tool_call_id
                        .clone()
                        .context("tool message missing tool_call_id")?,
                    "output": message
                        .content
                        .as_ref()
                        .and_then(MessageContent::joined_texts)
                        .unwrap_or_default(),
                }));
            }
            other => bail!("unsupported OpenAI Responses message role {other}"),
        }
    }

    let mut tool_defs = tools
        .iter()
        .map(openai_responses_function_tool)
        .collect::<Vec<_>>();
    tool_defs.extend(
        server_tools
            .iter()
            .filter(|tool| tool.format == ServerToolFormat::OpenAiResponses)
            .map(server_tool_definition),
    );

    let mut body = json!({
        "model": options.model,
        "input": input,
        "temperature": options.temperature,
        "stream": options.stream,
        "store": false,
    });
    insert_optional_json_field(&mut body, "top_p", options.top_p);
    insert_optional_json_field(&mut body, "presence_penalty", options.presence_penalty);
    insert_optional_json_field(&mut body, "frequency_penalty", options.frequency_penalty);
    if !instructions.is_empty() {
        body["instructions"] = Value::String(instructions.join("\n\n"));
    }
    if !tool_defs.is_empty() {
        body["tools"] = Value::Array(tool_defs);
    }
    insert_optional_json_field(&mut body, "max_output_tokens", options.max_output_tokens);

    let mut headers = json_headers();
    apply_bearer_auth(&mut headers, &config.api_key)?;

    Ok(WireRequest {
        url: format!("{}/responses", provider_base_url(config)),
        headers,
        body,
    })
}

fn openai_chat_function_tool(tool: &FunctionToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }
    })
}

fn openai_responses_function_tool(tool: &FunctionToolDefinition) -> Value {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.parameters,
        // March 的工具定义里存在大量真正的可选参数；若强制 strict mode，
        // OpenAI 会要求 properties 中每个字段都出现在 required 里。
        // 当前先显式关闭 strict，保留 best-effort function calling 兼容性。
        "strict": false,
    })
}

fn insert_optional_json_field<T>(body: &mut Value, key: &str, value: Option<T>)
where
    T: serde::Serialize,
{
    if let Some(value) = value {
        body[key] = json!(value);
    }
}

enum SerializedContent {
    Null,
    Text(String),
    Parts(Vec<Value>),
}

fn serialize_message_parts(
    content: Option<&MessageContent>,
    collapse_text_only: bool,
    text_part: impl Fn(&str) -> Value,
    image_part: impl Fn(&str, &str) -> Value,
) -> SerializedContent {
    let Some(content) = content else {
        return SerializedContent::Null;
    };

    if collapse_text_only
        && content
            .parts()
            .iter()
            .all(|part| matches!(part, MessageContentPart::Text(_)))
    {
        return SerializedContent::Text(content.joined_texts().unwrap_or_default());
    }

    SerializedContent::Parts(
        content
            .parts()
            .iter()
            .map(|part| match part {
                MessageContentPart::Text(text) => text_part(text),
                MessageContentPart::Image {
                    media_type,
                    data_base64,
                    ..
                } => image_part(media_type, data_base64),
            })
            .collect(),
    )
}

fn serialize_openai_responses_content(content: Option<&MessageContent>) -> Value {
    match serialize_message_parts(
        content,
        true,
        |text| {
            json!({
                "type": "input_text",
                "text": text,
            })
        },
        |media_type, data_base64| {
            json!({
                "type": "input_image",
                "image_url": format!("data:{};base64,{}", media_type, data_base64),
            })
        },
    ) {
        SerializedContent::Null => Value::Array(Vec::new()),
        SerializedContent::Text(text) => Value::String(text),
        SerializedContent::Parts(parts) => Value::Array(parts),
    }
}

fn parse_openai_responses_response(body: &Value) -> Result<WireResponse> {
    let output = body
        .get("output")
        .and_then(Value::as_array)
        .context("OpenAI Responses response missing output")?;

    let mut text = String::new();
    let mut tool_calls = Vec::new();

    for item in output {
        match item.get("type").and_then(Value::as_str).unwrap_or_default() {
            "message" => {
                if let Some(parts) = item.get("content").and_then(Value::as_array) {
                    for part in parts {
                        if part.get("type").and_then(Value::as_str) == Some("output_text") {
                            if let Some(value) = part.get("text").and_then(Value::as_str) {
                                text.push_str(value);
                            }
                        }
                    }
                }
            }
            "function_call" => {
                let id = item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .context("OpenAI Responses function_call missing call_id")?;
                let name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .context("OpenAI Responses function_call missing name")?;
                let arguments_json = item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}")
                    .to_string();
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

fn parse_openai_responses_stream_event(data: &str) -> Result<Vec<WireStreamDelta>> {
    let event: Value =
        serde_json::from_str(data).context("failed to decode OpenAI Responses stream event")?;
    match event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "response.output_text.delta" => Ok(event
            .get("delta")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
            .map(|text| vec![WireStreamDelta::ContentDelta(text.to_string())])
            .unwrap_or_default()),
        "response.output_item.added" => {
            let item = event
                .get("item")
                .context("OpenAI Responses output_item.added missing item")?;
            if item.get("type").and_then(Value::as_str) != Some("function_call") {
                return Ok(Vec::new());
            }

            let index = event
                .get("output_index")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0);
            Ok(vec![WireStreamDelta::ToolCallDelta {
                index,
                id: item
                    .get("call_id")
                    .or_else(|| item.get("id"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                name: item
                    .get("name")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned),
                arguments_fragment: item
                    .get("arguments")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            }])
        }
        "response.function_call_arguments.delta" => Ok(vec![WireStreamDelta::ToolCallDelta {
            index: event
                .get("output_index")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap_or(0),
            id: None,
            name: None,
            arguments_fragment: event
                .get("delta")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }]),
        "response.completed" => Ok(vec![WireStreamDelta::Done]),
        _ => Ok(Vec::new()),
    }
}

fn serialize_openai_content(content: Option<&MessageContent>) -> Value {
    match serialize_message_parts(
        content,
        true,
        |text| {
            json!({
                "type": "text",
                "text": text,
            })
        },
        |media_type, data_base64| {
            json!({
                "type": "image_url",
                "image_url": {
                    "url": format!("data:{};base64,{}", media_type, data_base64),
                }
            })
        },
    ) {
        SerializedContent::Null => Value::Null,
        SerializedContent::Text(text) => Value::String(text),
        SerializedContent::Parts(parts) => Value::Array(parts),
    }
}

fn serialize_anthropic_blocks(content: Option<&MessageContent>) -> Result<Vec<Value>> {
    Ok(
        match serialize_message_parts(
            content,
            false,
            |text| {
                json!({
                    "type": "text",
                    "text": text,
                })
            },
            |media_type, data_base64| {
                json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": media_type,
                        "data": data_base64,
                    }
                })
            },
        ) {
            SerializedContent::Null => Vec::new(),
            SerializedContent::Text(text) => vec![json!({
                "type": "text",
                "text": text,
            })],
            SerializedContent::Parts(parts) => parts,
        },
    )
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

fn openai_message_text(message: &Value) -> Option<String> {
    openai_message_content(message.get("content")).or_else(|| {
        message
            .get("reasoning_content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
    })
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
            let arguments_json =
                serde_json::to_string(function_call.get("args").unwrap_or(&Value::Null))
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
