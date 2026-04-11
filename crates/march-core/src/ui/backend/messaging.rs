use super::*;
use crate::agent::AgentToolStatus;
use crate::context::Hint;
use crate::storage::{
    MarchStorage, PersistedAssistantMessage, PersistedAssistantMessageState,
    PersistedAssistantTimelineEntry, PersistedReplyRef, PersistedTaskTimeline,
    PersistedTaskTimelineEntry, PersistedToolCallState, PersistedTurn, PersistedTurnState,
    PersistedTurnTrigger, PersistedUserMessage, turn_agent_id,
};
use crate::ui::UiReplyRef;
use futures_util::{
    FutureExt,
    future::BoxFuture,
    stream::{FuturesUnordered, StreamExt},
};
use indexmap::IndexMap;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

#[derive(Debug, Clone)]
struct TurnProgressContext {
    user_message_id: String,
    next_seq: u64,
}

impl TurnProgressContext {
    fn new(user_message_id: String) -> Self {
        Self {
            user_message_id,
            next_seq: 0,
        }
    }

    fn next_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }
}

#[derive(Debug, Clone)]
struct PendingTurn {
    agent_name: String,
    trigger: UiTurnTrigger,
    persisted_state: PersistedTaskState,
}

#[derive(Debug, Clone)]
struct RunningTurnState {
    pending_turn: PendingTurn,
}

#[derive(Debug, Clone)]
enum TurnWorkerUpdate {
    Progress(UiAgentProgressEvent),
    RoundComplete {
        turn_id: String,
        debug_round: DebugRound,
        current_state: PersistedTaskState,
        memory_index: Option<crate::memory::MemoryIndexView>,
    },
}

#[derive(Debug)]
enum TurnExecutionOutcome {
    Completed {
        turn_id: String,
        pending_turn: PendingTurn,
        completed_state: PersistedTaskState,
        memory_index: Option<crate::memory::MemoryIndexView>,
        result: AgentRunResult,
        next_agents: Vec<String>,
    },
    Failed {
        turn_id: String,
        pending_turn: PendingTurn,
        completed_state: PersistedTaskState,
        memory_index: Option<crate::memory::MemoryIndexView>,
        cancelled: bool,
        error_message: Option<String>,
    },
}

type TurnFuture = BoxFuture<'static, Result<TurnExecutionOutcome>>;

impl UiAppBackend {
    pub async fn handle_send_message(
        &mut self,
        request: UiSendMessageRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        self.handle_send_message_with_progress_and_cancel(
            request,
            |_| Ok(()),
            |_, _| Ok(()),
            TurnCancellation::never(),
        )
        .await
    }

    pub async fn handle_send_message_with_progress<F>(
        &mut self,
        request: UiSendMessageRequest,
        on_progress: F,
    ) -> Result<UiWorkspaceSnapshot>
    where
        F: FnMut(UiAgentProgressEvent) -> Result<()>,
    {
        self.handle_send_message_with_progress_and_cancel(
            request,
            on_progress,
            |_, _| Ok(()),
            TurnCancellation::never(),
        )
        .await
    }

    pub async fn handle_send_message_with_progress_and_cancel<F, G>(
        &mut self,
        request: UiSendMessageRequest,
        mut on_progress: F,
        mut on_turn_started: G,
        cancellation: &TurnCancellation,
    ) -> Result<UiWorkspaceSnapshot>
    where
        F: FnMut(UiAgentProgressEvent) -> Result<()>,
        G: FnMut(String, std::sync::Arc<TurnCancellation>) -> Result<()>,
    {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let requested_mentions = request.mentions;
        let requested_replies = request.replies;
        let content_blocks = request
            .content_blocks
            .into_iter()
            .map(content_block_from_ui)
            .collect::<Vec<_>>();
        let content_text = join_text_blocks(&content_blocks);
        self.validate_message_content(&content_blocks)?;

        let PreparedMessageContext {
            persisted_task: persisted_before,
            mut session,
            should_auto_title,
        } = self.prepare_message_context(task_id, &content_text).await?;
        let provider_config = provider_config_for_session(&persisted_before.task, &session)?;
        let provider = OpenAiCompatibleClient::new(provider_config.clone());
        let context_budget_tokens =
            resolve_context_window_with_provider(&provider, &provider_config.model)
                .await
                .unwrap_or_else(|| {
                    resolve_context_window_fallback(Some(provider_config.model.as_str()))
                });
        let user_message_timestamp = SystemTime::now();
        let user_message_id = format!(
            "user-{}-{}",
            task_id,
            system_time_to_unix(user_message_timestamp)
        );
        let mut progress_context = TurnProgressContext::new(user_message_id.clone());
        let initial_mentions = normalize_requested_mentions(
            if requested_mentions.is_empty() {
                extract_agent_mentions(&content_text, &session)
            } else {
                requested_mentions
            },
            &session,
        );
        let progress_task = self
            .storage
            .list_tasks()?
            .into_iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("task {} not found", task_id))?;
        let mut progress_rounds = Vec::new();
        on_progress(UiAgentProgressEvent::UserMessageAppended {
            task_id,
            seq: progress_context.next_seq(),
            user_message_id,
            content: content_text.clone(),
            ts: system_time_to_unix(user_message_timestamp),
            mentions: initial_mentions.clone(),
            replies: requested_replies.clone(),
        })?;
        session.add_user_turn(content_blocks.clone());
        let mut canonical_state = PersistedTaskState {
            timeline: Some(append_persisted_user_message(
                &persisted_before.timeline,
                PersistedUserMessage {
                    user_message_id: progress_context.user_message_id.clone(),
                    content: content_blocks.clone(),
                    mentions: initial_mentions.clone(),
                    replies: requested_replies
                        .clone()
                        .into_iter()
                        .map(reply_ref_from_ui)
                        .collect(),
                    timestamp: user_message_timestamp,
                },
            )),
            ..session.persisted_state()
        };
        let initial_turn_baseline = canonical_state.clone();
        let initial_agents = resolve_initial_agents(
            if initial_mentions.is_empty() {
                vec![session.active_agent_name().to_string()]
            } else {
                initial_mentions
            },
            &requested_replies,
            &persisted_before.timeline,
        );
        let initial_agents = if initial_agents.is_empty() {
            vec![session.active_agent_name().to_string()]
        } else {
            initial_agents
        };

        let mut combined_result = AgentRunResult {
            final_messages: Vec::new(),
            debug_rounds: Vec::new(),
        };
        let mut last_memory_index: Option<crate::memory::MemoryIndexView> = None;
        let (turn_updates_tx, mut turn_updates_rx) = unbounded_channel::<TurnWorkerUpdate>();
        let mut running_turns: FuturesUnordered<TurnFuture> = FuturesUnordered::new();
        let mut running_turn_states = IndexMap::<String, RunningTurnState>::new();

        for agent_name in initial_agents {
            Self::launch_pending_turn(
                task_id,
                &persisted_before.task,
                &mut self.storage,
                PendingTurn {
                    agent_name,
                    trigger: UiTurnTrigger::User {
                        id: progress_context.user_message_id.clone(),
                    },
                    persisted_state: initial_turn_baseline.clone(),
                },
                context_budget_tokens,
                cancellation,
                &mut on_progress,
                &mut on_turn_started,
                &mut progress_context,
                &turn_updates_tx,
                &mut running_turns,
                &mut running_turn_states,
            )?;
        }

        while !running_turns.is_empty() || !turn_updates_rx.is_closed() {
            if running_turns.is_empty() {
                break;
            }

            tokio::select! {
                biased;
                Some(update) = turn_updates_rx.recv() => {
                    match update {
                        TurnWorkerUpdate::Progress(event) => {
                            on_progress(event)?;
                        }
                        TurnWorkerUpdate::RoundComplete { turn_id, debug_round, current_state, memory_index } => {
                            last_memory_index = memory_index.clone().or(last_memory_index);
                            progress_rounds.push(debug_round.clone());
                            combined_result.debug_rounds.push(debug_round.clone());
                            let Some(running_turn) = running_turn_states.get(&turn_id) else {
                                continue;
                            };
                            let preview_state = merge_turn_state(
                                &running_turn.pending_turn.persisted_state,
                                &canonical_state,
                                &current_state,
                            );
                            let task = build_live_task_snapshot_from_state(
                                progress_task.clone(),
                                &persisted_before.task,
                                &preview_state,
                                memory_index,
                                &progress_rounds,
                                context_budget_tokens,
                            )?;
                            on_progress(UiAgentProgressEvent::RoundComplete {
                                task_id,
                                seq: progress_context.next_seq(),
                                turn_id,
                                debug_round: debug_round.into(),
                                task,
                            })?;
                        }
                    }
                }
                Some(outcome) = running_turns.next() => {
                    match outcome {
                        Ok(TurnExecutionOutcome::Completed {
                            turn_id,
                            pending_turn,
                            completed_state,
                            memory_index,
                            result,
                            next_agents,
                        }) => {
                            last_memory_index = memory_index.clone().or(last_memory_index);
                            running_turn_states.shift_remove(&turn_id);
                            canonical_state = merge_turn_state(
                                &pending_turn.persisted_state,
                                &canonical_state,
                                &completed_state,
                            );
                            let mut canonical_session =
                                restore_session_from_state(&persisted_before.task, &canonical_state)?;
                            canonical_session.restore_last_memory_index(memory_index);
                            save_session_with_timeline(
                                &mut self.storage,
                                task_id,
                                &mut canonical_session,
                                canonical_state.timeline.clone().unwrap_or_default(),
                            )?;
                            let task = Self::live_task_snapshot(
                                progress_task.clone(),
                                &canonical_session,
                                &progress_rounds,
                                context_budget_tokens,
                                canonical_state.timeline.clone().unwrap_or_default(),
                            )?;
                            on_progress(UiAgentProgressEvent::TurnFinished {
                                task_id,
                                seq: progress_context.next_seq(),
                                turn_id: turn_id.clone(),
                                reason: UiTurnFinishedReason::Idle,
                                error_message: None,
                                task,
                            })?;

                            combined_result
                                .final_messages
                                .extend(result.final_messages.clone().into_iter());

                            for agent_name in next_agents {
                                Self::launch_pending_turn(
                                    task_id,
                                    &persisted_before.task,
                                    &mut self.storage,
                                    PendingTurn {
                                        agent_name,
                                        trigger: UiTurnTrigger::Turn { id: turn_id.clone() },
                                        persisted_state: completed_state.clone(),
                                    },
                                    context_budget_tokens,
                                    cancellation,
                                    &mut on_progress,
                                    &mut on_turn_started,
                                    &mut progress_context,
                                    &turn_updates_tx,
                                    &mut running_turns,
                                    &mut running_turn_states,
                                )?;
                            }
                        }
                        Ok(TurnExecutionOutcome::Failed {
                            turn_id,
                            pending_turn,
                            completed_state,
                            memory_index,
                            cancelled,
                            error_message,
                        }) => {
                            last_memory_index = memory_index.clone().or(last_memory_index);
                            running_turn_states.shift_remove(&turn_id);
                            canonical_state = merge_turn_state(
                                &pending_turn.persisted_state,
                                &canonical_state,
                                &completed_state,
                            );
                            let mut canonical_session =
                                restore_session_from_state(&persisted_before.task, &canonical_state)?;
                            canonical_session.restore_last_memory_index(memory_index);
                            save_session_with_timeline(
                                &mut self.storage,
                                task_id,
                                &mut canonical_session,
                                canonical_state.timeline.clone().unwrap_or_default(),
                            )?;
                            let task = Self::live_task_snapshot(
                                progress_task.clone(),
                                &canonical_session,
                                &progress_rounds,
                                context_budget_tokens,
                                canonical_state.timeline.clone().unwrap_or_default(),
                            )?;
                            on_progress(UiAgentProgressEvent::TurnFinished {
                                task_id,
                                seq: progress_context.next_seq(),
                                turn_id,
                                reason: if cancelled {
                                    UiTurnFinishedReason::Cancelled
                                } else {
                                    UiTurnFinishedReason::Failed
                                },
                                error_message,
                                task,
                            })?;
                        }
                        Err(error) => return Err(error),
                    }
                }
                else => break,
            }
        }
        drop(turn_updates_tx);
        while let Some(update) = turn_updates_rx.recv().await {
            match update {
                TurnWorkerUpdate::Progress(event) => {
                    on_progress(event)?;
                }
                TurnWorkerUpdate::RoundComplete {
                    turn_id,
                    debug_round,
                    current_state,
                    memory_index,
                } => {
                    last_memory_index = memory_index.clone().or(last_memory_index);
                    progress_rounds.push(debug_round.clone());
                    combined_result.debug_rounds.push(debug_round.clone());
                    let Some(running_turn) = running_turn_states.get(&turn_id) else {
                        continue;
                    };
                    let preview_state = merge_turn_state(
                        &running_turn.pending_turn.persisted_state,
                        &canonical_state,
                        &current_state,
                    );
                    let task = build_live_task_snapshot_from_state(
                        progress_task.clone(),
                        &persisted_before.task,
                        &preview_state,
                        memory_index,
                        &progress_rounds,
                        context_budget_tokens,
                    )?;
                    on_progress(UiAgentProgressEvent::RoundComplete {
                        task_id,
                        seq: progress_context.next_seq(),
                        turn_id,
                        debug_round: debug_round.into(),
                        task,
                    })?;
                }
            }
        }

        let mut final_session =
            restore_session_from_state(&persisted_before.task, &canonical_state)?;
        final_session.restore_last_memory_index(last_memory_index);
        let runtime = final_session.ui_runtime_snapshot(context_budget_tokens);
        save_session_with_timeline(
            &mut self.storage,
            task_id,
            &mut final_session,
            canonical_state.timeline.clone().unwrap_or_default(),
        )?;
        if should_auto_title {
            let suggested_title = Self::suggest_auto_title(&provider, &content_text).await;
            self.apply_suggested_task_title(task_id, suggested_title)?;
        }
        let mut workspace = self.workspace_snapshot(Some(task_id))?;
        if let Some(active_task) = workspace.active_task.take() {
            workspace.active_task =
                Some(active_task.with_runtime(&runtime).with_debug_trace(
                    UiDebugTraceView::from_rounds(&combined_result.debug_rounds),
                ));
        }
        Ok(workspace)
    }

    fn validate_message_content(&self, content_blocks: &[ContentBlock]) -> Result<()> {
        if content_blocks.is_empty()
            || content_blocks.iter().all(|block| match block {
                ContentBlock::Text { text } => text.trim().is_empty(),
                ContentBlock::Image { .. } => false,
            })
        {
            bail!("message cannot be empty");
        }
        Ok(())
    }

    async fn prepare_message_context(
        &mut self,
        task_id: i64,
        content_text: &str,
    ) -> Result<PreparedMessageContext> {
        let persisted_task = self.storage.load_task(task_id)?;
        let should_auto_title = should_auto_title(&persisted_task, content_text);
        let session = self.load_session(task_id)?;
        Ok(PreparedMessageContext {
            persisted_task,
            session,
            should_auto_title,
        })
    }

    async fn suggest_auto_title(
        provider: &OpenAiCompatibleClient,
        content_text: &str,
    ) -> Option<String> {
        provider
            .suggest_task_title(content_text)
            .await
            .ok()
            .flatten()
            .or_else(|| fallback_task_title(content_text))
    }

    fn launch_pending_turn<F, G>(
        task_id: i64,
        task: &TaskRecord,
        storage: &mut MarchStorage,
        pending_turn: PendingTurn,
        context_budget_tokens: usize,
        cancellation: &TurnCancellation,
        on_progress: &mut F,
        on_turn_started: &mut G,
        progress_context: &mut TurnProgressContext,
        turn_updates_tx: &UnboundedSender<TurnWorkerUpdate>,
        running_turns: &mut FuturesUnordered<TurnFuture>,
        running_turn_states: &mut IndexMap<String, RunningTurnState>,
    ) -> Result<()>
    where
        F: FnMut(UiAgentProgressEvent) -> Result<()>,
        G: FnMut(String, std::sync::Arc<TurnCancellation>) -> Result<()>,
    {
        let mut session = restore_session_from_state(task, &pending_turn.persisted_state)?;
        session.set_active_agent(pending_turn.agent_name.clone());
        storage.update_task_active_agent(task_id, session.active_agent_name())?;
        let turn_id = format!(
            "turn-{}-{}",
            task_id,
            system_time_to_unix(SystemTime::now())
        );
        on_progress(UiAgentProgressEvent::TurnStarted {
            task_id,
            seq: progress_context.next_seq(),
            turn_id: turn_id.clone(),
            agent: session.active_agent_name().to_string(),
            agent_display_name: session.display_name_for_agent(session.active_agent_name()),
            trigger: pending_turn.trigger.clone(),
        })?;
        let turn_cancellation = std::sync::Arc::new(TurnCancellation::child_of(cancellation));
        on_turn_started(turn_id.clone(), turn_cancellation.clone())?;
        running_turn_states.insert(
            turn_id.clone(),
            RunningTurnState {
                pending_turn: pending_turn.clone(),
            },
        );
        running_turns.push(execute_pending_turn(
            task_id,
            task.clone(),
            pending_turn,
            turn_id,
            context_budget_tokens,
            turn_cancellation,
            turn_updates_tx.clone(),
        ));
        Ok(())
    }
}

fn restore_session_from_state(
    task: &TaskRecord,
    state: &PersistedTaskState,
) -> Result<AgentSession> {
    AgentSession::restore(
        ui_agent_config(),
        PersistedTask {
            task: task.clone(),
            active_agent: state.active_agent.clone(),
            timeline: state.timeline.clone().unwrap_or_default(),
            notes: state.notes.clone(),
            open_files: state.open_files.clone(),
            hints: state.hints.clone(),
        },
    )
}

fn execute_pending_turn(
    task_id: i64,
    task: TaskRecord,
    pending_turn: PendingTurn,
    turn_id: String,
    context_budget_tokens: usize,
    cancellation: std::sync::Arc<TurnCancellation>,
    turn_updates_tx: UnboundedSender<TurnWorkerUpdate>,
) -> TurnFuture {
    async move {
        let mut session = restore_session_from_state(&task, &pending_turn.persisted_state)?;
        session.set_active_agent(pending_turn.agent_name.clone());
        let mut turn_timeline = append_started_turn(
            pending_turn
                .persisted_state
                .timeline
                .as_deref()
                .unwrap_or(&[]),
            PersistedTurn {
                turn_id: turn_id.clone(),
                agent_id: session.active_agent_name().to_string(),
                trigger: turn_trigger_from_ui(pending_turn.trigger.clone()),
                state: PersistedTurnState::Streaming,
                error_message: None,
                timestamp: SystemTime::now(),
                messages: Vec::new(),
            },
        );

        let provider_config = provider_config_for_session(&task, &session)?;
        let provider = OpenAiCompatibleClient::new(provider_config);
        let result = session
            .continue_with_events_and_cancel(&provider, cancellation.as_ref(), |session, event| {
                turn_timeline = apply_agent_progress_to_persisted_timeline(
                    &turn_timeline,
                    &turn_id,
                    event.clone(),
                );
                emit_turn_worker_update(
                    task_id,
                    &turn_id,
                    context_budget_tokens,
                    session,
                    &turn_timeline,
                    event,
                    &turn_updates_tx,
                )
            })
            .await;

        match result {
            Ok(result) => {
                turn_timeline =
                    finish_persisted_turn(&turn_timeline, &turn_id, PersistedTurnState::Done, None);
                let completed_state = PersistedTaskState {
                    timeline: Some(turn_timeline),
                    ..session.persisted_state()
                };
                let memory_index = session.last_memory_index();
                let next_agents = result
                    .final_messages
                    .last()
                    .map(|final_message| {
                        extract_agent_mentions(&final_message.message, &session)
                            .into_iter()
                            .filter(|agent_name| agent_name != session.active_agent_name())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Ok(TurnExecutionOutcome::Completed {
                    turn_id,
                    pending_turn,
                    completed_state,
                    memory_index,
                    result,
                    next_agents,
                })
            }
            Err(error) => {
                let cancelled = is_turn_cancelled_error(&error);
                turn_timeline = finish_persisted_turn(
                    &turn_timeline,
                    &turn_id,
                    if cancelled {
                        PersistedTurnState::Cancelled
                    } else {
                        PersistedTurnState::Failed
                    },
                    if cancelled {
                        None
                    } else {
                        Some(error.to_string())
                    },
                );
                let completed_state = PersistedTaskState {
                    timeline: Some(turn_timeline),
                    ..session.persisted_state()
                };
                let memory_index = session.last_memory_index();
                Ok(TurnExecutionOutcome::Failed {
                    turn_id,
                    pending_turn,
                    completed_state,
                    memory_index,
                    cancelled,
                    error_message: if cancelled {
                        None
                    } else {
                        Some(error.to_string())
                    },
                })
            }
        }
    }
    .boxed()
}

fn emit_turn_worker_update(
    task_id: i64,
    turn_id: &str,
    context_budget_tokens: usize,
    session: &AgentSession,
    current_timeline: &[PersistedTaskTimelineEntry],
    event: AgentProgressEvent,
    turn_updates_tx: &UnboundedSender<TurnWorkerUpdate>,
) -> Result<()> {
    let runtime = session.ui_runtime_snapshot(context_budget_tokens);
    let update = match event {
        AgentProgressEvent::Status { .. } | AgentProgressEvent::FinalAssistantMessage(_) => {
            return Ok(());
        }
        AgentProgressEvent::ToolStarted {
            message_id,
            tool_call_id,
            tool_name,
            summary,
        } => TurnWorkerUpdate::Progress(UiAgentProgressEvent::ToolStarted {
            task_id,
            seq: 0,
            turn_id: turn_id.to_string(),
            message_id,
            tool_call_id,
            tool_name,
            summary,
            runtime,
        }),
        AgentProgressEvent::ToolFinished {
            message_id,
            tool_call_id,
            status,
            summary,
            preview,
            detail,
        } => TurnWorkerUpdate::Progress(UiAgentProgressEvent::ToolFinished {
            task_id,
            seq: 0,
            turn_id: turn_id.to_string(),
            message_id,
            tool_call_id,
            status: status.into(),
            summary,
            preview,
            detail,
            runtime,
        }),
        AgentProgressEvent::AssistantTextPreview { message_id, delta } => {
            TurnWorkerUpdate::Progress(UiAgentProgressEvent::AssistantStreamDelta {
                task_id,
                seq: 0,
                turn_id: turn_id.to_string(),
                message_id,
                field: UiAssistantStreamField::Content,
                delta,
                tool_call_id: None,
                runtime,
            })
        }
        AgentProgressEvent::MessageStarted { message_id } => {
            TurnWorkerUpdate::Progress(UiAgentProgressEvent::MessageStarted {
                task_id,
                seq: 0,
                turn_id: turn_id.to_string(),
                message_id,
                runtime,
            })
        }
        AgentProgressEvent::MessageFinished { message_id } => {
            TurnWorkerUpdate::Progress(UiAgentProgressEvent::MessageFinished {
                task_id,
                seq: 0,
                turn_id: turn_id.to_string(),
                message_id,
                runtime,
            })
        }
        AgentProgressEvent::RoundCompleted(debug_round) => TurnWorkerUpdate::RoundComplete {
            turn_id: turn_id.to_string(),
            debug_round,
            memory_index: session.last_memory_index(),
            current_state: PersistedTaskState {
                timeline: Some(current_timeline.to_vec()),
                ..session.persisted_state()
            },
        },
    };

    turn_updates_tx
        .send(update)
        .map_err(|_| anyhow::anyhow!("turn update channel closed"))?;
    Ok(())
}

fn build_live_task_snapshot_from_state(
    task: TaskRecord,
    session_task: &TaskRecord,
    state: &PersistedTaskState,
    memory_index: Option<crate::memory::MemoryIndexView>,
    debug_rounds: &[DebugRound],
    context_budget_tokens: usize,
) -> Result<UiTaskSnapshot> {
    let mut session = restore_session_from_state(session_task, state)?;
    session.restore_last_memory_index(memory_index);
    UiAppBackend::live_task_snapshot(
        task,
        &session,
        debug_rounds,
        context_budget_tokens,
        state.timeline.clone().unwrap_or_default(),
    )
}

fn merge_turn_state(
    baseline: &PersistedTaskState,
    canonical: &PersistedTaskState,
    completed: &PersistedTaskState,
) -> PersistedTaskState {
    let mut merged = canonical.clone();
    merged.active_agent = completed.active_agent.clone();
    merged.notes = merge_keyed_entries(
        &baseline.notes,
        &canonical.notes,
        &completed.notes,
        |note| (note.scope.clone(), note.id.clone()),
    );
    merged.open_files = merge_keyed_entries(
        &baseline.open_files,
        &canonical.open_files,
        &completed.open_files,
        |file| (file.scope.clone(), file.path.clone()),
    );
    merged.hints = merge_hint_progress(&canonical.hints);
    merged.timeline = Some(merge_timeline_suffix(
        baseline.timeline.as_deref().unwrap_or(&[]),
        canonical.timeline.as_deref().unwrap_or(&[]),
        completed.timeline.as_deref().unwrap_or(&[]),
    ));
    merged.last_active = completed.last_active;
    merged
}

fn merge_keyed_entries<T, K, F>(
    baseline: &[T],
    canonical: &[T],
    completed: &[T],
    key_of: F,
) -> Vec<T>
where
    T: Clone + PartialEq,
    K: std::hash::Hash + Eq + Clone,
    F: Fn(&T) -> K,
{
    let baseline_map = baseline
        .iter()
        .map(|entry| (key_of(entry), entry))
        .collect::<IndexMap<_, _>>();
    let completed_map = completed
        .iter()
        .map(|entry| (key_of(entry), entry))
        .collect::<IndexMap<_, _>>();
    let mut merged = canonical
        .iter()
        .cloned()
        .map(|entry| (key_of(&entry), entry))
        .collect::<IndexMap<_, _>>();

    for (key, entry) in &completed_map {
        let changed = match baseline_map.get(key) {
            Some(baseline_entry) => *baseline_entry != *entry,
            None => true,
        };
        if changed {
            merged.insert(key.clone(), (*entry).clone());
        }
    }

    for key in baseline_map.keys() {
        if !completed_map.contains_key(key) {
            merged.shift_remove(key);
        }
    }

    merged.into_values().collect()
}

fn merge_hint_progress(hints: &[Hint]) -> Vec<Hint> {
    let now = SystemTime::now();
    let mut merged = hints.to_vec();
    for hint in &mut merged {
        hint.tick_turn();
    }
    merged.retain(|hint| !hint.is_expired_at(now));
    merged
}

fn merge_timeline_suffix(
    baseline: &[PersistedTaskTimelineEntry],
    canonical: &[PersistedTaskTimelineEntry],
    completed: &[PersistedTaskTimelineEntry],
) -> PersistedTaskTimeline {
    let mut timeline = canonical.to_vec();
    timeline.extend(completed.iter().skip(baseline.len()).cloned());
    timeline
}

fn normalize_requested_mentions(mentions: Vec<String>, session: &AgentSession) -> Vec<String> {
    let mut normalized = Vec::new();

    for mention in mentions {
        let candidate = mention.trim().to_ascii_lowercase();
        if candidate.is_empty() || !session.has_agent(&candidate) {
            continue;
        }
        if !normalized.iter().any(|entry| entry == &candidate) {
            normalized.push(candidate);
        }
    }

    normalized
}

fn resolve_initial_agents(
    mentions: Vec<String>,
    replies: &[UiReplyRef],
    timeline: &[PersistedTaskTimelineEntry],
) -> Vec<String> {
    let mut agents = Vec::new();

    for mention in mentions {
        if !agents.iter().any(|entry| entry == &mention) {
            agents.push(mention);
        }
    }

    for reply in replies {
        let UiReplyRef::Turn { id } = reply else {
            continue;
        };
        let Some(agent_id) = turn_agent_id(timeline, id) else {
            continue;
        };
        if !agents.iter().any(|entry| entry == agent_id) {
            agents.push(agent_id.to_string());
        }
    }

    agents
}

fn append_persisted_user_message(
    timeline: &[PersistedTaskTimelineEntry],
    message: PersistedUserMessage,
) -> PersistedTaskTimeline {
    let mut next = timeline.to_vec();
    next.push(PersistedTaskTimelineEntry::UserMessage(message));
    next
}

fn append_started_turn(
    timeline: &[PersistedTaskTimelineEntry],
    turn: PersistedTurn,
) -> PersistedTaskTimeline {
    let mut next = timeline.to_vec();
    next.push(PersistedTaskTimelineEntry::Turn(turn));
    next
}

fn apply_agent_progress_to_persisted_timeline(
    timeline: &[PersistedTaskTimelineEntry],
    turn_id: &str,
    event: AgentProgressEvent,
) -> PersistedTaskTimeline {
    let mut next = timeline.to_vec();
    let Some(PersistedTaskTimelineEntry::Turn(turn)) = next.iter_mut().find(
        |entry| matches!(entry, PersistedTaskTimelineEntry::Turn(turn) if turn.turn_id == turn_id),
    ) else {
        return next;
    };

    match event {
        AgentProgressEvent::MessageStarted { message_id } => {
            if !turn
                .messages
                .iter()
                .any(|message| message.message_id == message_id)
            {
                turn.messages.push(PersistedAssistantMessage {
                    message_id,
                    turn_id: turn_id.to_string(),
                    state: PersistedAssistantMessageState::Streaming,
                    reasoning: String::new(),
                    timeline: Vec::new(),
                });
            }
        }
        AgentProgressEvent::ToolStarted {
            message_id,
            tool_call_id,
            tool_name,
            summary,
        } => {
            let message = ensure_persisted_message(turn, &message_id);
            message
                .timeline
                .push(PersistedAssistantTimelineEntry::Tool {
                    tool_call_id,
                    tool_name,
                    arguments: String::new(),
                    status: PersistedToolCallState::Running,
                    preview: Some(summary),
                    duration_ms: None,
                });
        }
        AgentProgressEvent::ToolFinished {
            message_id,
            tool_call_id,
            status,
            summary,
            preview,
            ..
        } => {
            let message = ensure_persisted_message(turn, &message_id);
            if let Some(PersistedAssistantTimelineEntry::Tool {
                status: tool_status,
                preview: tool_preview,
                ..
            }) = message.timeline.iter_mut().find(|entry| {
                matches!(
                    entry,
                    PersistedAssistantTimelineEntry::Tool { tool_call_id: existing_id, .. }
                    if existing_id == &tool_call_id
                )
            }) {
                *tool_status = match status {
                    AgentToolStatus::Success => PersistedToolCallState::Ok,
                    AgentToolStatus::Error => PersistedToolCallState::Error,
                };
                *tool_preview = preview.or(Some(summary));
            }
        }
        AgentProgressEvent::AssistantTextPreview { message_id, delta } => {
            let message = ensure_persisted_message(turn, &message_id);
            match message.timeline.last_mut() {
                Some(PersistedAssistantTimelineEntry::Text { text }) => text.push_str(&delta),
                _ => message
                    .timeline
                    .push(PersistedAssistantTimelineEntry::Text { text: delta }),
            }
        }
        AgentProgressEvent::MessageFinished { message_id } => {
            let message = ensure_persisted_message(turn, &message_id);
            message.state = PersistedAssistantMessageState::Done;
        }
        AgentProgressEvent::Status { .. }
        | AgentProgressEvent::FinalAssistantMessage(_)
        | AgentProgressEvent::RoundCompleted(_) => {}
    }

    next
}

fn finish_persisted_turn(
    timeline: &[PersistedTaskTimelineEntry],
    turn_id: &str,
    state: PersistedTurnState,
    error_message: Option<String>,
) -> PersistedTaskTimeline {
    let mut next = timeline.to_vec();
    if let Some(PersistedTaskTimelineEntry::Turn(turn)) = next.iter_mut().find(
        |entry| matches!(entry, PersistedTaskTimelineEntry::Turn(turn) if turn.turn_id == turn_id),
    ) {
        turn.state = state;
        turn.error_message = error_message;
        for message in &mut turn.messages {
            message.state = PersistedAssistantMessageState::Done;
        }
    }
    next
}

fn ensure_persisted_message<'a>(
    turn: &'a mut PersistedTurn,
    message_id: &str,
) -> &'a mut PersistedAssistantMessage {
    if let Some(index) = turn
        .messages
        .iter()
        .position(|message| message.message_id == message_id)
    {
        return &mut turn.messages[index];
    }

    turn.messages.push(PersistedAssistantMessage {
        message_id: message_id.to_string(),
        turn_id: turn.turn_id.clone(),
        state: PersistedAssistantMessageState::Streaming,
        reasoning: String::new(),
        timeline: Vec::new(),
    });
    turn.messages
        .last_mut()
        .expect("persisted message just pushed")
}

fn reply_ref_from_ui(reply: UiReplyRef) -> PersistedReplyRef {
    match reply {
        UiReplyRef::Turn { id } => PersistedReplyRef::Turn { id },
        UiReplyRef::UserMessage { id } => PersistedReplyRef::UserMessage { id },
    }
}

fn turn_trigger_from_ui(trigger: UiTurnTrigger) -> PersistedTurnTrigger {
    match trigger {
        UiTurnTrigger::User { id } => PersistedTurnTrigger::User { id },
        UiTurnTrigger::Turn { id } => PersistedTurnTrigger::Turn { id },
    }
}

fn save_session_with_timeline(
    storage: &mut MarchStorage,
    task_id: i64,
    session: &mut AgentSession,
    timeline: PersistedTaskTimeline,
) -> Result<()> {
    session.flush_memory_usage()?;
    let base_state = session.persisted_state();
    storage.save_task_state(
        task_id,
        &PersistedTaskState {
            timeline: Some(timeline),
            ..base_state
        },
    )
}
