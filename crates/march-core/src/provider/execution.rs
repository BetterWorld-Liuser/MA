use super::{
    DeliveryPath, ProviderClient, ProviderProgressEvent, ProviderResponse, RequestMessage,
    RequestOptions, TurnCancellation, function_tools_for_context, is_rate_limit_error,
    remember_stream_failure, remember_stream_success, stream_preference_for,
};
use crate::agent::is_turn_cancelled_error;
use crate::context::AgentContext;
use anyhow::{Context, Result};
use std::time::Duration;

/// 流式重连：最多重试次数（不含首次）。
const STREAM_MAX_RETRIES: u32 = 3;
/// 流式重连退避基础延迟（每次翻倍，上限 16s）。
const STREAM_INITIAL_BACKOFF: Duration = Duration::from_secs(1);

/// 非流式请求：最多重试次数（不含首次）。
const NON_STREAM_MAX_RETRIES: u32 = 2;
/// 非流式退避基础延迟（每次翻倍，上限 16s）。
const NON_STREAM_INITIAL_BACKOFF: Duration = Duration::from_secs(2);

impl ProviderClient {
    pub async fn complete_context(
        &self,
        context: &AgentContext,
        conversation: Vec<RequestMessage>,
    ) -> Result<ProviderResponse> {
        self.complete_context_with_events_and_cancel(
            context,
            conversation,
            TurnCancellation::never(),
            |_| Ok(()),
        )
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
        self.complete_context_with_events_and_cancel(
            context,
            conversation,
            TurnCancellation::never(),
            move |event| on_event(event),
        )
        .await
    }

    pub async fn complete_context_with_events_and_cancel<F>(
        &self,
        context: &AgentContext,
        conversation: Vec<RequestMessage>,
        cancellation: &TurnCancellation,
        mut on_event: F,
    ) -> Result<ProviderResponse>
    where
        F: FnMut(ProviderProgressEvent) -> Result<()>,
    {
        let mut provider = self.clone();
        provider.function_tools = function_tools_for_context(context);

        let mode = stream_preference_for(&provider.config);
        let stream_options = RequestOptions::for_chat(
            provider.config.model.clone(),
            true,
            provider.config.temperature,
            provider.config.top_p,
            provider.config.presence_penalty,
            provider.config.frequency_penalty,
            provider.config.max_output_tokens,
        );
        let request_preview = super::wire::adapter_for(&provider.config).build_request(
            &provider.config,
            &conversation,
            &provider.function_tools,
            &provider.config.server_tools,
            &stream_options,
        )?;
        let request_json = serde_json::to_string_pretty(&request_preview.body)
            .context("failed to encode provider request")?;

        let non_streaming_options = || {
            RequestOptions::for_chat(
                provider.config.model.clone(),
                false,
                provider.config.temperature,
                provider.config.top_p,
                provider.config.presence_penalty,
                provider.config.frequency_penalty,
                provider.config.max_output_tokens,
            )
        };

        if mode == super::delivery::ProviderDeliveryMode::NonStreaming {
            return retry_non_streaming(
                &provider,
                &conversation,
                &non_streaming_options(),
                request_json,
                DeliveryPath::NonStreamingCached,
                cancellation,
            )
            .await;
        }

        let mut stream_failure_summary: Option<String> = None;

        for attempt in 0..=STREAM_MAX_RETRIES {
            if attempt > 0 {
                let delay = backoff_delay(attempt - 1, STREAM_INITIAL_BACKOFF);
                tokio::select! {
                    _ = cancellation.cancelled() => {
                        return Err(anyhow::anyhow!("turn cancelled"));
                    }
                    _ = tokio::time::sleep(delay) => {}
                }
            }

            let mut attempt_content_delivered = false;
            let result = provider
                .complete_via_stream(
                    &conversation,
                    &stream_options,
                    &request_json,
                    cancellation,
                    &mut |event: ProviderProgressEvent| {
                        if matches!(event, ProviderProgressEvent::ContentDelta(_)) {
                            attempt_content_delivered = true;
                        }
                        on_event(event)
                    },
                )
                .await;

            match result {
                Ok(response) => {
                    remember_stream_success(&provider.config);
                    return Ok(response);
                }
                Err(failure) => {
                    if failure.should_skip_fallback() {
                        let summary = failure.summary();
                        return Err(failure.source.context(summary));
                    }
                    if failure.should_remember_non_streaming() {
                        remember_stream_failure(&provider.config);
                    }

                    let can_retry = failure.is_stream_retryable() && !attempt_content_delivered;
                    stream_failure_summary = Some(failure.summary());

                    if can_retry && attempt < STREAM_MAX_RETRIES {
                        continue;
                    }
                    break;
                }
            }
        }

        let delivery_path = DeliveryPath::NonStreamingFallback {
            stream_failure: stream_failure_summary.unwrap_or_default(),
        };
        retry_non_streaming(
            &provider,
            &conversation,
            &non_streaming_options(),
            request_json,
            delivery_path,
            cancellation,
        )
        .await
    }
}

/// 指数退避延迟，上限 16s。attempt 从 0 开始（0 = 首次重试前的等待）。
fn backoff_delay(attempt: u32, base: Duration) -> Duration {
    let factor = 1u64 << attempt.min(4);
    let millis = base.as_millis() as u64 * factor;
    Duration::from_millis(millis.min(16_000))
}

/// 非流式请求，带指数退避重试。
/// 遇到取消或 429 时立即上浮错误，不再重试。
async fn retry_non_streaming(
    provider: &ProviderClient,
    conversation: &[RequestMessage],
    options: &RequestOptions,
    request_json: String,
    delivery_path: DeliveryPath,
    cancellation: &TurnCancellation,
) -> Result<ProviderResponse> {
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 0..=NON_STREAM_MAX_RETRIES {
        if attempt > 0 {
            let delay = backoff_delay(attempt - 1, NON_STREAM_INITIAL_BACKOFF);
            tokio::select! {
                _ = cancellation.cancelled() => {
                    return Err(anyhow::anyhow!("turn cancelled"));
                }
                _ = tokio::time::sleep(delay) => {}
            }
        }

        match provider
            .complete_non_streaming(
                conversation,
                options,
                request_json.clone(),
                delivery_path.clone(),
                cancellation,
            )
            .await
        {
            Ok(response) => return Ok(response),
            Err(e) => {
                if is_turn_cancelled_error(&e) || is_rate_limit_error(&e) {
                    return Err(e);
                }
                last_error = Some(e);
            }
        }
    }

    let error = last_error.expect("loop ran at least once");
    let attempts = NON_STREAM_MAX_RETRIES + 1;
    let context_msg = match &delivery_path {
        DeliveryPath::NonStreamingFallback { stream_failure } if !stream_failure.is_empty() => {
            format!(
                "provider streaming failed ({stream_failure}) and non-stream request also failed after {attempts} attempts"
            )
        }
        _ => format!("non-stream provider request failed after {attempts} attempts"),
    };
    Err(error.context(context_msg))
}
