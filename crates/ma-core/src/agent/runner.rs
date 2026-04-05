use anyhow::{Result, anyhow, bail};

use crate::context::ContentBlock;
use crate::provider::{OpenAiCompatibleClient, ProviderProgressEvent, RequestMessage};

use super::prompting::{append_assistant_tool_call_message, render_prompt};
use super::tool_calls::{format_tool_error, preview_tool_result, summarize_tool_call};
use super::{
    AgentProgressEvent, AgentRunResult, AgentSession, AgentStatusPhase, AgentToolStatus,
    DebugRound, DebugToolCall, FinalAssistantMessage, TURN_CANCELLED_ERROR_MESSAGE, ToolOutcome,
};

impl AgentSession {
    pub async fn handle_user_message(
        &mut self,
        client: &OpenAiCompatibleClient,
        content: Vec<ContentBlock>,
    ) -> Result<AgentRunResult> {
        self.handle_user_message_with_events_and_cancel(client, content, || false, |_, _| Ok(()))
            .await
    }

    pub async fn handle_user_message_with_events<F>(
        &mut self,
        client: &OpenAiCompatibleClient,
        content: Vec<ContentBlock>,
        on_event: F,
    ) -> Result<AgentRunResult>
    where
        F: FnMut(&AgentSession, AgentProgressEvent) -> Result<()>,
    {
        self.handle_user_message_with_events_and_cancel(client, content, || false, on_event)
            .await
    }

    pub async fn handle_user_message_with_events_and_cancel<F, C>(
        &mut self,
        client: &OpenAiCompatibleClient,
        content: Vec<ContentBlock>,
        is_cancelled: C,
        mut on_event: F,
    ) -> Result<AgentRunResult>
    where
        F: FnMut(&AgentSession, AgentProgressEvent) -> Result<()>,
        C: Fn() -> bool,
    {
        self.add_user_turn(content);

        let mut final_messages = Vec::new();
        let mut summaries = Vec::new();
        let mut debug_rounds = Vec::new();
        let mut transient_messages: Vec<RequestMessage> = Vec::new();
        let mut iteration = 0usize;

        loop {
            ensure_turn_not_cancelled(&is_cancelled)?;
            iteration += 1;
            on_event(
                self,
                AgentProgressEvent::Status {
                    phase: AgentStatusPhase::BuildingContext,
                    label: "正在整理上下文".to_string(),
                },
            )?;
            let context = self.build_context();
            let context_preview = render_prompt(&context);
            let mut conversation = crate::provider::build_messages(&context);
            conversation.extend(transient_messages.clone());
            let mut content_preview = String::new();
            on_event(
                self,
                AgentProgressEvent::Status {
                    phase: AgentStatusPhase::WaitingModel,
                    label: "正在调用模型".to_string(),
                },
            )?;
            let response = client
                .complete_context_with_events(&context, conversation, |event| {
                    ensure_turn_not_cancelled(&is_cancelled)?;
                    if let ProviderProgressEvent::ContentDelta(ref delta) = event {
                        if !delta.is_empty() {
                            content_preview.push_str(delta);
                            on_event(
                                self,
                                AgentProgressEvent::Status {
                                    phase: AgentStatusPhase::Streaming,
                                    label: "正在生成回复".to_string(),
                                },
                            )?;
                            on_event(
                                self,
                                AgentProgressEvent::AssistantTextPreview {
                                    message: content_preview.clone(),
                                },
                            )?;
                        }
                    }
                    Ok(())
                })
                .await?;
            ensure_turn_not_cancelled(&is_cancelled)?;
            let assistant_text = response
                .content
                .as_deref()
                .filter(|text| !text.trim().is_empty())
                .map(ToOwned::to_owned);
            let mut debug_round = DebugRound {
                iteration,
                context_preview,
                provider_request_json: response.request_json.clone(),
                provider_raw_response: response.raw_response.clone(),
                tool_calls: response
                    .tool_calls
                    .iter()
                    .map(|tool_call| DebugToolCall {
                        id: tool_call.id.clone(),
                        name: tool_call.name.clone(),
                        arguments_json: tool_call.arguments_json.clone(),
                    })
                    .collect(),
                tool_results: Vec::new(),
            };

            if response.tool_calls.is_empty() {
                let final_message = match assistant_text {
                    Some(text) if !text.trim().is_empty() => text,
                    _ => bail!("provider returned no tool calls and no text; cannot end turn"),
                };
                let final_message = FinalAssistantMessage {
                    message: final_message,
                };
                self.add_assistant_turn(
                    vec![ContentBlock::text(final_message.message.clone())],
                    summaries.clone(),
                );
                on_event(
                    self,
                    AgentProgressEvent::FinalAssistantMessage(final_message.clone()),
                )?;
                final_messages.push(final_message);
                debug_rounds.push(debug_round);
                on_event(
                    self,
                    AgentProgressEvent::RoundCompleted(
                        debug_rounds
                            .last()
                            .cloned()
                            .expect("debug round just pushed"),
                    ),
                )?;
                return Ok(AgentRunResult {
                    final_messages,
                    debug_rounds,
                });
            }

            append_assistant_tool_call_message(
                &mut transient_messages,
                assistant_text,
                &response.tool_calls,
            );

            for tool_call in response.tool_calls {
                ensure_turn_not_cancelled(&is_cancelled)?;
                let tool_summary =
                    summarize_tool_call(tool_call.name.as_str(), &tool_call.arguments_json);
                on_event(
                    self,
                    AgentProgressEvent::Status {
                        phase: AgentStatusPhase::RunningTool,
                        label: "正在执行工具".to_string(),
                    },
                )?;
                on_event(
                    self,
                    AgentProgressEvent::ToolStarted {
                        tool_call_id: tool_call.id.clone(),
                        tool_name: tool_call.name.clone(),
                        summary: tool_summary.clone(),
                    },
                )?;
                let outcome = match self.execute_tool_call(&tool_call) {
                    Ok(outcome) => {
                        on_event(
                            self,
                            AgentProgressEvent::ToolFinished {
                                tool_call_id: tool_call.id.clone(),
                                status: AgentToolStatus::Success,
                                summary: outcome
                                    .summary
                                    .as_ref()
                                    .map(|summary| summary.summary.clone())
                                    .unwrap_or_else(|| tool_summary.clone()),
                                preview: preview_tool_result(&outcome.result_text),
                            },
                        )?;
                        outcome
                    }
                    Err(error) => {
                        let result_text = format_tool_error(&tool_call.name, &error);
                        on_event(
                            self,
                            AgentProgressEvent::ToolFinished {
                                tool_call_id: tool_call.id.clone(),
                                status: AgentToolStatus::Error,
                                summary: tool_summary.clone(),
                                preview: preview_tool_result(&result_text),
                            },
                        )?;
                        ToolOutcome {
                            result_text,
                            summary: None,
                        }
                    }
                };
                ensure_turn_not_cancelled(&is_cancelled)?;
                transient_messages.push(RequestMessage::tool(
                    tool_call.id,
                    outcome.result_text.clone(),
                ));
                debug_round.tool_results.push(outcome.result_text.clone());
                if let Some(summary) = outcome.summary {
                    summaries.push(summary);
                }
            }

            debug_rounds.push(debug_round);
            on_event(
                self,
                AgentProgressEvent::RoundCompleted(
                    debug_rounds
                        .last()
                        .cloned()
                        .expect("debug round just pushed"),
                ),
            )?;
        }
    }
}

fn ensure_turn_not_cancelled<C>(is_cancelled: &C) -> Result<()>
where
    C: Fn() -> bool,
{
    if is_cancelled() {
        return Err(anyhow!(TURN_CANCELLED_ERROR_MESSAGE));
    }
    Ok(())
}

pub fn is_turn_cancelled_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.to_string().contains(TURN_CANCELLED_ERROR_MESSAGE))
}
