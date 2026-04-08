mod anthropic;
mod gemini;
mod openai;
mod shared;

use anyhow::{Context, Result};
use reqwest::header::HeaderMap;
use serde_json::{Value, json};

use crate::settings::{ServerToolConfig, ServerToolFormat};

use super::RuntimeProviderConfig;
use super::messages::{
    FunctionToolDefinition, RequestMessage, RequestOptions, server_tool_definition,
    validate_messages,
};
use super::transport::provider_base_url;
use shared::{
    apply_bearer_auth, insert_optional_json_field, json_headers, openai_message_text,
    parse_openai_tool_calls, serialize_openai_message, should_use_openai_responses_api,
};
use openai::{
    build_openai_responses_request, openai_chat_function_tool, parse_openai_responses_response,
    parse_openai_responses_stream_event,
};
use anthropic::{
    build_anthropic_request, parse_anthropic_response, parse_anthropic_stream_event,
};
use gemini::{build_gemini_request, parse_gemini_response, parse_gemini_stream_event};

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
        build_anthropic_request(config, messages, tools, server_tools, options)
    }

    fn parse_response(&self, body: &Value) -> Result<WireResponse> {
        parse_anthropic_response(body)
    }

    fn parse_stream_event(
        &self,
        event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<WireStreamDelta>> {
        parse_anthropic_stream_event(event_name, data)
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
        build_gemini_request(config, messages, tools, server_tools, options)
    }

    fn parse_response(&self, body: &Value) -> Result<WireResponse> {
        parse_gemini_response(body)
    }

    fn parse_stream_event(
        &self,
        _event_name: Option<&str>,
        data: &str,
    ) -> Result<Vec<WireStreamDelta>> {
        parse_gemini_stream_event(data)
    }

    fn is_stream_done(&self, _event_name: Option<&str>, _data: &str) -> bool {
        false
    }
}
