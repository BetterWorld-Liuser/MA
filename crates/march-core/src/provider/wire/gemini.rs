use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::provider::RuntimeProviderConfig;
use crate::provider::messages::{
    FunctionToolDefinition, MessageContent, RequestMessage, RequestOptions,
    serialize_tool_arguments, server_tool_definition, validate_messages,
};
use crate::provider::transport::provider_base_url;
use crate::settings::{ServerToolConfig, ServerToolFormat};

use super::{WireRequest, WireResponse, WireStreamDelta};
use super::shared::{json_headers, parse_gemini_parts, serialize_gemini_parts};

const DEFAULT_MAX_OUTPUT_TOKENS: u32 = 16_384;

pub(super) fn build_gemini_request(
    config: &RuntimeProviderConfig,
    messages: &[RequestMessage],
    tools: &[FunctionToolDefinition],
    server_tools: &[ServerToolConfig],
    options: &RequestOptions,
) -> Result<WireRequest> {
    validate_messages(messages)?;

    let mut system_parts = Vec::new();
    let mut contents = Vec::new();
    let mut tool_names = HashMap::new();

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

pub(super) fn parse_gemini_response(body: &Value) -> Result<WireResponse> {
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

pub(super) fn parse_gemini_stream_event(data: &str) -> Result<Vec<WireStreamDelta>> {
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
