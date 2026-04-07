use super::*;

impl UiAppBackend {
    pub async fn handle_send_message(
        &mut self,
        request: UiSendMessageRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        self.handle_send_message_with_progress_and_cancel(
            request,
            |_| Ok(()),
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
            TurnCancellation::never(),
        )
        .await
    }

    pub async fn handle_send_message_with_progress_and_cancel<F>(
        &mut self,
        request: UiSendMessageRequest,
        mut on_progress: F,
        cancellation: &TurnCancellation,
    ) -> Result<UiWorkspaceSnapshot>
    where
        F: FnMut(UiAgentProgressEvent) -> Result<()>,
    {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
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
        let turn_id = format!(
            "turn-{}-{}",
            task_id,
            system_time_to_unix(SystemTime::now())
        );
        let progress_task = self
            .storage
            .list_tasks()?
            .into_iter()
            .find(|task| task.id == task_id)
            .ok_or_else(|| anyhow::anyhow!("task {} not found", task_id))?;
        let mut progress_rounds = Vec::new();
        on_progress(UiAgentProgressEvent::TurnStarted {
            task_id,
            turn_id: turn_id.clone(),
            user_message: content_text.clone(),
            agent: session.active_agent_name().to_string(),
            agent_display_name: session.display_name_for_agent(session.active_agent_name()),
        })?;
        let mut result = session
            .handle_user_message_with_events_and_cancel(
                &provider,
                content_blocks,
                cancellation,
                |session, event| {
                    Self::forward_progress_event(
                        task_id,
                        &turn_id,
                        progress_task.clone(),
                        session,
                        &mut progress_rounds,
                        context_budget_tokens,
                        &mut on_progress,
                        event,
                    )
                },
            )
            .await;
        if let Ok(accumulated) = &mut result {
            self.handle_agent_mentions(
                task_id,
                &persisted_before,
                &mut session,
                accumulated,
                cancellation,
                &turn_id,
                progress_task.clone(),
                &mut progress_rounds,
                context_budget_tokens,
                &mut on_progress,
            )
            .await?;
        }
        if let Err(error) = &result {
            self.save_session(task_id, &mut session)?;
            if is_turn_cancelled_error(error) {
                let task = Self::live_task_snapshot(
                    progress_task.clone(),
                    &session,
                    &progress_rounds,
                    context_budget_tokens,
                )?;
                on_progress(UiAgentProgressEvent::TurnCancelled {
                    task_id,
                    turn_id: turn_id.clone(),
                    task,
                })?;
                return self.workspace_snapshot(Some(task_id));
            }
            let (stage, retryable) = classify_turn_failure(error);
            on_progress(UiAgentProgressEvent::TurnFailed {
                task_id,
                turn_id: turn_id.clone(),
                stage,
                message: error.to_string(),
                retryable,
            })?;
        }
        let result = result?;
        let runtime = session.ui_runtime_snapshot(context_budget_tokens);
        self.save_session(task_id, &mut session)?;
        if should_auto_title {
            let suggested_title = Self::suggest_auto_title(&provider, &content_text).await;
            self.apply_suggested_task_title(task_id, suggested_title)?;
        }
        let mut workspace = self.workspace_snapshot(Some(task_id))?;
        if let Some(active_task) = workspace.active_task.take() {
            workspace.active_task = Some(
                active_task
                    .with_runtime(&runtime)
                    .with_debug_trace(UiDebugTraceView::from_rounds(&result.debug_rounds)),
            );
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
        let mut session = self.load_session(task_id)?;
        self.apply_agent_switch(task_id, &mut session, content_text)?;
        Ok(PreparedMessageContext {
            persisted_task,
            session,
            should_auto_title,
        })
    }

    fn apply_agent_switch(
        &mut self,
        task_id: i64,
        session: &mut AgentSession,
        content_text: &str,
    ) -> Result<()> {
        if let Some(agent_name) = detect_agent_mention(content_text, session) {
            session.set_active_agent(agent_name);
            self.storage
                .update_task_active_agent(task_id, session.active_agent_name())?;
        }
        Ok(())
    }

    async fn handle_agent_mentions<F>(
        &mut self,
        task_id: i64,
        persisted_before: &PersistedTask,
        session: &mut AgentSession,
        accumulated: &mut AgentRunResult,
        cancellation: &TurnCancellation,
        turn_id: &str,
        progress_task: TaskRecord,
        progress_rounds: &mut Vec<DebugRound>,
        context_budget_tokens: usize,
        on_progress: &mut F,
    ) -> Result<()>
    where
        F: FnMut(UiAgentProgressEvent) -> Result<()>,
    {
        while let Some(agent_name) = accumulated
            .final_messages
            .last()
            .and_then(|message| detect_agent_mention(&message.message, session))
            .filter(|agent_name| agent_name != session.active_agent_name())
        {
            session.set_active_agent(agent_name.clone());
            self.storage
                .update_task_active_agent(task_id, session.active_agent_name())?;
            let provider_config = provider_config_for_session(&persisted_before.task, session)?;
            let provider = OpenAiCompatibleClient::new(provider_config);
            let continuation = session
                .continue_with_events_and_cancel(&provider, cancellation, |session, event| {
                    Self::forward_progress_event(
                        task_id,
                        turn_id,
                        progress_task.clone(),
                        session,
                        progress_rounds,
                        context_budget_tokens,
                        on_progress,
                        event,
                    )
                })
                .await?;
            accumulated
                .final_messages
                .extend(continuation.final_messages);
            accumulated.debug_rounds.extend(continuation.debug_rounds);
        }
        Ok(())
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

    fn forward_progress_event<F>(
        task_id: i64,
        turn_id: &str,
        progress_task: TaskRecord,
        session: &AgentSession,
        progress_rounds: &mut Vec<DebugRound>,
        context_budget_tokens: usize,
        on_progress: &mut F,
        event: AgentProgressEvent,
    ) -> Result<()>
    where
        F: FnMut(UiAgentProgressEvent) -> Result<()>,
    {
        let runtime = session.ui_runtime_snapshot(context_budget_tokens);
        match event {
            AgentProgressEvent::Status {
                agent,
                phase,
                label,
            } => {
                let agent_display_name = session.display_name_for_agent(&agent);
                on_progress(UiAgentProgressEvent::Status {
                    task_id,
                    turn_id: turn_id.to_string(),
                    agent,
                    agent_display_name,
                    phase: phase.into(),
                    label,
                    runtime: runtime.clone(),
                })
            }
            AgentProgressEvent::ToolStarted {
                tool_call_id,
                tool_name,
                summary,
            } => on_progress(UiAgentProgressEvent::ToolStarted {
                task_id,
                turn_id: turn_id.to_string(),
                tool_call_id,
                tool_name,
                summary,
                runtime: runtime.clone(),
            }),
            AgentProgressEvent::ToolFinished {
                tool_call_id,
                status,
                summary,
                preview,
            } => on_progress(UiAgentProgressEvent::ToolFinished {
                task_id,
                turn_id: turn_id.to_string(),
                tool_call_id,
                status: status.into(),
                summary,
                preview,
                runtime: runtime.clone(),
            }),
            AgentProgressEvent::AssistantTextPreview { agent, message } => {
                let agent_display_name = session.display_name_for_agent(&agent);
                on_progress(UiAgentProgressEvent::AssistantTextPreview {
                    task_id,
                    turn_id: turn_id.to_string(),
                    agent,
                    agent_display_name,
                    message,
                    runtime: runtime.clone(),
                })
            }
            AgentProgressEvent::AssistantMessageCheckpoint(checkpoint) => {
                let agent = session.active_agent_name().to_string();
                let agent_display_name = session.display_name_for_agent(&agent);
                on_progress(UiAgentProgressEvent::AssistantMessageCheckpoint {
                    task_id,
                    turn_id: turn_id.to_string(),
                    agent,
                    agent_display_name,
                    message_id: checkpoint.message_id,
                    content: checkpoint.message,
                    checkpoint_type: checkpoint.checkpoint_type.into(),
                    runtime: runtime.clone(),
                })
            }
            AgentProgressEvent::FinalAssistantMessage(_) => {
                let task = Self::live_task_snapshot(
                    progress_task,
                    session,
                    progress_rounds,
                    context_budget_tokens,
                )?;
                on_progress(UiAgentProgressEvent::FinalAssistantMessage {
                    task_id,
                    turn_id: turn_id.to_string(),
                    task,
                })
            }
            AgentProgressEvent::RoundCompleted(round) => {
                progress_rounds.push(round);
                let task = Self::live_task_snapshot(
                    progress_task,
                    session,
                    progress_rounds,
                    context_budget_tokens,
                )?;
                on_progress(UiAgentProgressEvent::RoundComplete {
                    task_id,
                    turn_id: turn_id.to_string(),
                    task,
                })
            }
        }
    }
}
