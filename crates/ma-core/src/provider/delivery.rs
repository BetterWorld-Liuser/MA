use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;
use genai::chat::{
    ChatOptions, ChatRequest, ChatResponse, ChatStreamEvent, MessageContent,
    ToolCall as GenAiToolCall,
};
use serde::Serialize;
use serde_json::Value;

use super::title::debug_structured_response;
use super::{
    ProviderClient, ProviderProgressEvent, ProviderResponse, ProviderToolCall,
    ProviderToolCallDelta, RuntimeProviderConfig,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProviderDeliveryMode {
    Streaming,
    NonStreaming,
}

#[derive(Debug, Clone)]
pub(super) enum DeliveryPath {
    Streaming,
    NonStreamingCached,
    NonStreamingFallback { stream_failure: String },
}

#[derive(Debug)]
pub(super) struct StreamAttemptFailure {
    pub kind: StreamFailureKind,
    pub source: anyhow::Error,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum StreamFailureKind {
    Start,
    ReadEvent,
    UnexpectedEnd,
}

impl StreamAttemptFailure {
    pub fn summary(&self) -> String {
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

#[derive(Debug, Serialize)]
pub(super) struct DebugStructuredProviderResponse {
    pub delivery_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_failure: Option<String>,
    pub content: Option<String>,
    pub tool_calls: Vec<DebugStructuredToolCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub captured_raw_body: Option<Value>,
}

#[derive(Debug, Serialize)]
pub(super) struct DebugStructuredToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
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

impl DeliveryPath {
    pub fn label(&self) -> &'static str {
        match self {
            DeliveryPath::Streaming => "streaming",
            DeliveryPath::NonStreamingCached => "non_streaming_cached",
            DeliveryPath::NonStreamingFallback { .. } => "non_streaming_fallback",
        }
    }

    pub fn stream_failure(&self) -> Option<String> {
        match self {
            DeliveryPath::NonStreamingFallback { stream_failure } => Some(stream_failure.clone()),
            _ => None,
        }
    }
}

impl ProviderClient {
    pub(super) fn chat_options() -> ChatOptions {
        ChatOptions::default()
            .with_temperature(0.2)
            .with_capture_content(true)
            .with_capture_tool_calls(true)
    }

    pub(super) async fn complete_via_stream<F>(
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

    pub(super) async fn complete_non_streaming(
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

pub(super) fn stream_preference_for(config: &RuntimeProviderConfig) -> ProviderDeliveryMode {
    stream_capability_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&provider_capability_key(config)).copied())
        .unwrap_or(ProviderDeliveryMode::Streaming)
}

pub(super) fn remember_stream_success(config: &RuntimeProviderConfig) {
    if let Ok(mut cache) = stream_capability_cache().lock() {
        cache.insert(
            provider_capability_key(config),
            ProviderDeliveryMode::Streaming,
        );
    }
}

pub(super) fn remember_stream_failure(config: &RuntimeProviderConfig) {
    if let Ok(mut cache) = stream_capability_cache().lock() {
        cache.insert(
            provider_capability_key(config),
            ProviderDeliveryMode::NonStreaming,
        );
    }
}

pub(super) fn provider_capability_key(config: &RuntimeProviderConfig) -> String {
    format!(
        "{}|{}|{}",
        config.provider_type.as_db_value(),
        config.base_url.as_deref().unwrap_or("(default)"),
        config.model,
    )
}
