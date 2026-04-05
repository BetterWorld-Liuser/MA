use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Result, bail};
use indexmap::IndexMap;

use crate::agents::{AgentProfile, MARCH_AGENT_NAME, SHARED_SCOPE, load_agent_profiles};
use crate::context::{
    AgentContext, AgentContextBuilder, ContentBlock, ContextBuildConfig, ContextPressure,
    ConversationHistory, DisplayTurn, FileSnapshot, Hint, Injection, NoteEntry, Role,
    SessionStatus, SystemStatus, ToolSummary, join_text_blocks,
};
use crate::paths::clean_path;
use crate::storage::{PersistedNote, PersistedOpenFile, PersistedTask, PersistedTaskState};
use crate::tools::ToolRuntime;
use crate::ui::{
    UiContextPressureView, UiContextUsageSectionView, UiContextUsageView, UiFileSnapshotView,
    UiRuntimeSnapshot, UiShellView, UiSkillView, UiSystemStatusView,
};
use crate::watcher::FileWatcherService;

mod editing;
mod prompting;
mod runner;
mod shells;
mod tool_calls;

#[cfg(test)]
use prompting::append_assistant_tool_call_message;
use prompting::normalize_open_files_for_workspace;
pub(crate) use prompting::{base_instructions, default_march_prompt, default_system_core};
use prompting::{load_skills_for_workspace, render_prompt, upsert_injection};
pub use runner::is_turn_cancelled_error;
use shells::decode_command_output;
pub use shells::{AvailableShell, CommandShell};
use shells::{detect_available_shells, platform_label, shell_command, workspace_entries};

const AGENTS_FILENAME: &str = "AGENTS.md";
const TURN_CANCELLED_ERROR_MESSAGE: &str = "turn cancelled";

pub struct AgentSession {
    config: AgentConfig,
    watcher: FileWatcherService,
    agent_profiles: IndexMap<String, AgentProfile>,
    active_agent: String,
    history: ConversationHistory,
    notes: IndexMap<String, IndexMap<String, NoteEntry>>,
    open_files: Vec<PersistedOpenFile>,
    hints: Vec<Hint>,
    injections: Vec<Injection>,
    skills: Vec<crate::skills::SkillEntry>,
    available_shells: Vec<AvailableShell>,
    working_directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub system_core: String,
    pub max_recent_turns: usize,
}

pub const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 128_000;

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_core: default_system_core(),
            max_recent_turns: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandRequest {
    pub command: String,
    pub shell: CommandShell,
}

#[derive(Debug, Clone)]
pub struct CommandExecution {
    pub command: String,
    pub working_directory: PathBuf,
    pub shell: CommandShell,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
}

#[derive(Debug, Clone)]
pub struct AgentRunResult {
    pub final_messages: Vec<FinalAssistantMessage>,
    pub debug_rounds: Vec<DebugRound>,
}

#[derive(Debug, Clone)]
pub enum AgentProgressEvent {
    Status {
        agent: String,
        phase: AgentStatusPhase,
        label: String,
    },
    ToolStarted {
        tool_call_id: String,
        tool_name: String,
        summary: String,
    },
    ToolFinished {
        tool_call_id: String,
        status: AgentToolStatus,
        summary: String,
        preview: Option<String>,
    },
    AssistantTextPreview {
        agent: String,
        message: String,
    },
    FinalAssistantMessage(FinalAssistantMessage),
    RoundCompleted(DebugRound),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatusPhase {
    BuildingContext,
    WaitingModel,
    RunningTool,
    Streaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentToolStatus {
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct FinalAssistantMessage {
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct DebugRound {
    pub iteration: usize,
    pub context_preview: String,
    pub provider_request_json: String,
    pub provider_raw_response: String,
    pub tool_calls: Vec<DebugToolCall>,
    pub tool_results: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DebugToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone)]
struct ToolOutcome {
    result_text: String,
    summary: Option<ToolSummary>,
}

impl AgentSession {
    pub fn new(
        config: AgentConfig,
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
        Self::create(
            config,
            task.history,
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
            active_agent,
            history,
            notes: notes_by_scope(notes),
            open_files,
            hints,
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
        let context = AgentContextBuilder::new(self.system_core_for_active_agent())
            .with_config(ContextBuildConfig {
                max_recent_chat_turns: self.config.max_recent_turns,
                max_recent_chat_image_turns: 4,
            })
            .injections(self.injections.clone())
            .tools(tools)
            .notes(notes)
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

    pub fn run_command(&mut self, request: CommandRequest) -> Result<CommandExecution> {
        let started_at = SystemTime::now();
        let selected_shell = self.resolve_shell(request.shell)?;
        let tracked_paths = self
            .open_file_snapshots()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let _guard = self
            .watcher
            .store()
            .begin_agent_write(tracked_paths.clone())?;
        let output = shell_command(
            selected_shell.kind,
            &selected_shell.program,
            &request.command,
            &self.working_directory,
        )?;
        let finished_at = SystemTime::now();

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

        Ok(CommandExecution {
            command: request.command,
            working_directory: self.working_directory.clone(),
            shell: selected_shell.kind,
            exit_code: output.status.code().unwrap_or(-1),
            stdout: decode_command_output(&output.stdout),
            stderr: decode_command_output(&output.stderr),
            started_at,
            finished_at,
        })
    }

    pub fn persisted_state(&self) -> PersistedTaskState {
        PersistedTaskState {
            active_agent: self.active_agent.clone(),
            history: self.history.clone(),
            notes: self.persisted_notes(),
            open_files: self.open_files.clone(),
            hints: self.hints.clone(),
            last_active: SystemTime::now(),
        }
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

    pub fn ui_system_status(&self, context_budget_tokens: usize) -> UiSystemStatusView {
        UiSystemStatusView {
            locked_files: self.locked_files_for_active_agent(),
            context_pressure: self.estimate_context_pressure(context_budget_tokens).map(
                |pressure| UiContextPressureView {
                    used_percent: pressure.used_percent,
                    message: pressure.message,
                },
            ),
        }
    }

    pub fn ui_context_usage(&self, context_budget_tokens: usize) -> UiContextUsageView {
        let sections = vec![
            UiContextUsageSectionView::new(
                "system",
                estimate_token_count(&self.system_core_for_active_agent()),
            ),
            UiContextUsageSectionView::new(
                "injections",
                self.injections
                    .iter()
                    .map(|injection| estimate_token_count(&injection.content))
                    .sum(),
            ),
            UiContextUsageSectionView::new(
                "notes",
                self.notes_for_active_agent()
                    .values()
                    .map(|note| estimate_token_count(&note.content))
                    .sum(),
            ),
            UiContextUsageSectionView::new(
                "chat",
                self.history
                    .turns
                    .iter()
                    .map(|turn| estimate_content_blocks_token_count(&turn.content))
                    .sum(),
            ),
            UiContextUsageSectionView::new(
                "files",
                self.open_file_snapshots_for_active_agent()
                    .values()
                    .map(|snapshot| match snapshot {
                        FileSnapshot::Available { content, .. } => estimate_token_count(content),
                        FileSnapshot::Deleted { .. } | FileSnapshot::Moved { .. } => 8,
                    })
                    .sum(),
            ),
        ];

        let used_tokens = sections.iter().map(|section| section.tokens).sum();
        UiContextUsageView::new(used_tokens, context_budget_tokens, sections)
    }

    pub fn ui_runtime_snapshot(&self, context_budget_tokens: usize) -> UiRuntimeSnapshot {
        let open_file_snapshots = self.open_file_snapshots_for_active_agent();
        let available_shells = self
            .available_shells
            .iter()
            .map(|shell| UiShellView {
                kind: shell.kind.label().to_string(),
                program: shell.program.clone(),
            })
            .collect::<Vec<_>>();

        let open_files = open_file_snapshots
            .values()
            .cloned()
            .map(UiFileSnapshotView::from)
            .collect::<Vec<_>>();

        let skills = self
            .skills
            .iter()
            .map(|skill| UiSkillView {
                name: skill.name.clone(),
                path: clean_path(skill.path.clone()),
                description: skill.description.clone(),
                opened: open_file_snapshots.contains_key(&skill.path),
            })
            .collect::<Vec<_>>();

        UiRuntimeSnapshot::new(
            clean_path(self.working_directory.clone()),
            available_shells,
            open_files,
            skills,
            self.ui_system_status(context_budget_tokens),
            self.ui_context_usage(context_budget_tokens),
        )
    }

    pub(crate) fn open_file_snapshots(&self) -> IndexMap<PathBuf, FileSnapshot> {
        self.watcher.store().snapshots()
    }

    fn session_status(&self) -> SessionStatus {
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

    fn prune_expired_hints(&mut self) {
        let now = SystemTime::now();
        self.hints.retain(|hint| !hint.is_expired_at(now));
    }

    fn tick_hints(&mut self) {
        for hint in &mut self.hints {
            hint.tick_turn();
        }
        self.prune_expired_hints();
    }

    fn estimate_context_pressure(&self, context_budget_tokens: usize) -> Option<ContextPressure> {
        let budget = context_budget_tokens.max(1);
        let size = estimate_token_count(&self.system_core_for_active_agent())
            + self
                .injections
                .iter()
                .map(|injection| estimate_token_count(&injection.content))
                .sum::<usize>()
            + self
                .notes_for_active_agent()
                .values()
                .map(|note| estimate_token_count(&note.content))
                .sum::<usize>()
            + self
                .history
                .turns
                .iter()
                .map(|turn| estimate_content_blocks_token_count(&turn.content))
                .sum::<usize>()
            + self
                .open_file_snapshots_for_active_agent()
                .values()
                .map(|snapshot| match snapshot {
                    FileSnapshot::Available { content, .. } => estimate_token_count(content),
                    FileSnapshot::Deleted { .. } | FileSnapshot::Moved { .. } => 8,
                })
                .sum::<usize>();
        let used_percent = ((size as f32 / budget as f32) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8;
        (used_percent >= 75).then_some(ContextPressure {
            used_percent,
            message:
                "Estimated token usage is getting dense; consider closing files or removing stale notes."
                    .to_string(),
        })
    }
}

impl AgentSession {
    pub fn active_agent_name(&self) -> &str {
        &self.active_agent
    }

    pub fn set_active_agent(&mut self, name: impl Into<String>) {
        let name = name.into();
        if self.agent_profiles.contains_key(&name) {
            self.active_agent = name;
        }
    }

    pub fn has_agent(&self, name: &str) -> bool {
        self.agent_profiles.contains_key(name)
    }

    pub fn agent_profiles(&self) -> impl Iterator<Item = &AgentProfile> {
        self.agent_profiles.values()
    }

    pub fn active_agent_profile(&self) -> Option<&AgentProfile> {
        self.agent_profiles.get(self.active_agent_name())
    }

    pub fn display_name_for_agent(&self, name: &str) -> String {
        self.agent_profiles
            .get(name)
            .map(|profile| profile.display_name.clone())
            .unwrap_or_else(|| {
                if name.eq_ignore_ascii_case(MARCH_AGENT_NAME) {
                    "March".to_string()
                } else {
                    name.to_string()
                }
            })
    }

    pub fn refresh_agent_profiles(&mut self) -> Result<()> {
        let active_agent = self.active_agent.clone();
        self.agent_profiles = load_agent_profiles(&self.working_directory)?
            .into_iter()
            .map(|profile| (profile.name.clone(), profile))
            .collect::<IndexMap<_, _>>();
        if !self.agent_profiles.contains_key(&active_agent) {
            self.active_agent = MARCH_AGENT_NAME.to_string();
        }
        Ok(())
    }

    pub fn open_file_in_scope(
        &mut self,
        scope: impl Into<String>,
        path: impl Into<PathBuf>,
    ) -> Result<()> {
        let scope = scope.into();
        let path = self.resolve_path(path.into());
        self.watcher.watch_file(path.clone())?;
        if !self
            .open_files
            .iter()
            .any(|entry| entry.scope == scope && entry.path == path)
        {
            self.open_files.push(PersistedOpenFile {
                scope,
                path,
                locked: false,
            });
        }
        Ok(())
    }

    pub fn write_note_in_scope(
        &mut self,
        scope: impl Into<String>,
        id: impl Into<String>,
        content: impl Into<String>,
    ) {
        let scope = scope.into();
        self.notes
            .entry(scope)
            .or_default()
            .insert(id.into(), NoteEntry::new(content));
    }

    fn private_scope(&self) -> &str {
        &self.active_agent
    }

    fn notes_for_active_agent(&self) -> IndexMap<String, NoteEntry> {
        let mut merged = IndexMap::new();
        if let Some(shared) = self.notes.get(SHARED_SCOPE) {
            for (id, note) in shared {
                merged.insert(id.clone(), note.clone());
            }
        }
        if let Some(private) = self.notes.get(self.private_scope()) {
            for (id, note) in private {
                merged.insert(id.clone(), note.clone());
            }
        }
        merged
    }

    fn persisted_notes(&self) -> Vec<PersistedNote> {
        let mut persisted = Vec::new();
        for (scope, notes) in &self.notes {
            for (id, entry) in notes {
                persisted.push(PersistedNote {
                    scope: scope.clone(),
                    id: id.clone(),
                    entry: entry.clone(),
                });
            }
        }
        persisted.sort_by(|left, right| {
            (left.scope == SHARED_SCOPE)
                .cmp(&(right.scope == SHARED_SCOPE))
                .reverse()
                .then_with(|| left.scope.cmp(&right.scope))
                .then_with(|| left.id.cmp(&right.id))
        });
        persisted
    }

    fn open_file_snapshots_for_active_agent(&self) -> IndexMap<PathBuf, FileSnapshot> {
        let all = self.open_file_snapshots();
        let mut filtered = IndexMap::new();
        for scope in [SHARED_SCOPE, self.private_scope()] {
            for entry in self.open_files.iter().filter(|entry| entry.scope == scope) {
                if filtered.contains_key(&entry.path) {
                    continue;
                }
                if let Some(snapshot) = all.get(&entry.path) {
                    filtered.insert(entry.path.clone(), snapshot.clone());
                }
            }
        }
        filtered
    }

    fn locked_files_for_active_agent(&self) -> Vec<PathBuf> {
        let mut locked = Vec::new();
        for scope in [SHARED_SCOPE, self.private_scope()] {
            for entry in self
                .open_files
                .iter()
                .filter(|entry| entry.scope == scope && entry.locked)
            {
                if !locked.iter().any(|path| path == &entry.path) {
                    locked.push(entry.path.clone());
                }
            }
        }
        clean_unique_paths(&locked)
    }

    /// Assembles the system core for the current active agent following the
    /// design in agents-teams.md:
    ///   [base instructions]  — shared foundation (tool rules, completion, handoff)
    ///   [agents roster]      — who's available + active_agent marker
    ///   [agent system_prompt] — the active agent's persona/behavior
    fn system_core_for_active_agent(&self) -> String {
        let Some(profile) = self.agent_profiles.get(self.private_scope()) else {
            return self.config.system_core.clone();
        };

        let mut output = String::new();

        // 1. Base instructions — shared by all agents
        output.push_str(base_instructions());

        // 2. Agents roster (contains active_agent marker)
        output.push_str("\n\n# Available Agents\n");
        output.push_str(&self.available_agents_for_prompt());

        // 3. Active agent's own system_prompt (persona / behavior)
        let role_prompt = profile.system_prompt.trim();
        if !role_prompt.is_empty() {
            output.push_str("\n\n# Agent Role\n");
            output.push_str(role_prompt);
        }

        output
    }

    fn available_agents_for_prompt(&self) -> String {
        let active = self.active_agent_name();
        self.agent_profiles
            .values()
            .map(|profile| {
                if profile.name == active {
                    format!(
                        "- {} | {} | {} (you)",
                        profile.name, profile.display_name, profile.description
                    )
                } else {
                    format!(
                        "- {} | {} | {}",
                        profile.name, profile.display_name, profile.description
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn notes_by_scope(notes: Vec<PersistedNote>) -> IndexMap<String, IndexMap<String, NoteEntry>> {
    let mut by_scope = IndexMap::new();
    for note in notes {
        by_scope
            .entry(note.scope)
            .or_insert_with(IndexMap::new)
            .insert(note.id, note.entry);
    }
    by_scope
}

fn estimate_token_count(text: &str) -> usize {
    let ascii_chars = text.chars().filter(|ch| ch.is_ascii()).count();
    let non_ascii_chars = text.chars().count().saturating_sub(ascii_chars);
    ascii_chars.div_ceil(4) + non_ascii_chars
}

fn estimate_content_blocks_token_count(content: &[ContentBlock]) -> usize {
    let text_tokens = estimate_token_count(&join_text_blocks(content));
    let image_tokens = content
        .iter()
        .map(ContentBlock::image_token_cost)
        .sum::<usize>();
    text_tokens + image_tokens
}

fn clean_unique_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths.iter().cloned().map(clean_path) {
        if !unique.iter().any(|existing| existing == &path) {
            unique.push(path);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::shells::resolve_shell_program_with;
    use super::{
        AGENTS_FILENAME, AgentConfig, AgentSession, CommandShell,
        append_assistant_tool_call_message, base_instructions, decode_command_output,
        default_march_prompt, default_system_core, normalize_open_files_for_workspace,
    };
    use crate::agents::{MARCH_AGENT_NAME, SHARED_SCOPE};
    use crate::context::{ConversationHistory, Hint};
    use crate::provider::{ProviderToolCall, RequestMessage};
    use crate::storage::{PersistedOpenFile, PersistedTask, TaskRecord, TaskTitleSource};

    #[test]
    fn base_instructions_include_tool_and_handoff_guidance() {
        let base = base_instructions();

        // Tool use rules
        assert!(
            base.contains(
                "you must inspect the workspace with one or more tools before giving a substantive answer"
            )
        );
        assert!(
            base.contains("A repository-dependent request answered without tool use is incomplete")
        );
        assert!(
            base.contains("When all work is complete, output your final response as plain text")
        );

        // Agent collaboration rules
        assert!(base.contains("You may mention another existing agent with `@agent_name`"));
        assert!(base.contains("March will automatically continue the next round as that agent"));
        assert!(base.contains("Do not claim that agent-to-agent handoff is unsupported"));
        assert!(base.contains("Do not reply with meta acknowledgements such as"));
    }

    #[test]
    fn march_prompt_includes_persona_and_behavior() {
        let march = default_march_prompt();

        assert!(march.contains("You are March, an agentic coding partner"));
        assert!(march.contains("If the user is greeting you or making small talk"));
        assert!(
            march.contains(
                "Do not assume every user message is a request for a project status report"
            )
        );
    }

    #[test]
    fn default_system_core_combines_base_and_march() {
        let full = default_system_core();
        assert!(full.contains(base_instructions()));
        assert!(full.contains(default_march_prompt()));
    }

    #[test]
    fn non_march_agents_get_base_instructions_and_own_prompt() {
        let workspace = temp_workspace_dir("ma-agent-system-core");
        let agent_dir = workspace.join(".march").join("agents");
        fs::create_dir_all(&agent_dir).expect("create agents dir");
        fs::write(
            agent_dir.join("reviewer.md"),
            "---\nname: reviewer\ndisplay_name: Code Reviewer\n---\nFocus on implementation risks first.",
        )
        .expect("write reviewer agent");

        let mut session = AgentSession::new(
            AgentConfig::default(),
            ConversationHistory::default(),
            [],
            workspace,
        )
        .expect("create agent session");
        session.set_active_agent("reviewer");

        let prompt = session.system_core_for_active_agent();

        // Has base instructions (shared foundation)
        assert!(prompt.contains("Core operating rule:"));
        assert!(prompt.contains("Tool use:"));
        assert!(prompt.contains("Agent collaboration:"));

        // Has roster with inline (you) marker on active agent
        assert!(prompt.contains("# Available Agents"));
        assert!(
            prompt
                .contains("- reviewer | Code Reviewer | Focus on implementation risks first (you)")
        );
        assert!(!prompt.contains("active_agent:"));

        // Has the reviewer's own system_prompt under Agent Role heading
        assert!(prompt.contains("# Agent Role\n"));
        assert!(prompt.contains("Focus on implementation risks first."));

        // Does NOT have March's persona or the old Active Agent Role section
        assert!(!prompt.contains("You are March, an agentic coding partner"));
        assert!(!prompt.contains("# Active Agent Role"));
        assert!(!prompt.contains("agent_name:"));
    }

    #[test]
    fn write_file_starts_tracking_new_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let temp_path = std::env::current_dir()
            .expect("current dir")
            .join(format!("ma-write-file-{unique}.txt"));

        let mut session = AgentSession::new(
            AgentConfig::default(),
            ConversationHistory::default(),
            [],
            std::env::current_dir().expect("current dir"),
        )
        .expect("create agent session");
        let tool_call = ProviderToolCall {
            id: "call_write".to_string(),
            name: "write_file".to_string(),
            arguments_json: serde_json::json!({
                "path": temp_path,
                "content": "hello from write_file\n",
            })
            .to_string(),
        };

        session
            .execute_tool_call(&tool_call)
            .expect("write_file should succeed");

        let persisted = session.persisted_state();
        assert_eq!(persisted.open_files.len(), 1);
        assert_eq!(persisted.open_files[0].path, temp_path);
        assert!(
            session
                .runtime_open_file_snapshots()
                .contains_key(&temp_path)
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn restore_skips_missing_open_files_from_persisted_state() {
        let missing_path = std::env::current_dir()
            .expect("current dir")
            .join("definitely-missing-open-file.txt");
        let existing_path = std::env::current_dir()
            .expect("current dir")
            .join("Cargo.toml");
        let persisted = PersistedTask {
            task: TaskRecord {
                id: 1,
                name: "test".to_string(),
                title_source: TaskTitleSource::Default,
                title_locked: false,
                working_directory: std::env::current_dir().expect("current dir"),
                selected_provider_id: None,
                selected_model: None,
                active_agent: MARCH_AGENT_NAME.to_string(),
                created_at: SystemTime::now(),
                last_active: SystemTime::now(),
            },
            active_agent: MARCH_AGENT_NAME.to_string(),
            history: ConversationHistory::default(),
            notes: Vec::new(),
            open_files: vec![
                PersistedOpenFile {
                    scope: SHARED_SCOPE.to_string(),
                    path: missing_path.clone(),
                    locked: true,
                },
                PersistedOpenFile {
                    scope: SHARED_SCOPE.to_string(),
                    path: existing_path.clone(),
                    locked: false,
                },
            ],
            hints: Vec::<Hint>::new(),
        };

        let session = AgentSession::restore(AgentConfig::default(), persisted)
            .expect("restore should skip missing files");

        let persisted = session.persisted_state();
        assert_eq!(persisted.open_files.len(), 1);
        assert_eq!(persisted.open_files[0].path, existing_path);
        assert!(!persisted.open_files[0].locked);
    }

    #[test]
    fn normalize_open_files_auto_adds_agents_file_as_locked_first() {
        let workspace = temp_workspace_dir("ma-agent-open-files");
        let regular_path = workspace.join("Cargo.toml");
        fs::write(&regular_path, "[package]\nname = \"demo\"\n").expect("write cargo");
        let agents_path = workspace.join(AGENTS_FILENAME);
        fs::write(&agents_path, "# rules\n").expect("write agents");

        let open_files = normalize_open_files_for_workspace(
            &workspace,
            vec![PersistedOpenFile {
                scope: SHARED_SCOPE.to_string(),
                path: regular_path.clone(),
                locked: false,
            }],
        );

        assert_eq!(open_files.len(), 2);
        assert_eq!(open_files[0].path, agents_path);
        assert!(open_files[0].locked);
        assert_eq!(open_files[1].path, regular_path);
        assert!(!open_files[1].locked);
    }

    #[test]
    fn normalize_open_files_preserves_existing_agents_lock_state_and_position() {
        let workspace = temp_workspace_dir("ma-agent-existing-agents");
        let first_path = workspace.join("src").join("main.rs");
        let agents_path = workspace.join(AGENTS_FILENAME);
        fs::create_dir_all(first_path.parent().expect("main parent")).expect("create src");
        fs::write(&first_path, "fn main() {}\n").expect("write main");
        fs::write(&agents_path, "# rules\n").expect("write agents");

        let open_files = normalize_open_files_for_workspace(
            &workspace,
            vec![
                PersistedOpenFile {
                    scope: SHARED_SCOPE.to_string(),
                    path: first_path.clone(),
                    locked: false,
                },
                PersistedOpenFile {
                    scope: SHARED_SCOPE.to_string(),
                    path: agents_path.clone(),
                    locked: false,
                },
            ],
        );

        assert_eq!(open_files.len(), 2);
        assert_eq!(open_files[0].path, first_path);
        assert_eq!(open_files[1].path, agents_path);
        assert!(!open_files[1].locked);
    }

    #[test]
    fn shell_detection_requires_successful_probe() {
        let resolved = resolve_shell_program_with(
            CommandShell::Bash,
            |candidate| Some(PathBuf::from(format!("C:/fake/{candidate}.exe"))),
            |_, _| false,
        );

        assert_eq!(resolved, None);
    }

    #[test]
    fn shell_detection_returns_first_runnable_candidate() {
        let resolved = resolve_shell_program_with(
            CommandShell::PowerShell,
            |candidate| match candidate {
                "powershell" => Some(PathBuf::from("C:/fake/powershell.exe")),
                "pwsh" => Some(PathBuf::from("C:/fake/pwsh.exe")),
                _ => None,
            },
            |_, path| path.ends_with("pwsh.exe"),
        );

        assert_eq!(resolved.as_deref(), Some("pwsh"));
    }

    #[test]
    fn shell_detection_prefers_pwsh_when_multiple_powershells_work() {
        let resolved = resolve_shell_program_with(
            CommandShell::PowerShell,
            |candidate| match candidate {
                "powershell" => Some(PathBuf::from("C:/fake/powershell.exe")),
                "pwsh" => Some(PathBuf::from("C:/fake/pwsh.exe")),
                _ => None,
            },
            |_, _| true,
        );

        assert_eq!(resolved.as_deref(), Some("pwsh"));
    }

    #[test]
    fn decode_command_output_falls_back_to_gbk_on_windows_style_bytes() {
        let decoded = decode_command_output(&[0xB2, 0xE2, 0xCA, 0xD4]);
        assert_eq!(decoded, "测试");
    }

    #[test]
    fn transient_messages_accumulate_tool_rounds() {
        let first_call = ProviderToolCall {
            id: "call_1".to_string(),
            name: "run_command".to_string(),
            arguments_json: serde_json::json!({
                "shell": "cmd",
                "command": "type package.json",
            })
            .to_string(),
        };
        let second_call = ProviderToolCall {
            id: "call_2".to_string(),
            name: "run_command".to_string(),
            arguments_json: serde_json::json!({
                "shell": "cmd",
                "command": "type Cargo.toml",
            })
            .to_string(),
        };

        let mut transient_messages = Vec::<RequestMessage>::new();
        append_assistant_tool_call_message(
            &mut transient_messages,
            None,
            std::slice::from_ref(&first_call),
        );
        transient_messages.push(RequestMessage::tool(
            first_call.id.clone(),
            "Exit code: 0\nStdout:\n{}",
        ));
        append_assistant_tool_call_message(
            &mut transient_messages,
            None,
            std::slice::from_ref(&second_call),
        );

        let payload = serde_json::to_value(&transient_messages).expect("serialize messages");
        let messages = payload.as_array().expect("messages array");

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["tool_call_id"], "call_1");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["tool_calls"][0]["id"], "call_2");
    }

    fn temp_workspace_dir(prefix: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create workspace");
        root
    }
}
