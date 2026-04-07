use super::*;
use crate::ui::workspace::{search_mentions, search_skills, search_workspace_entries};
use crate::ui::{UiRuntimeSnapshot, UiTaskSummary, UiToggleOpenFileLockRequest};

impl UiAppBackend {
    pub fn open(workspace_path: impl Into<PathBuf>) -> Result<Self> {
        let workspace_path = clean_path(workspace_path.into());
        let storage = crate::storage::MarchStorage::open(&workspace_path)?;
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

        let mut options = TaskCreateOptions::new(self.workspace_path.clone());
        options.title_source = title_source;
        options.title_locked = title_locked;
        options.selected_model_config_id = defaults.default_model_config_id;
        options.selected_model = settings.default_model()?;
        let task = self.storage.create_task_with_options(name, options)?;
        let mut session = AgentSession::new(
            ui_agent_config(),
            task.name.clone(),
            ConversationHistory::default(),
            [],
            self.workspace_path.clone(),
        )?;
        self.save_session(task.id, &mut session)?;
        Ok(task)
    }

    pub fn delete_task(&mut self, task_id: i64) -> Result<()> {
        self.storage.delete_task(task_id)
    }

    pub fn load_session(&self, task_id: i64) -> Result<AgentSession> {
        AgentSession::restore(ui_agent_config(), self.storage.load_task(task_id)?)
    }

    pub fn save_session(&mut self, task_id: i64, session: &mut AgentSession) -> Result<()> {
        session.flush_memory_usage()?;
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
        session.write_note_in_scope(SHARED_SCOPE.to_string(), note_id, content);
        self.save_session(task_id, &mut session)
    }

    pub fn delete_note(&mut self, task_id: i64, note_id: &str) -> Result<()> {
        let mut session = self.load_session(task_id)?;
        session.remove_note_in_scope(SHARED_SCOPE.to_string(), note_id);
        self.save_session(task_id, &mut session)
    }

    pub fn set_open_file_lock(&mut self, task_id: i64, path: PathBuf, locked: bool) -> Result<()> {
        let mut session = self.load_session(task_id)?;
        session.set_lock_file_in_scope(SHARED_SCOPE.to_string(), path, locked)?;
        self.save_session(task_id, &mut session)
    }

    pub fn close_open_file(&mut self, task_id: i64, path: PathBuf) -> Result<()> {
        let mut session = self.load_session(task_id)?;
        session.close_file_in_scope(SHARED_SCOPE.to_string(), path)?;
        self.save_session(task_id, &mut session)
    }

    pub fn open_files(&mut self, task_id: i64, paths: Vec<PathBuf>) -> Result<()> {
        let mut session = self.load_session(task_id)?;
        for path in paths {
            session.open_file_in_scope(SHARED_SCOPE.to_string(), path)?;
        }
        self.save_session(task_id, &mut session)
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
            .map(UiTaskSummary::from)
            .collect::<Vec<_>>();
        let persisted = self.storage.load_task(active_task_id)?;
        let selected_model = self.selected_model_for_task(Some(active_task_id))?;
        let context_budget_tokens = resolve_context_window_fallback(selected_model.as_deref());
        let display_session = self.load_session(active_task_id).ok();
        let runtime = display_session
            .as_ref()
            .map(|session| session.ui_runtime_snapshot(context_budget_tokens));
        let active_task = Some({
            let snapshot = UiTaskSnapshot::from_persisted(persisted);
            let snapshot = if let Some(session) = display_session.as_ref() {
                snapshot.with_agent_display_names(session)
            } else {
                snapshot
            };
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
        let session = self.load_session(task_id)?;
        Ok(UiTaskSnapshot::from_persisted(persisted).with_agent_display_names(&session))
    }

    pub fn task_snapshot_with_runtime(
        &self,
        task_id: i64,
        runtime: &UiRuntimeSnapshot,
    ) -> Result<UiTaskSnapshot> {
        self.task_snapshot(task_id)
            .map(|snapshot| snapshot.with_runtime(runtime))
    }

    pub fn handle_create_task(
        &mut self,
        request: UiCreateTaskRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task = self.create_task(request.name.unwrap_or_default())?;
        self.workspace_snapshot(Some(task.id))
    }

    pub(super) fn apply_suggested_task_title(
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
        request: UiToggleOpenFileLockRequest,
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
        let settings = SettingsStorage::open()?;
        let model_config = settings.load_model_config(request.model_config_id)?;
        self.storage.update_task_selection(
            task_id,
            Some(model_config.id),
            Some(model_config.model_id),
        )?;
        self.workspace_snapshot(Some(task_id))
    }

    pub fn handle_set_task_model_settings(
        &mut self,
        request: UiSetTaskModelSettingsRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        self.storage.update_task_model_settings(
            task_id,
            request.temperature,
            request.top_p,
            request.presence_penalty,
            request.frequency_penalty,
            request.max_output_tokens,
        )?;
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
        let mut session = AgentSession::restore(ui_agent_config(), task)?;
        self.save_session(task_id, &mut session)?;
        self.workspace_snapshot(Some(task_id))
    }

    fn normalize_task_working_directory(&self, path: Option<PathBuf>) -> Result<PathBuf> {
        let requested = path.unwrap_or_else(|| self.workspace_path.clone());
        let normalized = canonicalize_clean(&requested)
            .with_context(|| format!("failed to resolve {}", requested.display()))?;
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

    pub(super) fn live_task_snapshot(
        task: TaskRecord,
        session: &AgentSession,
        debug_rounds: &[DebugRound],
        context_budget_tokens: usize,
    ) -> Result<UiTaskSnapshot> {
        let PersistedTaskState {
            active_agent,
            history,
            notes,
            open_files,
            hints,
            ..
        } = session.persisted_state();
        let runtime = session.ui_runtime_snapshot(context_budget_tokens);

        Ok(UiTaskSnapshot::from_persisted(PersistedTask {
            task,
            active_agent,
            history,
            notes,
            open_files,
            hints,
        })
        .with_agent_display_names(session)
        .with_runtime(&runtime)
        .with_debug_trace(UiDebugTraceView::from_rounds(debug_rounds)))
    }

    pub fn search_workspace_entries(
        &self,
        request: UiSearchWorkspaceEntriesRequest,
    ) -> Result<Vec<UiWorkspaceEntryView>> {
        let limit = request.limit.unwrap_or(12).clamp(1, 50);
        let working_directory = self.working_directory_for_task(request.task_id)?;
        search_workspace_entries(&working_directory, &request.query, request.kind, limit)
    }

    pub fn search_mentions(
        &self,
        request: UiSearchWorkspaceEntriesRequest,
    ) -> Result<Vec<UiMentionTargetView>> {
        let limit = request.limit.unwrap_or(12).clamp(1, 50);
        let working_directory = self.working_directory_for_task(request.task_id)?;
        search_mentions(&working_directory, &request.query, limit)
    }

    pub fn search_skills(&self, request: UiSearchSkillsRequest) -> Result<Vec<UiSkillSearchView>> {
        let Some(task_id) = request.task_id else {
            return Ok(Vec::new());
        };

        let limit = request.limit.unwrap_or(12).clamp(1, 50);
        let session = self.load_session(task_id)?;
        let opened_paths = session
            .runtime_open_file_snapshots()
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        Ok(search_skills(
            session.skills(),
            &opened_paths,
            &request.query,
            limit,
        ))
    }

    pub fn load_workspace_image(
        &self,
        request: UiLoadWorkspaceImageRequest,
    ) -> Result<UiWorkspaceImageView> {
        let working_directory = self.working_directory_for_task(request.task_id)?;
        let resolved_path = resolve_workspace_path(&working_directory, &request.path)?;
        let media_type = infer_image_media_type(&resolved_path).ok_or_else(|| {
            anyhow::anyhow!("unsupported image format: {}", resolved_path.display())
        })?;
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

    pub fn task_working_directories(&self) -> Result<Vec<PathBuf>> {
        let mut directories = self
            .storage
            .list_tasks()?
            .into_iter()
            .map(|task| clean_path(task.working_directory))
            .collect::<Vec<_>>();
        directories.sort();
        directories.dedup();
        Ok(directories)
    }

    pub fn list_memories(
        &mut self,
        request: UiListMemoriesRequest,
    ) -> Result<Vec<UiMemoryDetailView>> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let session = self.load_session(task_id)?;
        let working_directory = self.working_directory_for_task(Some(task_id))?;
        let mut manager = MemoryManager::load(&working_directory)?;
        let memories = manager
            .list_visible(session.active_agent_name())?
            .into_iter()
            .map(UiMemoryDetailView::from)
            .collect::<Vec<_>>();
        Ok(memories)
    }

    pub fn get_memory(&mut self, request: UiGetMemoryRequest) -> Result<UiMemoryDetailView> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let session = self.load_session(task_id)?;
        let working_directory = self.working_directory_for_task(Some(task_id))?;
        let mut manager = MemoryManager::load(&working_directory)?;
        manager
            .peek(&request.id, session.active_agent_name())
            .map(UiMemoryDetailView::from)
    }

    pub fn handle_upsert_memory(
        &mut self,
        request: UiUpsertMemoryRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let session = self.load_session(task_id)?;
        let working_directory = self.working_directory_for_task(Some(task_id))?;
        let mut manager = MemoryManager::load(&working_directory)?;
        manager.memorize(
            MemorizeRequest {
                id: request.id,
                memory_type: request.memory_type,
                topic: request.topic,
                title: request.title,
                content: request.content,
                tags: request.tags,
                scope: request.scope,
                level: request.level,
            },
            session.active_agent_name(),
        )?;
        self.workspace_snapshot(Some(task_id))
    }

    pub fn handle_delete_memory(
        &mut self,
        request: UiDeleteMemoryRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let working_directory = self.working_directory_for_task(Some(task_id))?;
        let mut manager = MemoryManager::load(&working_directory)?;
        manager.forget(&request.id)?;
        self.workspace_snapshot(Some(task_id))
    }
}
