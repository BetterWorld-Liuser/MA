use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use eventsource_stream::Eventsource;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;

use super::title::debug_structured_response;
use super::wire::{WireResponse, WireStreamDelta, adapter_for};
use super::{
    ProviderClient, ProviderProgressEvent, ProviderResponse, ProviderToolCall,
    ProviderToolCallDelta, RequestMessage, RequestOptions, RuntimeProviderConfig,
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
    fn ingest_delta(&mut self, delta: WireStreamDelta) {
        match delta {
            WireStreamDelta::ContentDelta(text) => self.content.push_str(&text),
            WireStreamDelta::ToolCallDelta {
                index,
                id,
                name,
                arguments_fragment,
            } => {
                while self.tool_calls.len() <= index {
                    self.tool_calls.push(StreamToolCallAccumulator::default());
                }
                let slot = &mut self.tool_calls[index];
                if let Some(id) = id {
                    slot.id = Some(id);
                }
                if let Some(name) = name {
                    slot.name = name;
                }
                slot.arguments_json.push_str(&arguments_fragment);
            }
            WireStreamDelta::Done => {}
        }
    }

    fn finish(&self) -> Result<Vec<ProviderToolCall>> {
        self.tool_calls
            .iter()
            .map(|tool_call| {
                let id = tool_call
                    .id
                    .clone()
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| format!("tool-call-{}", tool_call.name));
                if tool_call.name.trim().is_empty() {
                    bail!("streamed tool call missing function name");
                }
                let arguments_json = normalized_arguments_json(&tool_call.arguments_json);
                Ok(ProviderToolCall {
                    id,
                    name: tool_call.name.clone(),
                    arguments_json,
                })
            })
            .collect()
    }

    fn tool_call_deltas(&self) -> Vec<ProviderToolCallDelta> {
        self.tool_calls
            .iter()
            .filter(|tool_call| !tool_call.name.trim().is_empty() || tool_call.id.is_some())
            .map(|tool_call| ProviderToolCallDelta {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                arguments_json: normalized_arguments_json(&tool_call.arguments_json),
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
    pub(super) async fn complete_via_stream<F>(
        &self,
        conversation: &[RequestMessage],
        options: &RequestOptions,
        request_json: &str,
        on_event: &mut F,
    ) -> std::result::Result<ProviderResponse, StreamAttemptFailure>
    where
        F: FnMut(ProviderProgressEvent) -> Result<()>,
    {
        let adapter = adapter_for(&self.config);
        let request = adapter
            .build_request(
                &self.config,
                conversation,
                &self.function_tools,
                &self.config.server_tools,
                options,
            )
            .map_err(|error| StreamAttemptFailure {
                kind: StreamFailureKind::Start,
                source: error,
            })?;

        let response = self
            .http
            .post(&request.url)
            .headers(request.headers)
            .json(&request.body)
            .send()
            .await
            .map_err(|error| StreamAttemptFailure {
                kind: StreamFailureKind::Start,
                source: error.into(),
            })?
            .error_for_status()
            .map_err(|error| StreamAttemptFailure {
                kind: StreamFailureKind::Start,
                source: error.into(),
            })?;

        let mut stream = response.bytes_stream().eventsource();
        let mut collector = StreamCollector::default();

        while let Some(event) = stream.next().await {
            let event = event.map_err(|error| StreamAttemptFailure {
                kind: StreamFailureKind::ReadEvent,
                source: error.into(),
            })?;
            let event_name = (!event.event.trim().is_empty()).then_some(event.event.as_str());
            let deltas = adapter
                .parse_stream_event(event_name, &event.data)
                .map_err(|error| StreamAttemptFailure {
                    kind: StreamFailureKind::ReadEvent,
                    source: error,
                })?;

            for delta in deltas {
                collector.ingest_delta(delta.clone());
                match delta {
                    WireStreamDelta::ContentDelta(text) => {
                        on_event(ProviderProgressEvent::ContentDelta(text)).map_err(|error| {
                            StreamAttemptFailure {
                                kind: StreamFailureKind::ReadEvent,
                                source: error,
                            }
                        })?;
                    }
                    WireStreamDelta::ToolCallDelta { .. } => {
                        on_event(ProviderProgressEvent::ToolCallsUpdated(
                            collector.tool_call_deltas(),
                        ))
                        .map_err(|error| StreamAttemptFailure {
                            kind: StreamFailureKind::ReadEvent,
                            source: error,
                        })?;
                    }
                    WireStreamDelta::Done => {}
                }
            }

            if adapter.is_stream_done(event_name, &event.data) {
                let tool_calls = collector.finish().map_err(|error| StreamAttemptFailure {
                    kind: StreamFailureKind::ReadEvent,
                    source: error,
                })?;
                let content = if collector.content.trim().is_empty() {
                    None
                } else {
                    Some(collector.content.clone())
                };
                let response = ProviderResponse {
                    content: content.clone(),
                    tool_calls: tool_calls.clone(),
                    request_json: request_json.to_string(),
                    raw_response: serde_json::to_string_pretty(&debug_structured_response(
                        &ProviderResponse {
                            content,
                            tool_calls,
                            request_json: String::new(),
                            raw_response: String::new(),
                        },
                        DeliveryPath::Streaming,
                        None,
                    ))
                    .unwrap_or_else(|_| "(failed to format provider response)".to_string()),
                };
                return Ok(response);
            }
        }

        Err(StreamAttemptFailure {
            kind: StreamFailureKind::UnexpectedEnd,
            source: anyhow::anyhow!("provider stream ended unexpectedly"),
        })
    }

    pub(super) async fn complete_non_streaming(
        &self,
        conversation: &[RequestMessage],
        options: &RequestOptions,
        request_json: String,
        delivery_path: DeliveryPath,
    ) -> Result<ProviderResponse> {
        let adapter = adapter_for(&self.config);
        let request = adapter.build_request(
            &self.config,
            conversation,
            &self.function_tools,
            &self.config.server_tools,
            options,
        )?;
        let body = self
            .http
            .post(&request.url)
            .headers(request.headers)
            .json(&request.body)
            .send()
            .await
            .context("failed to request provider non-stream response")?
            .error_for_status()
            .context("provider non-stream request failed")?
            .json::<Value>()
            .await
            .context("failed to decode provider non-stream JSON response")?;

        build_provider_response_from_wire_response(
            adapter.parse_response(&body)?,
            request_json,
            delivery_path,
            Some(body),
        )
    }
}

fn build_provider_response_from_wire_response(
    response: WireResponse,
    request_json: String,
    delivery_path: DeliveryPath,
    captured_raw_body: Option<Value>,
) -> Result<ProviderResponse> {
    let provider_response = ProviderResponse {
        content: response.content,
        tool_calls: response
            .tool_calls
            .into_iter()
            .map(|tool_call| ProviderToolCall {
                id: tool_call.id,
                name: tool_call.name,
                arguments_json: normalized_arguments_json(&tool_call.arguments_json),
            })
            .collect(),
        request_json,
        raw_response: String::new(),
    };
    let debug_payload = debug_structured_response(&provider_response, delivery_path, captured_raw_body);

    Ok(ProviderResponse {
        raw_response: serde_json::to_string_pretty(&debug_payload)
            .unwrap_or_else(|_| "(failed to format provider response)".to_string()),
        ..provider_response
    })
}

fn normalized_arguments_json(arguments_json: &str) -> String {
    let trimmed = arguments_json.trim();
    if trimmed.is_empty() {
        "{}".to_string()
    } else if serde_json::from_str::<Value>(trimmed).is_ok() {
        trimmed.to_string()
    } else {
        serde_json::to_string(trimmed).unwrap_or_else(|_| "\"\"".to_string())
    }
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
