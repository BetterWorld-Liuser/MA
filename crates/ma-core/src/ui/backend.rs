use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use base64::Engine;

use crate::agent::{
    AgentConfig, AgentProgressEvent, AgentSession, DebugRound, is_turn_cancelled_error,
};
use crate::context::{ContentBlock, ConversationHistory, join_text_blocks};
use crate::provider::{OpenAiCompatibleClient, fallback_task_title};
use crate::paths::{canonicalize_clean, clean_path};
use crate::settings::{ProviderType, SettingsStorage};
use crate::storage::{PersistedTask, PersistedTaskState, TaskRecord, TaskTitleSource};

use super::provider::provider_config_for_task;
use super::util::{resolve_context_window_fallback, system_time_to_unix};
use super::{
    DEFAULT_TASK_NAME, UI_MAX_RECENT_TURNS, UiAgentFailureStage, UiAgentProgressEvent,
    UiAppBackend, UiCloseOpenFileRequest, UiCreateTaskRequest, UiDebugTraceView,
    UiDeleteNoteRequest, UiDeleteProviderRequest, UiDeleteTaskRequest, UiOpenFilesRequest,
    UiComposerContentBlock, UiLoadWorkspaceImageRequest,
    UiProviderSettingsView, UiSearchWorkspaceEntriesRequest, UiSelectTaskRequest,
    UiSendMessageRequest, UiSetDefaultProviderRequest, UiSetTaskModelRequest,
    UiSetTaskWorkingDirectoryRequest, UiTaskSnapshot, UiUpsertNoteRequest, UiUpsertProviderRequest,
    UiWorkspaceEntryView, UiWorkspaceImageView, UiWorkspaceSnapshot,
};

impl UiAppBackend {
    pub fn open(workspace_path: impl Into<PathBuf>) -> Result<Self> {
        let workspace_path = clean_path(workspace_path.into());
        let storage = crate::storage::MaStorage::open(&workspace_path)?;
        Ok(Self {
            workspace_path,
            storage,
        })
    }

    pub fn resolve_or_create_task_id(&mut self, active_task_id: Option<i64>) -> Result<i64> {
        let tasks = self.storage.list_tasks()?;

        if let Some(task_id) =
            active_task_id.filter(|task_id| tasks.iter().any(|task| task.id == *task_id))
        {
            return Ok(task_id);
        }

        if let Some(task) = tasks.first() {
            return Ok(task.id);
        }

        Ok(self.create_task(DEFAULT_TASK_NAME)?.id)
    }

    pub fn create_task(&mut self, name: impl AsRef<str>) -> Result<TaskRecord> {
        let name = name.as_ref().trim();
        let (name, title_source, title_locked) = if name.is_empty() {
            (DEFAULT_TASK_NAME, TaskTitleSource::Default, false)
        } else {
            (name, TaskTitleSource::Manual, true)
        };
        let settings = SettingsStorage::open()?;
        let defaults = settings.snapshot()?;

        let task = self.storage.create_task_with_metadata_and_selection(
            name,
            title_source,
            title_locked,
            self.workspace_path.clone(),
            defaults.default_provider_id,
            defaults.default_model,
        )?;
        let session = AgentSession::new(
            ui_agent_config(),
            ConversationHistory::default(),
            [],
            self.workspace_path.clone(),
        )?;
        self.save_session(task.id, &session)?;
        Ok(task)
    }

    pub fn delete_task(&mut self, task_id: i64) -> Result<()> {
        self.storage.delete_task(task_id)
    }

    pub fn load_session(&self, task_id: i64) -> Result<AgentSession> {
        AgentSession::restore(ui_agent_config(), self.storage.load_task(task_id)?)
    }

    pub fn save_session(&mut self, task_id: i64, session: &AgentSession) -> Result<()> {
        self.storage
            .save_task_state(task_id, &session.persisted_state())
    }

    pub fn upsert_note(
        &mut self,
        task_id: i64,
        note_id: impl Into<String>,
        content: impl Into<String>,
    ) -> Result<()> {
        let mut session = self.load_session(task_id)?;
        session.write_note(note_id, content);
        self.save_session(task_id, &session)
    }

    pub fn delete_note(&mut self, task_id: i64, note_id: &str) -> Result<()> {
        let mut session = self.load_session(task_id)?;
        session.remove_note(note_id);
        self.save_session(task_id, &session)
    }

    pub fn set_open_file_lock(&mut self, task_id: i64, path: PathBuf, locked: bool) -> Result<()> {
        let mut session = self.load_session(task_id)?;
        if locked {
            session.lock_file(path)?;
        } else {
            session.unlock_file(path)?;
        }
        self.save_session(task_id, &session)
    }

    pub fn close_open_file(&mut self, task_id: i64, path: PathBuf) -> Result<()> {
        let mut session = self.load_session(task_id)?;
        session.close_file(path)?;
        self.save_session(task_id, &session)
    }

    pub fn open_files(&mut self, task_id: i64, paths: Vec<PathBuf>) -> Result<()> {
        let mut session = self.load_session(task_id)?;
        for path in paths {
            session.open_file(path)?;
        }
        self.save_session(task_id, &session)
    }

    pub fn workspace_snapshot(
        &mut self,
        active_task_id: Option<i64>,
    ) -> Result<UiWorkspaceSnapshot> {
        let active_task_id = self.resolve_or_create_task_id(active_task_id)?;
        let tasks = self
            .storage
            .list_tasks()?
            .into_iter()
            .map(super::UiTaskSummary::from)
            .collect::<Vec<_>>();
        let persisted = self.storage.load_task(active_task_id)?;
        let selected_model = self.selected_model_for_task(Some(active_task_id))?;
        let context_budget_tokens = resolve_context_window_fallback(selected_model.as_deref());
        let runtime = self
            .load_session(active_task_id)
            .ok()
            .map(|session| session.ui_runtime_snapshot(context_budget_tokens));
        let active_task = Some({
            let snapshot = UiTaskSnapshot::from_persisted(persisted);
            if let Some(runtime) = runtime {
                snapshot.with_runtime(&runtime)
            } else {
                snapshot
            }
        });

        Ok(UiWorkspaceSnapshot {
            workspace_path: clean_path(self.workspace_path.clone()),
            database_path: self.storage.database_path().to_path_buf(),
            tasks,
            active_task,
        })
    }

    pub fn task_snapshot(&self, task_id: i64) -> Result<UiTaskSnapshot> {
        let persisted = self.storage.load_task(task_id)?;
        Ok(UiTaskSnapshot::from_persisted(persisted))
    }

    pub fn task_snapshot_with_runtime(
        &self,
        task_id: i64,
        runtime: &super::UiRuntimeSnapshot,
    ) -> Result<UiTaskSnapshot> {
        self.task_snapshot(task_id)
            .map(|snapshot| snapshot.with_runtime(runtime))
    }

    pub async fn handle_send_message(
        &mut self,
        request: UiSendMessageRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        self.handle_send_message_with_progress_and_cancel(request, |_| Ok(()), || false)
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
        self.handle_send_message_with_progress_and_cancel(request, on_progress, || false)
            .await
    }

    pub async fn handle_send_message_with_progress_and_cancel<F, C>(
        &mut self,
        request: UiSendMessageRequest,
        mut on_progress: F,
        is_cancelled: C,
    ) -> Result<UiWorkspaceSnapshot>
    where
        F: FnMut(UiAgentProgressEvent) -> Result<()>,
        C: Fn() -> bool,
    {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let content_blocks = request
            .content_blocks
            .into_iter()
            .map(content_block_from_ui)
            .collect::<Vec<_>>();
        let content_text = join_text_blocks(&content_blocks);
        if content_blocks.is_empty()
            || content_blocks.iter().all(|block| match block {
                ContentBlock::Text { text } => text.trim().is_empty(),
                ContentBlock::Image { .. } => false,
            })
        {
            bail!("message cannot be empty");
        }

        let persisted_before = self.storage.load_task(task_id)?;
        let should_auto_title = should_auto_title(&persisted_before, &content_text);
        let provider_config = provider_config_for_task(&persisted_before.task)?;
        let provider = OpenAiCompatibleClient::new(provider_config.clone());
        let context_budget_tokens =
            resolve_context_window_with_provider(&provider, &provider_config.model)
                .await
                .unwrap_or_else(|| {
                    resolve_context_window_fallback(Some(provider_config.model.as_str()))
                });
        let mut session = self.load_session(task_id)?;
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
        })?;
        let result = session
            .handle_user_message_with_events_and_cancel(
                &provider,
                content_blocks,
                &is_cancelled,
                |session, event| {
                    match event {
                        AgentProgressEvent::Status { phase, label } => {
                            on_progress(UiAgentProgressEvent::Status {
                                task_id,
                                turn_id: turn_id.clone(),
                                phase: phase.into(),
                                label,
                            })?;
                        }
                        AgentProgressEvent::ToolStarted {
                            tool_call_id,
                            tool_name,
                            summary,
                        } => {
                            on_progress(UiAgentProgressEvent::ToolStarted {
                                task_id,
                                turn_id: turn_id.clone(),
                                tool_call_id,
                                tool_name,
                                summary,
                            })?;
                        }
                        AgentProgressEvent::ToolFinished {
                            tool_call_id,
                            status,
                            summary,
                            preview,
                        } => {
                            on_progress(UiAgentProgressEvent::ToolFinished {
                                task_id,
                                turn_id: turn_id.clone(),
                                tool_call_id,
                                status: status.into(),
                                summary,
                                preview,
                            })?;
                        }
                        AgentProgressEvent::AssistantTextPreview { message } => {
                            on_progress(UiAgentProgressEvent::AssistantTextPreview {
                                task_id,
                                turn_id: turn_id.clone(),
                                message,
                            })?;
                        }
                        AgentProgressEvent::FinalAssistantMessage(_) => {
                            let task = Self::live_task_snapshot(
                                progress_task.clone(),
                                session,
                                &progress_rounds,
                                context_budget_tokens,
                            )?;
                            on_progress(UiAgentProgressEvent::FinalAssistantMessage {
                                task_id,
                                turn_id: turn_id.clone(),
                                task,
                            })?;
                        }
                        AgentProgressEvent::RoundCompleted(round) => {
                            progress_rounds.push(round);
                            let task = Self::live_task_snapshot(
                                progress_task.clone(),
                                session,
                                &progress_rounds,
                                context_budget_tokens,
                            )?;
                            on_progress(UiAgentProgressEvent::RoundComplete {
                                task_id,
                                turn_id: turn_id.clone(),
                                task,
                            })?;
                        }
                    }
                    Ok(())
                },
            )
            .await;
        if let Err(error) = &result {
            self.save_session(task_id, &session)?;
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
        self.save_session(task_id, &session)?;
        if should_auto_title {
            let suggested_title = provider
                .suggest_task_title(&content_text)
                .await
                .ok()
                .flatten()
                .or_else(|| fallback_task_title(&content_text));
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

    fn live_task_snapshot(
        task: TaskRecord,
        session: &AgentSession,
        debug_rounds: &[DebugRound],
        context_budget_tokens: usize,
    ) -> Result<UiTaskSnapshot> {
        let PersistedTaskState {
            history,
            notes,
            open_files,
            hints,
            ..
        } = session.persisted_state();
        let runtime = session.ui_runtime_snapshot(context_budget_tokens);

        Ok(UiTaskSnapshot::from_persisted(PersistedTask {
            task,
            history,
            notes,
            open_files,
            hints,
        })
        .with_runtime(&runtime)
        .with_debug_trace(UiDebugTraceView::from_rounds(debug_rounds)))
    }

    pub fn handle_create_task(
        &mut self,
        request: UiCreateTaskRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task = self.create_task(request.name.unwrap_or_default())?;
        self.workspace_snapshot(Some(task.id))
    }

    fn apply_suggested_task_title(
        &self,
        task_id: i64,
        suggested_title: Option<String>,
    ) -> Result<()> {
        let Some(title) = suggested_title else {
            return Ok(());
        };

        let current = self.storage.load_task(task_id)?;
        if current.task.title_source != TaskTitleSource::Default || current.task.title_locked {
            return Ok(());
        }

        self.storage
            .update_task_title(task_id, title, TaskTitleSource::Auto, false)
    }

    pub fn handle_select_task(
        &mut self,
        request: UiSelectTaskRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        self.workspace_snapshot(Some(request.task_id))
    }

    pub fn handle_delete_task(
        &mut self,
        request: UiDeleteTaskRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        self.delete_task(request.task_id)?;

        let next_task_id = self.storage.list_tasks()?.first().map(|task| task.id);

        self.workspace_snapshot(next_task_id)
    }

    pub fn handle_upsert_note(
        &mut self,
        request: UiUpsertNoteRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let note_id = request.note_id.trim();
        if note_id.is_empty() {
            bail!("note_id cannot be empty");
        }
        let content = request.content.trim();
        if content.is_empty() {
            bail!("content cannot be empty");
        }

        self.upsert_note(task_id, note_id.to_string(), content.to_string())?;
        self.workspace_snapshot(Some(task_id))
    }

    pub fn handle_delete_note(
        &mut self,
        request: UiDeleteNoteRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let note_id = request.note_id.trim();
        if note_id.is_empty() {
            bail!("note_id cannot be empty");
        }

        self.delete_note(task_id, note_id)?;
        self.workspace_snapshot(Some(task_id))
    }

    pub fn handle_toggle_open_file_lock(
        &mut self,
        request: super::UiToggleOpenFileLockRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        self.set_open_file_lock(task_id, request.path, request.locked)?;
        self.workspace_snapshot(Some(task_id))
    }

    pub fn handle_close_open_file(
        &mut self,
        request: UiCloseOpenFileRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        self.close_open_file(task_id, request.path)?;
        self.workspace_snapshot(Some(task_id))
    }

    pub fn handle_open_files(
        &mut self,
        request: UiOpenFilesRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        self.open_files(task_id, request.paths)?;
        self.workspace_snapshot(Some(task_id))
    }

    pub fn selected_model_for_task(&self, task_id: Option<i64>) -> Result<Option<String>> {
        let task_model = task_id
            .and_then(|id| self.storage.load_task(id).ok())
            .and_then(|task| task.task.selected_model);

        if task_model.is_some() {
            return Ok(task_model);
        }

        let settings = SettingsStorage::open()?;
        settings.default_model()
    }

    pub fn task_record_for_provider_models(
        &self,
        task_id: Option<i64>,
    ) -> Result<Option<TaskRecord>> {
        task_id
            .map(|id| self.storage.load_task(id).map(|persisted| persisted.task))
            .transpose()
    }

    pub fn handle_set_task_model(
        &mut self,
        request: UiSetTaskModelRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let model = request.model.trim();
        if model.is_empty() {
            bail!("model cannot be empty");
        }
        let task = self.storage.load_task(task_id)?;
        let provider_id = request
            .provider_id
            .or(task.task.selected_provider_id)
            .or(SettingsStorage::open()?.snapshot()?.default_provider_id);
        self.storage
            .update_task_selection(task_id, provider_id, Some(model.to_string()))?;
        self.workspace_snapshot(Some(task_id))
    }

    pub fn handle_set_task_working_directory(
        &mut self,
        request: UiSetTaskWorkingDirectoryRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let working_directory = self.normalize_task_working_directory(request.path)?;
        self.storage
            .update_task_working_directory(task_id, working_directory)?;
        let task = self.storage.load_task(task_id)?;
        let session = AgentSession::restore(ui_agent_config(), task)?;
        self.save_session(task_id, &session)?;
        self.workspace_snapshot(Some(task_id))
    }

    fn normalize_task_working_directory(&self, path: Option<PathBuf>) -> Result<PathBuf> {
        let requested = path.unwrap_or_else(|| self.workspace_path.clone());
        let normalized =
            canonicalize_clean(&requested).with_context(|| format!("failed to resolve {}", requested.display()))?;
        if !normalized.is_dir() {
            bail!(
                "working directory must be a directory: {}",
                normalized.display()
            );
        }
        Ok(normalized)
    }

    fn working_directory_for_task(&self, task_id: Option<i64>) -> Result<PathBuf> {
        match task_id {
            Some(task_id) => Ok(self.storage.load_task(task_id)?.task.working_directory),
            None => Ok(self.workspace_path.clone()),
        }
    }

    pub fn provider_settings(&self) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        Ok(UiProviderSettingsView::from_snapshot(
            settings.database_path().to_path_buf(),
            settings.snapshot()?,
        ))
    }

    pub fn handle_upsert_provider(
        &self,
        request: UiUpsertProviderRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        let provider_type =
            ProviderType::from_db_value(&request.provider_type).ok_or_else(|| {
                anyhow::anyhow!("unsupported provider type {}", request.provider_type)
            })?;
        settings.upsert_provider(
            request.id,
            provider_type,
            request.name,
            request.api_key,
            request.base_url,
        )?;
        Ok(UiProviderSettingsView::from_snapshot(
            settings.database_path().to_path_buf(),
            settings.snapshot()?,
        ))
    }

    pub fn handle_delete_provider(
        &self,
        request: UiDeleteProviderRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        settings.delete_provider(request.provider_id)?;
        Ok(UiProviderSettingsView::from_snapshot(
            settings.database_path().to_path_buf(),
            settings.snapshot()?,
        ))
    }

    pub fn handle_set_default_provider(
        &mut self,
        request: UiSetDefaultProviderRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        let previous = settings.snapshot()?;
        self.storage
            .backfill_missing_task_defaults(previous.default_provider_id, previous.default_model)?;
        settings.set_default_provider(request.provider_id, request.model)?;
        Ok(UiProviderSettingsView::from_snapshot(
            settings.database_path().to_path_buf(),
            settings.snapshot()?,
        ))
    }

    pub fn search_workspace_entries(
        &self,
        request: UiSearchWorkspaceEntriesRequest,
    ) -> Result<Vec<UiWorkspaceEntryView>> {
        let limit = request.limit.unwrap_or(12).clamp(1, 50);
        let working_directory = self.working_directory_for_task(request.task_id)?;
        super::workspace::search_workspace_entries(
            &working_directory,
            &request.query,
            request.kind,
            limit,
        )
    }

    pub fn load_workspace_image(
        &self,
        request: UiLoadWorkspaceImageRequest,
    ) -> Result<UiWorkspaceImageView> {
        let working_directory = self.working_directory_for_task(request.task_id)?;
        let resolved_path = resolve_workspace_path(&working_directory, &request.path)?;
        let media_type = infer_image_media_type(&resolved_path)
            .ok_or_else(|| anyhow::anyhow!("unsupported image format: {}", resolved_path.display()))?;
        let bytes = fs::read(&resolved_path)
            .with_context(|| format!("failed to read image {}", resolved_path.display()))?;

        Ok(UiWorkspaceImageView {
            path: clean_path(resolved_path.clone()),
            media_type: media_type.to_string(),
            data_url: format!(
                "data:{};base64,{}",
                media_type,
                base64::engine::general_purpose::STANDARD.encode(bytes),
            ),
            name: resolved_path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| resolved_path.display().to_string()),
        })
    }

    pub fn workspace_path(&self) -> &std::path::Path {
        &self.workspace_path
    }
}

fn ui_agent_config() -> AgentConfig {
    AgentConfig {
        max_recent_turns: UI_MAX_RECENT_TURNS,
        ..AgentConfig::default()
    }
}

fn should_auto_title(task: &PersistedTask, user_message: &str) -> bool {
    task.task.title_source == TaskTitleSource::Default
        && !task.task.title_locked
        && task.history.turns.is_empty()
        && user_message.trim().chars().count() >= 4
}

fn classify_turn_failure(error: &anyhow::Error) -> (UiAgentFailureStage, bool) {
    let message = error.to_string().to_ascii_lowercase();

    if message.contains("tool ")
        || message.contains("invalid ")
        || message.contains("failed to decode arguments for tool")
        || message.contains("write_file")
        || message.contains("replace_lines")
        || message.contains("insert_lines")
        || message.contains("delete_lines")
        || message.contains("run_command")
        || message.contains("open_file")
    {
        return (UiAgentFailureStage::Tool, true);
    }

    if message.contains("provider")
        || message.contains("chat completion")
        || message.contains("model list")
        || message.contains("request chat")
        || message.contains("stream")
        || message.contains("assistant text without tool calls")
        || message.contains("neither assistant text nor tool calls")
    {
        return (UiAgentFailureStage::Provider, true);
    }

    if message.contains("context") {
        return (UiAgentFailureStage::Context, true);
    }

    (UiAgentFailureStage::Internal, false)
}

async fn resolve_context_window_with_provider(
    provider: &OpenAiCompatibleClient,
    current_model: &str,
) -> Option<usize> {
    provider
        .resolve_model_context_window(current_model)
        .await
        .ok()
        .flatten()
        .or_else(|| Some(resolve_context_window_fallback(Some(current_model))))
}

fn resolve_workspace_path(working_directory: &Path, path: &Path) -> Result<PathBuf> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_directory.join(path)
    };

    canonicalize_clean(&candidate).with_context(|| format!("failed to resolve {}", candidate.display()))
}

fn infer_image_media_type(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_string_lossy().to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

fn content_block_from_ui(block: UiComposerContentBlock) -> ContentBlock {
    match block {
        UiComposerContentBlock::Text { text } => ContentBlock::text(text),
        UiComposerContentBlock::Image {
            media_type,
            data_base64,
            source_path,
            name,
        } => ContentBlock::image(media_type, data_base64, source_path, name),
    }
}
