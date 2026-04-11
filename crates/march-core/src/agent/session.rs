use std::time::Duration;

use super::*;
use crate::diagnostics::{
    DiagnosticChannel, DiagnosticLevel, DiagnosticLogger, DiagnosticRecord, now_timestamp_ms,
};
use crate::paths::resolve_project_root;
use crate::storage::history_from_timeline;

pub const DEFAULT_RUN_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);

impl AgentSession {
    pub fn new(
        config: AgentConfig,
        task_name: impl Into<String>,
        history: ConversationHistory,
        open_files: impl IntoIterator<Item = PathBuf>,
        working_directory: PathBuf,
    ) -> Result<Self> {
        let normalized_open_files = normalize_open_files_for_workspace(
            &working_directory,
            open_files.into_iter().map(|path| PersistedOpenFile {
                scope: SHARED_SCOPE.to_string(),
                path,
                locked: false,
            }),
        );
        Self::create(
            config,
            task_name.into(),
            history,
            normalized_open_files,
            working_directory,
            MARCH_AGENT_NAME.to_string(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    pub fn restore(config: AgentConfig, task: PersistedTask) -> Result<Self> {
        let working_directory = task.task.working_directory.clone();
        let open_files = normalize_open_files_for_workspace(&working_directory, task.open_files);
        let history = history_from_timeline(&task.timeline);
        Self::create(
            config,
            task.task.name,
            history,
            open_files,
            working_directory,
            task.active_agent,
            task.notes,
            task.hints,
            Vec::new(),
        )
    }

    fn create(
        config: AgentConfig,
        task_name: String,
        history: ConversationHistory,
        open_files: Vec<PersistedOpenFile>,
        working_directory: PathBuf,
        active_agent: String,
        notes: Vec<PersistedNote>,
        hints: Vec<Hint>,
        injections: Vec<Injection>,
    ) -> Result<Self> {
        let mut watcher = FileWatcherService::new()?;
        for open_file in &open_files {
            watcher.watch_file(open_file.path.clone())?;
        }

        let agent_profiles = load_agent_profiles(&working_directory)?
            .into_iter()
            .map(|profile| (profile.name.clone(), profile))
            .collect::<IndexMap<_, _>>();
        let memory_manager = MemoryManager::load(&working_directory)?;
        let (skills, skill_injection) = load_skills_for_workspace(&working_directory)?;
        let mut injections = injections;
        upsert_injection(&mut injections, skill_injection);
        let active_agent = if agent_profiles.contains_key(&active_agent) {
            active_agent
        } else {
            MARCH_AGENT_NAME.to_string()
        };

        Ok(Self {
            config,
            watcher,
            agent_profiles,
            task_name,
            active_agent,
            history,
            notes: super::scopes::notes_by_scope(notes),
            open_files,
            hints,
            memory_manager,
            last_memory_index: None,
            injections,
            skills,
            available_shells: detect_available_shells()?,
            working_directory,
        })
    }

    pub fn add_injection(&mut self, id: impl Into<String>, content: impl Into<String>) {
        let id = id.into();
        let content = content.into();
        if let Some(injection) = self
            .injections
            .iter_mut()
            .find(|injection| injection.id == id)
        {
            injection.content = content;
        } else {
            self.injections.push(Injection { id, content });
        }
    }

    pub fn add_user_turn(&mut self, content: impl Into<Vec<ContentBlock>>) {
        self.history.turns.push(DisplayTurn {
            role: Role::User,
            agent: self.active_agent.clone(),
            content: content.into(),
            tool_calls: Vec::new(),
            timestamp: SystemTime::now(),
        });
    }

    pub fn add_assistant_turn(
        &mut self,
        content: impl Into<Vec<ContentBlock>>,
        tool_calls: Vec<ToolSummary>,
    ) {
        self.history.turns.push(DisplayTurn {
            role: Role::Assistant,
            agent: self.active_agent.clone(),
            content: content.into(),
            tool_calls,
            timestamp: SystemTime::now(),
        });
    }

    pub fn add_hint(&mut self, hint: Hint) {
        self.hints.push(hint);
    }

    pub fn write_note(&mut self, id: impl Into<String>, content: impl Into<String>) {
        self.write_note_in_scope(self.private_scope().to_string(), id, content);
    }

    pub fn remove_note_in_scope(&mut self, scope: impl Into<String>, id: &str) {
        let scope = scope.into();
        if let Some(notes) = self.notes.get_mut(&scope) {
            notes.shift_remove(id);
        }
    }

    pub fn remove_note(&mut self, id: &str) {
        self.remove_note_in_scope(self.private_scope().to_string(), id);
    }

    pub fn open_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.open_file_in_scope(self.private_scope().to_string(), path)
    }

    pub fn close_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.close_file_in_scope(self.private_scope().to_string(), path)
    }

    pub fn close_file_in_scope(
        &mut self,
        scope: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<()> {
        let path = self.resolve_path(path.into());
        let scope = scope.into();
        if self
            .open_files
            .iter()
            .any(|entry| entry.scope == scope && entry.path == path && entry.locked)
        {
            bail!("cannot close locked file {}", path.display());
        }
        self.open_files
            .retain(|entry| !(entry.scope == scope && entry.path == path));
        if !self.open_files.iter().any(|entry| entry.path == path) {
            self.watcher.unwatch_file(path)?;
        }
        Ok(())
    }

    pub fn lock_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.set_lock_file_in_scope(self.private_scope().to_string(), path, true)
    }

    pub fn set_lock_file_in_scope(
        &mut self,
        scope: impl Into<String>,
        path: impl Into<PathBuf>,
        locked: bool,
    ) -> Result<()> {
        let path = self.resolve_path(path.into());
        let scope = scope.into();
        let Some(entry) = self
            .open_files
            .iter_mut()
            .find(|entry| entry.scope == scope && entry.path == path)
        else {
            bail!("cannot lock unopened file {}", path.display());
        };
        entry.locked = locked;
        Ok(())
    }

    pub fn unlock_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.set_lock_file_in_scope(self.private_scope().to_string(), path, false)
    }

    pub fn build_context(&mut self) -> AgentContext {
        self.prune_expired_hints();
        let tools = ToolRuntime::for_session(&self.available_shells, &self.working_directory).tools;
        let notes = self.notes_for_active_agent();
        let open_files = self.open_file_snapshots_for_active_agent();
        let memory_index = self
            .memory_manager
            .search(&self.build_memory_query(), 12)
            .ok()
            .filter(|index| !index.is_empty());
        self.last_memory_index = memory_index.clone();
        let context = AgentContextBuilder::new(self.system_core_for_active_agent())
            .with_config(ContextBuildConfig {
                max_recent_chat_turns: self.config.max_recent_turns,
                max_recent_chat_image_turns: 4,
            })
            .injections(self.injections.clone())
            .tools(tools)
            .notes(notes)
            .memory_index(memory_index)
            .session_status(self.session_status())
            .runtime_status(SystemStatus {
                locked_files: self.locked_files_for_active_agent(),
                context_pressure: self.estimate_context_pressure(DEFAULT_CONTEXT_WINDOW_TOKENS),
            })
            .hints(self.hints.clone())
            .history(self.history.clone())
            .build_from_open_files(open_files);
        self.tick_hints();
        context
    }

    pub fn render_prompt(&mut self) -> String {
        let context = self.build_context();
        render_prompt(&context)
    }

    pub fn last_memory_index(&self) -> Option<crate::memory::MemoryIndexView> {
        self.last_memory_index.clone()
    }

    pub fn restore_last_memory_index(
        &mut self,
        memory_index: Option<crate::memory::MemoryIndexView>,
    ) {
        self.last_memory_index = memory_index.filter(|index| !index.is_empty());
    }

    pub async fn run_command_with_output<F>(
        &mut self,
        request: CommandRequest,
        cancellation: &TurnCancellation,
        mut on_output: F,
    ) -> Result<CommandExecution>
    where
        F: FnMut(crate::agent::CommandOutputStreamUpdate) -> Result<()>,
    {
        let started_at = SystemTime::now();
        let selected_shell = self.resolve_shell(request.shell)?;
        let diagnostic_logger =
            DiagnosticLogger::new(&resolve_project_root(&self.working_directory))?;
        let command_text = request.command.clone();
        let shell_label = selected_shell.kind.label().to_string();
        let timeout_secs = request.timeout.as_secs().to_string();
        write_command_diagnostic(
            &diagnostic_logger,
            DiagnosticLevel::Info,
            "command.started",
            "command started",
            [
                ("shell", shell_label.clone()),
                ("timeout_secs", timeout_secs.clone()),
                ("command", command_text.clone()),
            ],
        )?;
        let tracked_paths = self
            .open_file_snapshots()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let _guard = self
            .watcher
            .store()
            .begin_agent_write(tracked_paths.clone())?;
        let output = shell_command_with_cancel(
            selected_shell.kind,
            &selected_shell.program,
            &request.command,
            &self.working_directory,
            request.timeout,
            cancellation,
            &mut on_output,
        )
        .await;

        for path in tracked_paths {
            if path.exists() {
                self.watcher
                    .store()
                    .refresh_file(path, crate::context::ModifiedBy::Agent)?;
            } else {
                self.watcher
                    .store()
                    .remove_file(path, crate::context::ModifiedBy::Agent)?;
            }
        }

        let output = match output {
            Ok(output) => output,
            Err(error) => {
                let error_message = error.to_string();
                let (event_name, level) = if error_message.contains("command timed out after") {
                    ("command.timed_out", DiagnosticLevel::Warn)
                } else if error_message.contains("turn cancelled") {
                    ("command.cancelled", DiagnosticLevel::Info)
                } else {
                    ("command.failed", DiagnosticLevel::Error)
                };
                write_command_diagnostic(
                    &diagnostic_logger,
                    level,
                    event_name,
                    &error_message,
                    [
                        ("shell", shell_label),
                        ("timeout_secs", timeout_secs),
                        ("command", command_text),
                    ],
                )?;
                return Err(error);
            }
        };
        let finished_at = SystemTime::now();
        let duration = finished_at.duration_since(started_at).unwrap_or_default();
        write_command_diagnostic(
            &diagnostic_logger,
            DiagnosticLevel::Info,
            "command.finished",
            "command finished",
            [
                ("shell", shell_label),
                ("timeout_secs", timeout_secs),
                ("command", command_text),
                ("exit_code", output.status.code().unwrap_or(-1).to_string()),
                ("duration_ms", duration.as_millis().to_string()),
            ],
        )?;

        Ok(CommandExecution {
            command: request.command,
            working_directory: self.working_directory.clone(),
            shell: selected_shell.kind,
            exit_code: output.status.code().unwrap_or(-1),
            stdout: decode_command_output(&output.stdout),
            stderr: decode_command_output(&output.stderr),
            started_at,
            finished_at,
            duration,
        })
    }

    pub async fn run_command(
        &mut self,
        request: CommandRequest,
        cancellation: &TurnCancellation,
    ) -> Result<CommandExecution> {
        self.run_command_with_output(request, cancellation, |_| Ok(()))
            .await
    }

    pub fn persisted_state(&self) -> PersistedTaskState {
        PersistedTaskState {
            active_agent: self.active_agent.clone(),
            timeline: None,
            notes: self.persisted_notes(),
            open_files: self.open_files.clone(),
            hints: self.hints.clone(),
            last_active: SystemTime::now(),
        }
    }

    pub fn flush_memory_usage(&mut self) -> Result<()> {
        self.memory_manager.flush_pending_usage_updates()
    }

    pub fn available_shells(&self) -> &[AvailableShell] {
        &self.available_shells
    }

    pub fn skills(&self) -> &[crate::skills::SkillEntry] {
        &self.skills
    }

    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub fn runtime_open_file_snapshots(&self) -> IndexMap<PathBuf, FileSnapshot> {
        self.open_file_snapshots()
    }

    pub(crate) fn open_file_snapshots(&self) -> IndexMap<PathBuf, FileSnapshot> {
        self.watcher.store().snapshots()
    }

    pub(crate) fn session_status(&self) -> SessionStatus {
        SessionStatus {
            workspace_root: clean_path(self.working_directory.clone()),
            platform: platform_label().to_string(),
            shell: self
                .available_shells
                .first()
                .map(|shell| shell.kind.label().to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            available_shells: self
                .available_shells
                .iter()
                .map(|shell| shell.kind.label().to_string())
                .collect(),
            workspace_entries: workspace_entries(&self.working_directory),
        }
    }

    pub(crate) fn prune_expired_hints(&mut self) {
        let now = SystemTime::now();
        self.hints.retain(|hint| !hint.is_expired_at(now));
    }

    pub(crate) fn tick_hints(&mut self) {
        for hint in &mut self.hints {
            hint.tick_turn();
        }
        self.prune_expired_hints();
    }

    fn build_memory_query(&self) -> crate::memory::MemoryQuery {
        crate::memory::MemoryQuery {
            task_name: Some(self.task_name.clone()),
            latest_user_message: self
                .history
                .turns
                .iter()
                .rev()
                .find(|turn| matches!(turn.role, Role::User))
                .map(|turn| join_text_blocks(&turn.content)),
            open_file_paths: self
                .open_file_snapshots_for_active_agent()
                .keys()
                .cloned()
                .collect(),
            recent_assistant_messages: self
                .history
                .turns
                .iter()
                .rev()
                .filter(|turn| matches!(turn.role, Role::Assistant))
                .take(2)
                .map(|turn| join_text_blocks(&turn.content))
                .collect(),
            active_agent: self.active_agent.clone(),
            context_pressure_percent: self
                .estimate_context_pressure(DEFAULT_CONTEXT_WINDOW_TOKENS)
                .map(|pressure| pressure.used_percent),
        }
    }
}

fn write_command_diagnostic<I, K, V>(
    diagnostic_logger: &DiagnosticLogger,
    level: DiagnosticLevel,
    event: &str,
    message: &str,
    fields: I,
) -> Result<()>
where
    I: IntoIterator<Item = (K, V)>,
    K: Into<String>,
    V: Into<String>,
{
    diagnostic_logger.write_backend(DiagnosticRecord {
        timestamp_ms: now_timestamp_ms(),
        level,
        channel: DiagnosticChannel::Backend,
        scope: "command-execution".to_string(),
        event: event.to_string(),
        message: message.to_string(),
        fields: fields
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use tokio::runtime::Runtime;
    use tokio::time::sleep;

    use super::*;

    use crate::context::ContentBlock;
    use crate::memory::{MemorizeRequest, MemoryManager};
    use crate::storage::{TaskRecord, TaskTitleSource};
    #[test]
    fn run_command_writes_finished_diagnostic_log() {
        let runtime = Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let workspace = temp_workspace("run-command-finished");
            let mut session = AgentSession::new(
                AgentConfig::default(),
                "default",
                ConversationHistory::default(),
                [],
                workspace.clone(),
            )
            .expect("create session");

            let (shell, command) = if cfg!(windows) {
                (CommandShell::PowerShell, "Write-Output 'ok'".to_string())
            } else {
                (CommandShell::Sh, "printf 'ok\\n'".to_string())
            };

            session
                .run_command(
                    CommandRequest {
                        command,
                        shell,
                        timeout: Duration::from_secs(5),
                    },
                    TurnCancellation::never(),
                )
                .await
                .expect("command should succeed");

            let backend_log = fs::read_to_string(
                workspace
                    .join(".march")
                    .join("diagnostics")
                    .join("backend.log"),
            )
            .expect("read backend log");

            assert!(backend_log.contains("command.started command started"));
            assert!(backend_log.contains("command.finished command finished"));
        });
    }

    #[test]
    fn run_command_writes_timeout_diagnostic_log() {
        let runtime = Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let workspace = temp_workspace("run-command-timeout");
            let mut session = AgentSession::new(
                AgentConfig::default(),
                "default",
                ConversationHistory::default(),
                [],
                workspace.clone(),
            )
            .expect("create session");

            let (shell, command) = if cfg!(windows) {
                (
                    CommandShell::PowerShell,
                    "Start-Sleep -Seconds 5".to_string(),
                )
            } else {
                (CommandShell::Sh, "sleep 5".to_string())
            };

            let error = session
                .run_command(
                    CommandRequest {
                        command,
                        shell,
                        timeout: Duration::from_secs(1),
                    },
                    TurnCancellation::never(),
                )
                .await
                .expect_err("command should time out");

            assert!(error.to_string().contains("command timed out after 1.000s"));

            let backend_log = fs::read_to_string(
                workspace
                    .join(".march")
                    .join("diagnostics")
                    .join("backend.log"),
            )
            .expect("read backend log");

            assert!(backend_log.contains("command.started command started"));
            assert!(backend_log.contains("command.timed_out"));
        });
    }

    #[test]
    fn run_command_writes_cancelled_diagnostic_log() {
        let runtime = Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let workspace = temp_workspace("run-command-cancelled");
            let mut session = AgentSession::new(
                AgentConfig::default(),
                "default",
                ConversationHistory::default(),
                [],
                workspace.clone(),
            )
            .expect("create session");

            let cancellation = Arc::new(TurnCancellation::new());
            let (shell, command) = if cfg!(windows) {
                (
                    CommandShell::PowerShell,
                    "Start-Sleep -Seconds 5".to_string(),
                )
            } else {
                (CommandShell::Sh, "sleep 5".to_string())
            };

            let cancel_handle = Arc::clone(&cancellation);
            tokio::spawn(async move {
                sleep(Duration::from_millis(150)).await;
                cancel_handle.cancel();
            });

            let error = session
                .run_command(
                    CommandRequest {
                        command,
                        shell,
                        timeout: Duration::from_secs(5),
                    },
                    cancellation.as_ref(),
                )
                .await
                .expect_err("command should be cancelled");

            assert!(error.to_string().contains("turn cancelled"));

            let backend_log = fs::read_to_string(
                workspace
                    .join(".march")
                    .join("diagnostics")
                    .join("backend.log"),
            )
            .expect("read backend log");

            assert!(backend_log.contains("command.started command started"));
            assert!(backend_log.contains("command.cancelled"));
        });
    }

    fn temp_workspace(prefix: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("march-session-{prefix}-{unique}"));
        fs::create_dir_all(root.join(".march")).expect("create .march");
        root
    }

    #[test]
    fn restored_session_can_reuse_last_memory_index_for_runtime_snapshot() {
        let workspace = temp_workspace("restore-last-memory-index");
        let mut manager = MemoryManager::load(&workspace).expect("load memory manager");
        manager
            .memorize(
                MemorizeRequest {
                    id: "memory-panel-visibility".to_string(),
                    memory_type: "fact".to_string(),
                    topic: "memory".to_string(),
                    title: "Memory panel should retain activated entries".to_string(),
                    content: "Activated memories should remain visible after the reply finishes."
                        .to_string(),
                    tags: vec![
                        "memory".to_string(),
                        "panel".to_string(),
                        "visibility".to_string(),
                    ],
                    scope: None,
                    level: Some("project".to_string()),
                },
                MARCH_AGENT_NAME,
            )
            .expect("store test memory");

        let mut session = AgentSession::new(
            AgentConfig::default(),
            "memory panel visibility",
            ConversationHistory::default(),
            [],
            workspace.clone(),
        )
        .expect("create session");
        session.add_user_turn(vec![ContentBlock::text(
            "memory panel should keep activated memories visible",
        )]);
        session.build_context();

        let last_memory_index = session.last_memory_index();
        assert!(
            last_memory_index
                .as_ref()
                .is_some_and(|view| !view.entries.is_empty()),
            "build_context should populate last_memory_index for matching memories"
        );

        let persisted_state = session.persisted_state();
        let persisted = PersistedTask {
            task: TaskRecord {
                id: 1,
                name: "memory panel visibility".to_string(),
                title_source: TaskTitleSource::Default,
                title_locked: false,
                working_directory: workspace,
                selected_model_config_id: None,
                selected_model: None,
                model_temperature: None,
                model_top_p: None,
                model_presence_penalty: None,
                model_frequency_penalty: None,
                model_max_output_tokens: None,
                active_agent: MARCH_AGENT_NAME.to_string(),
                last_event_seq: 0,
                created_at: SystemTime::now(),
                last_active: SystemTime::now(),
            },
            active_agent: MARCH_AGENT_NAME.to_string(),
            timeline: Vec::new(),
            notes: persisted_state.notes,
            open_files: persisted_state.open_files,
            hints: persisted_state.hints,
        };

        let restored_without_memory =
            AgentSession::restore(AgentConfig::default(), persisted.clone())
                .expect("restore session");
        assert!(
            restored_without_memory
                .ui_runtime_snapshot(DEFAULT_CONTEXT_WINDOW_TOKENS)
                .memories
                .is_empty(),
            "restored sessions start without the transient last_memory_index"
        );

        let mut restored_with_memory =
            AgentSession::restore(AgentConfig::default(), persisted).expect("restore session");
        restored_with_memory.restore_last_memory_index(last_memory_index);
        let runtime = restored_with_memory.ui_runtime_snapshot(DEFAULT_CONTEXT_WINDOW_TOKENS);
        assert!(
            runtime
                .memories
                .iter()
                .any(|memory| memory.id == "p:memory-panel-visibility"),
            "restored runtime should preserve the target matched memory entry"
        );
    }
}
