use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

use crate::provider::RuntimeProviderConfig;
use crate::provider::messages::{
    FunctionToolDefinition, MessageContent, RequestMessage, RequestOptions, server_tool_definition,
};
use crate::settings::{ServerToolConfig, ServerToolFormat};

use super::shared::{
    SerializedContent, apply_bearer_auth, insert_optional_json_field, json_headers,
    serialize_message_parts,
};
use super::{WireRequest, WireResponse, WireStreamDelta, WireToolCall};
use crate::provider::transport::provider_base_url;

pub(super) fn build_openai_responses_request(
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

pub(super) fn openai_chat_function_tool(tool: &FunctionToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": tool.description,
            "parameters": tool.parameters,
        }
    })
}

pub(super) fn openai_responses_function_tool(tool: &FunctionToolDefinition) -> Value {
    json!({
        "type": "function",
        "name": tool.name,
        "description": tool.description,
        "parameters": tool.parameters,
        "strict": false,
    })
}

pub(super) fn serialize_openai_responses_content(content: Option<&MessageContent>) -> Value {
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

pub(super) fn parse_openai_responses_response(body: &Value) -> Result<WireResponse> {
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

pub(super) fn parse_openai_responses_stream_event(data: &str) -> Result<Vec<WireStreamDelta>> {
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
