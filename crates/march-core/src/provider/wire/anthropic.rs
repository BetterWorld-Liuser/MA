use anyhow::{Context, Result, bail};
use reqwest::header::HeaderValue;
use serde_json::{Value, json};

use crate::provider::RuntimeProviderConfig;
use crate::provider::messages::{
    FunctionToolDefinition, MessageContent, RequestMessage, RequestOptions,
    serialize_tool_arguments, server_tool_definition, validate_messages,
};
use crate::provider::transport::provider_base_url;
use crate::settings::{ServerToolConfig, ServerToolFormat};

use super::{WireRequest, WireResponse, WireStreamDelta, WireToolCall};
use super::shared::{json_headers, serialize_anthropic_blocks};

const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 16_384;

pub(super) fn build_anthropic_request(
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

pub(super) fn parse_anthropic_response(body: &Value) -> Result<WireResponse> {
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

pub(super) fn parse_anthropic_stream_event(
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
