use anyhow::{Context, Result, bail};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde_json::{Value, json};

use crate::settings::ServerToolConfig;

use super::{WireResponse, WireToolCall};
use crate::provider::RuntimeProviderConfig;
use crate::provider::messages::{MessageContent, MessageContentPart, RequestMessage};

pub(super) fn insert_optional_json_field<T>(body: &mut Value, key: &str, value: Option<T>)
where
    T: serde::Serialize,
{
    if let Some(value) = value {
        body[key] = json!(value);
    }
}

pub(super) enum SerializedContent {
    Null,
    Text(String),
    Parts(Vec<Value>),
}

pub(super) fn serialize_message_parts(
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

pub(super) fn serialize_openai_content(content: Option<&MessageContent>) -> Value {
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

pub(super) fn serialize_anthropic_blocks(content: Option<&MessageContent>) -> Result<Vec<Value>> {
    Ok(match serialize_message_parts(
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
    })
}

pub(super) fn serialize_gemini_parts(content: Option<&MessageContent>) -> Result<Vec<Value>> {
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

pub(super) fn openai_message_text(message: &Value) -> Option<String> {
    openai_message_content(message.get("content")).or_else(|| {
        message
            .get("reasoning_content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(ToOwned::to_owned)
    })
}

pub(super) fn openai_message_content(value: Option<&Value>) -> Option<String> {
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

pub(super) fn parse_openai_tool_calls(value: Option<&Value>) -> Result<Vec<WireToolCall>> {
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

pub(super) fn parse_gemini_parts(parts: &[Value]) -> Result<WireResponse> {
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

pub(super) fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers
}

pub(super) fn apply_bearer_auth(headers: &mut HeaderMap, api_key: &str) -> Result<()> {
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

pub(super) fn should_use_openai_responses_api(
    config: &RuntimeProviderConfig,
    _server_tools: &[ServerToolConfig],
) -> bool {
    config.provider_type.uses_openai_responses_api()
}

pub(super) fn serialize_openai_message(message: &RequestMessage) -> Result<Value> {
    let mut object = serde_json::Map::new();
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
