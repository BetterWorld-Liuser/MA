use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::agent::{
    AgentConfig, AgentProgressEvent, AgentSession, AgentStatusPhase, AgentToolStatus, DebugRound,
    DebugToolCall,
};
use crate::context::{
    ContextPressure, ConversationHistory, DisplayTurn, FileSnapshot, Hint, ModifiedBy, Role,
    SystemStatus, ToolSummary,
};
use crate::provider::{OpenAiCompatibleClient, OpenAiCompatibleConfig, fallback_task_title};
use crate::storage::{
    PersistedOpenFile, PersistedTask, PersistedTaskState, TaskRecord, TaskTitleSource,
};

const DEFAULT_TASK_NAME: &str = "默认任务";
const UI_MAX_RECENT_TURNS: usize = 4;

/// Tauri commands 的输入对象保持得尽量薄，避免把 UI 状态设计成另一套持久化模型。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiCreateTaskRequest {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSelectTaskRequest {
    pub task_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDeleteTaskRequest {
    pub task_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSendMessageRequest {
    pub task_id: Option<i64>,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiUpsertNoteRequest {
    pub task_id: Option<i64>,
    pub note_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiDeleteNoteRequest {
    pub task_id: Option<i64>,
    pub note_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiToggleOpenFileLockRequest {
    pub task_id: Option<i64>,
    pub path: PathBuf,
    pub locked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiCloseOpenFileRequest {
    pub task_id: Option<i64>,
    pub path: PathBuf,
}

/// 面向 UI 的工作区快照。
/// 这一层把 storage 和 session 的信息整理成前端可直接消费的 JSON 结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiWorkspaceSnapshot {
    pub workspace_path: PathBuf,
    pub database_path: PathBuf,
    pub tasks: Vec<UiTaskSummary>,
    pub active_task: Option<UiTaskSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTaskSummary {
    pub id: i64,
    pub name: String,
    pub title_source: String,
    pub title_locked: bool,
    pub created_at: i64,
    pub last_active: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTaskSnapshot {
    pub task: UiTaskSummary,
    pub history: Vec<UiTurnView>,
    pub notes: Vec<UiNoteView>,
    pub open_files: Vec<UiOpenFileView>,
    pub hints: Vec<UiHintView>,
    pub system_status: UiSystemStatusView,
    pub runtime: Option<UiRuntimeSnapshot>,
    pub debug_trace: Option<UiDebugTraceView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiRuntimeSnapshot {
    pub working_directory: PathBuf,
    pub available_shells: Vec<UiShellView>,
    pub open_files: Vec<UiFileSnapshotView>,
    pub system_status: UiSystemStatusView,
    pub context_usage: UiContextUsageView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiDebugTraceView {
    pub rounds: Vec<UiDebugRoundView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiDebugRoundView {
    pub iteration: usize,
    pub context_preview: String,
    pub provider_request_json: String,
    pub provider_response_raw: String,
    pub tool_calls: Vec<UiDebugToolCallView>,
    pub tool_results: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiDebugToolCallView {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiAgentProgressEvent {
    TurnStarted {
        task_id: i64,
        turn_id: String,
        user_message: String,
    },
    Status {
        task_id: i64,
        turn_id: String,
        phase: UiAgentStatusPhase,
        label: String,
    },
    ToolStarted {
        task_id: i64,
        turn_id: String,
        tool_call_id: String,
        tool_name: String,
        summary: String,
    },
    ToolFinished {
        task_id: i64,
        turn_id: String,
        tool_call_id: String,
        status: UiAgentToolStatus,
        summary: String,
        preview: Option<String>,
    },
    ReplyPreview {
        task_id: i64,
        turn_id: String,
        message: String,
    },
    Reply {
        task_id: i64,
        turn_id: String,
        task: UiTaskSnapshot,
        wait: bool,
    },
    RoundComplete {
        task_id: i64,
        turn_id: String,
        task: UiTaskSnapshot,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAgentStatusPhase {
    BuildingContext,
    WaitingModel,
    RunningTool,
    Streaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiAgentToolStatus {
    Success,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiShellView {
    pub kind: String,
    pub program: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiTurnView {
    pub role: UiRoleView,
    pub content: String,
    pub tool_summaries: Vec<UiToolSummaryView>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiRoleView {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiToolSummaryView {
    pub name: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiNoteView {
    pub id: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiHintView {
    pub content: String,
    pub expires_at: Option<i64>,
    pub turns_remaining: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiOpenFileView {
    pub path: PathBuf,
    pub locked: bool,
    pub snapshot: Option<UiFileSnapshotView>,
}

impl UiOpenFileView {
    pub fn from_persisted(open_file: PersistedOpenFile) -> Self {
        open_file.into()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiFileSnapshotView {
    Available {
        path: PathBuf,
        content: String,
        last_modified_at: i64,
        modified_by: UiModifiedByView,
    },
    Deleted {
        path: PathBuf,
        last_seen_at: i64,
        modified_by: UiModifiedByView,
    },
    Moved {
        path: PathBuf,
        new_path: PathBuf,
        last_seen_at: i64,
        modified_by: UiModifiedByView,
    },
}

impl UiFileSnapshotView {
    pub fn path(&self) -> &Path {
        match self {
            Self::Available { path, .. }
            | Self::Deleted { path, .. }
            | Self::Moved { path, .. } => path.as_path(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiModifiedByView {
    Agent,
    User,
    External,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiSystemStatusView {
    pub locked_files: Vec<PathBuf>,
    pub context_pressure: Option<UiContextPressureView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiContextPressureView {
    pub used_percent: u8,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiContextUsageView {
    pub used_bytes: usize,
    pub budget_bytes: usize,
    pub used_percent: u8,
    pub sections: Vec<UiContextUsageSectionView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiContextUsageSectionView {
    pub name: String,
    pub bytes: usize,
}

pub struct UiAppBackend {
    workspace_path: PathBuf,
    storage: crate::storage::MaStorage,
}

impl UiAppBackend {
    pub fn open(workspace_path: impl Into<PathBuf>) -> Result<Self> {
        let workspace_path = workspace_path.into();
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

        Ok(self.storage.create_task(DEFAULT_TASK_NAME)?.id)
    }

    pub fn create_task(&mut self, name: impl AsRef<str>) -> Result<TaskRecord> {
        let name = name.as_ref().trim();
        let (name, title_source, title_locked) = if name.is_empty() {
            (DEFAULT_TASK_NAME, TaskTitleSource::Default, false)
        } else {
            (name, TaskTitleSource::Manual, true)
        };

        let task = self
            .storage
            .create_task_with_metadata(name, title_source, title_locked)?;
        let session = AgentSession::new(ui_agent_config(), ConversationHistory::default(), [])?;
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
        let runtime = self
            .load_session(active_task_id)
            .ok()
            .map(|session| session.ui_runtime_snapshot());
        let active_task = Some({
            let snapshot = UiTaskSnapshot::from_persisted(persisted);
            if let Some(runtime) = runtime {
                snapshot.with_runtime(&runtime)
            } else {
                snapshot
            }
        });

        Ok(UiWorkspaceSnapshot {
            workspace_path: self.workspace_path.clone(),
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
        runtime: &UiRuntimeSnapshot,
    ) -> Result<UiTaskSnapshot> {
        self.task_snapshot(task_id)
            .map(|snapshot| snapshot.with_runtime(runtime))
    }

    pub async fn handle_send_message(
        &mut self,
        request: UiSendMessageRequest,
    ) -> Result<UiWorkspaceSnapshot> {
        self.handle_send_message_with_progress(request, |_| Ok(()))
            .await
    }

    pub async fn handle_send_message_with_progress<F>(
        &mut self,
        request: UiSendMessageRequest,
        mut on_progress: F,
    ) -> Result<UiWorkspaceSnapshot>
    where
        F: FnMut(UiAgentProgressEvent) -> Result<()>,
    {
        let task_id = self.resolve_or_create_task_id(request.task_id)?;
        let content = request.content.trim();
        if content.is_empty() {
            bail!("message cannot be empty");
        }

        let persisted_before = self.storage.load_task(task_id)?;
        let should_auto_title = should_auto_title(&persisted_before, content);
        let provider = OpenAiCompatibleClient::new(OpenAiCompatibleConfig::from_env()?);
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
            user_message: content.to_string(),
        })?;
        let result = session
            .handle_user_message_with_events(&provider, content.to_string(), |session, event| {
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
                    AgentProgressEvent::ReplyPreview { message } => {
                        on_progress(UiAgentProgressEvent::ReplyPreview {
                            task_id,
                            turn_id: turn_id.clone(),
                            message,
                        })?;
                    }
                    AgentProgressEvent::Reply(reply) => {
                        let task = Self::live_task_snapshot(
                            progress_task.clone(),
                            session,
                            &progress_rounds,
                        )?;
                        on_progress(UiAgentProgressEvent::Reply {
                            task_id,
                            turn_id: turn_id.clone(),
                            task,
                            wait: reply.wait,
                        })?;
                    }
                    AgentProgressEvent::RoundCompleted(round) => {
                        progress_rounds.push(round);
                        let task = Self::live_task_snapshot(
                            progress_task.clone(),
                            session,
                            &progress_rounds,
                        )?;
                        on_progress(UiAgentProgressEvent::RoundComplete {
                            task_id,
                            turn_id: turn_id.clone(),
                            task,
                        })?;
                    }
                }
                Ok(())
            })
            .await?;
        let runtime = session.ui_runtime_snapshot();
        self.save_session(task_id, &session)?;
        if should_auto_title {
            let suggested_title = provider
                .suggest_task_title(content)
                .await
                .ok()
                .flatten()
                .or_else(|| fallback_task_title(content));
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
    ) -> Result<UiTaskSnapshot> {
        let PersistedTaskState {
            history,
            notes,
            open_files,
            hints,
            ..
        } = session.persisted_state();
        let runtime = session.ui_runtime_snapshot();

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
}

fn ui_agent_config() -> AgentConfig {
    AgentConfig {
        max_recent_turns: UI_MAX_RECENT_TURNS,
        ..AgentConfig::default()
    }
}

impl UiTaskSnapshot {
    pub fn from_persisted(task: PersistedTask) -> Self {
        let PersistedTask {
            task,
            history,
            notes,
            open_files,
            hints,
        } = task;

        Self {
            task: UiTaskSummary::from(task),
            history: history.turns.into_iter().map(UiTurnView::from).collect(),
            notes: notes
                .into_iter()
                .map(|(id, note)| UiNoteView {
                    id,
                    content: note.content,
                })
                .collect(),
            open_files: open_files.into_iter().map(UiOpenFileView::from).collect(),
            hints: hints.into_iter().map(UiHintView::from).collect(),
            system_status: UiSystemStatusView {
                locked_files: Vec::new(),
                context_pressure: None,
            },
            runtime: None,
            debug_trace: None,
        }
    }

    pub fn with_runtime(mut self, runtime: &UiRuntimeSnapshot) -> Self {
        let runtime_snapshot_map = runtime
            .open_files
            .iter()
            .map(|snapshot| (snapshot.path().to_path_buf(), snapshot.clone()))
            .collect::<IndexMap<_, _>>();

        for open_file in &mut self.open_files {
            if let Some(snapshot) = runtime_snapshot_map.get(&open_file.path) {
                open_file.snapshot = Some(snapshot.clone());
            }
        }

        self.system_status = runtime.system_status.clone();
        self.runtime = Some(runtime.clone());
        self
    }

    pub fn with_debug_trace(mut self, debug_trace: UiDebugTraceView) -> Self {
        self.debug_trace = Some(debug_trace);
        self
    }
}

impl UiRuntimeSnapshot {
    pub fn new(
        working_directory: PathBuf,
        available_shells: Vec<UiShellView>,
        open_files: Vec<UiFileSnapshotView>,
        system_status: UiSystemStatusView,
        context_usage: UiContextUsageView,
    ) -> Self {
        Self {
            working_directory,
            available_shells,
            open_files,
            system_status,
            context_usage,
        }
    }
}

impl UiDebugTraceView {
    pub fn from_rounds(rounds: &[DebugRound]) -> Self {
        Self {
            rounds: rounds.iter().cloned().map(UiDebugRoundView::from).collect(),
        }
    }
}

impl UiTaskSummary {
    pub fn from_task(task: TaskRecord) -> Self {
        task.into()
    }
}

impl From<TaskRecord> for UiTaskSummary {
    fn from(task: TaskRecord) -> Self {
        Self {
            id: task.id,
            name: task.name,
            title_source: task.title_source.as_db_value().to_string(),
            title_locked: task.title_locked,
            created_at: system_time_to_unix(task.created_at),
            last_active: system_time_to_unix(task.last_active),
        }
    }
}

fn should_auto_title(task: &PersistedTask, user_message: &str) -> bool {
    task.task.title_source == TaskTitleSource::Default
        && !task.task.title_locked
        && task.history.turns.is_empty()
        && user_message.trim().chars().count() >= 4
}

impl From<DisplayTurn> for UiTurnView {
    fn from(turn: DisplayTurn) -> Self {
        Self {
            role: UiRoleView::from(turn.role),
            content: turn.content,
            tool_summaries: turn
                .tool_calls
                .into_iter()
                .map(UiToolSummaryView::from)
                .collect(),
            timestamp: system_time_to_unix(turn.timestamp),
        }
    }
}

impl From<Role> for UiRoleView {
    fn from(role: Role) -> Self {
        match role {
            Role::System => Self::System,
            Role::User => Self::User,
            Role::Assistant => Self::Assistant,
            Role::Tool => Self::Tool,
        }
    }
}

impl From<ToolSummary> for UiToolSummaryView {
    fn from(summary: ToolSummary) -> Self {
        Self {
            name: summary.name,
            summary: summary.summary,
        }
    }
}

impl From<Hint> for UiHintView {
    fn from(hint: Hint) -> Self {
        Self {
            content: hint.content,
            expires_at: hint.expires_at.map(system_time_to_unix),
            turns_remaining: hint.turns_remaining,
        }
    }
}

impl From<DebugRound> for UiDebugRoundView {
    fn from(round: DebugRound) -> Self {
        Self {
            iteration: round.iteration,
            context_preview: round.context_preview,
            provider_request_json: pretty_json_or_original(&round.provider_request_json),
            provider_response_raw: pretty_json_or_original(&round.provider_raw_response),
            tool_calls: round
                .tool_calls
                .into_iter()
                .map(UiDebugToolCallView::from)
                .collect(),
            tool_results: round.tool_results,
        }
    }
}

impl From<DebugToolCall> for UiDebugToolCallView {
    fn from(tool_call: DebugToolCall) -> Self {
        Self {
            id: tool_call.id,
            name: tool_call.name,
            arguments_json: tool_call.arguments_json,
        }
    }
}

impl From<PersistedOpenFile> for UiOpenFileView {
    fn from(open_file: PersistedOpenFile) -> Self {
        Self {
            path: open_file.path,
            locked: open_file.locked,
            snapshot: None,
        }
    }
}

impl From<FileSnapshot> for UiFileSnapshotView {
    fn from(snapshot: FileSnapshot) -> Self {
        match snapshot {
            FileSnapshot::Available {
                path,
                content,
                last_modified,
                last_modified_by,
            } => Self::Available {
                path,
                content,
                last_modified_at: system_time_to_unix(last_modified),
                modified_by: last_modified_by.into(),
            },
            FileSnapshot::Deleted {
                path,
                last_seen_at,
                last_modified_by,
            } => Self::Deleted {
                path,
                last_seen_at: system_time_to_unix(last_seen_at),
                modified_by: last_modified_by.into(),
            },
            FileSnapshot::Moved {
                path,
                new_path,
                last_seen_at,
                last_modified_by,
            } => Self::Moved {
                path,
                new_path,
                last_seen_at: system_time_to_unix(last_seen_at),
                modified_by: last_modified_by.into(),
            },
        }
    }
}

impl From<ModifiedBy> for UiModifiedByView {
    fn from(value: ModifiedBy) -> Self {
        match value {
            ModifiedBy::Agent => Self::Agent,
            ModifiedBy::User => Self::User,
            ModifiedBy::External => Self::External,
            ModifiedBy::Unknown => Self::Unknown,
        }
    }
}

impl From<ContextPressure> for UiContextPressureView {
    fn from(value: ContextPressure) -> Self {
        Self {
            used_percent: value.used_percent,
            message: value.message,
        }
    }
}

impl UiSystemStatusView {
    pub fn from_system_status(status: SystemStatus) -> Self {
        Self {
            locked_files: status.locked_files,
            context_pressure: status.context_pressure.map(Into::into),
        }
    }
}

impl UiContextUsageView {
    pub fn new(
        used_bytes: usize,
        budget_bytes: usize,
        sections: Vec<UiContextUsageSectionView>,
    ) -> Self {
        let used_percent = if budget_bytes == 0 {
            0
        } else {
            (((used_bytes as f64 / budget_bytes as f64) * 100.0).round()).clamp(0.0, 100.0) as u8
        };

        Self {
            used_bytes,
            budget_bytes,
            used_percent,
            sections,
        }
    }
}

impl UiContextUsageSectionView {
    pub fn new(name: impl Into<String>, bytes: usize) -> Self {
        Self {
            name: name.into(),
            bytes,
        }
    }
}

impl From<AgentStatusPhase> for UiAgentStatusPhase {
    fn from(value: AgentStatusPhase) -> Self {
        match value {
            AgentStatusPhase::BuildingContext => Self::BuildingContext,
            AgentStatusPhase::WaitingModel => Self::WaitingModel,
            AgentStatusPhase::RunningTool => Self::RunningTool,
            AgentStatusPhase::Streaming => Self::Streaming,
        }
    }
}

impl From<AgentToolStatus> for UiAgentToolStatus {
    fn from(value: AgentToolStatus) -> Self {
        match value {
            AgentToolStatus::Success => Self::Success,
            AgentToolStatus::Error => Self::Error,
        }
    }
}

impl UiRuntimeSnapshot {
    pub fn from_session(session: &AgentSession) -> Self {
        session.ui_runtime_snapshot()
    }
}

impl UiAppBackend {
    pub fn workspace_path(&self) -> &Path {
        &self.workspace_path
    }
}

fn pretty_json_or_original(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| text.to_string())
}

fn system_time_to_unix(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}
