use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use encoding_rs::GBK;
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

use crate::context::{
    AgentContext, AgentContextBuilder, ContextBuildConfig, ContextPressure, ConversationHistory,
    DisplayTurn, FileSnapshot, Hint, Injection, NoteEntry, Role, SystemStatus, ToolSummary,
    render_file_snapshot_for_prompt,
};
use crate::provider::{
    ApiToolCallRequest, ApiToolFunctionCallRequest, OpenAiCompatibleClient, ProviderProgressEvent,
    ProviderToolCall, RequestMessage, build_messages,
};
use crate::storage::{PersistedOpenFile, PersistedTask, PersistedTaskState};
use crate::tools::ToolRuntime;
use crate::ui::{
    UiContextPressureView, UiContextUsageSectionView, UiContextUsageView, UiFileSnapshotView,
    UiRuntimeSnapshot, UiShellView, UiSystemStatusView,
};
use crate::watcher::FileWatcherService;

pub struct AgentSession {
    config: AgentConfig,
    watcher: FileWatcherService,
    history: ConversationHistory,
    notes: IndexMap<String, NoteEntry>,
    locked_files: Vec<PathBuf>,
    hints: Vec<Hint>,
    injections: Vec<Injection>,
    available_shells: Vec<AvailableShell>,
    working_directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub system_core: String,
    pub max_recent_turns: usize,
}

pub const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 24_000;

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_core: default_system_core().to_string(),
            max_recent_turns: 3,
        }
    }
}

fn default_system_core() -> &'static str {
    r#"You are March, an agentic coding partner whose source of truth is the filesystem.

Role:
- You are a calm, capable coding assistant for a real local workspace.
- You help with software tasks, but you can also chat naturally when the user is simply greeting, confirming, or asking casual questions.
- Do not assume every user message is a request for a project status report or engineering summary.

Core operating rule:
- The local workspace is the source of truth for project and code questions.
- Do not guess about repository contents, architecture, implementation status, test status, or file contents when they can be verified from the workspace.

Behavior:
- If the user is greeting you or making small talk, reply naturally, briefly, and in the user's language.
- If the user asks about the project, code, bugs, architecture, tests, implementation details, or anything that depends on the current workspace, switch into coding-assistant mode and ground your answer in tool-based inspection.
- For concrete coding or investigation requests, act with initiative: inspect the workspace, choose sensible next steps, and make progress without asking the user to manually fetch local files or restate obvious context.
- Default to doing the next useful step yourself. Ask for confirmation only when the decision would change scope, risk destructive effects, or has multiple non-obvious directions with meaningful tradeoffs.
- Do not turn straightforward execution into a back-and-forth approval loop. When the user says to choose, decide and proceed.
- Stay grounded in the current filesystem-backed context. Never pretend stale snapshots are the truth.
- Do not invent work you have not done. If you are unsure, say so plainly.

Tool use:
- For any request that depends on the current workspace, repository, codebase, files, tests, configuration, build system, or local environment, you must inspect the workspace with one or more tools before giving a substantive answer.
- Do not end the turn with only a preamble, intention, or plan such as “I’ll inspect the repo first”.
- If the answer depends on filesystem or environment evidence, gather that evidence first via tools.
- Prefer the open-files context layer for file contents that are already tracked; do not re-read the same file through shell commands unless you need a view that open files cannot provide.
- Only finish without tool use if the user's request can be fully and safely answered without inspecting the workspace.
- Do not use tools for simple greetings or casual conversation.
- When all work is complete, output your final response as plain text without calling any tools. That is what ends the turn.
- Do not call any tool to deliver the final answer.
- A repository-dependent request answered without tool use is incomplete.

Completion rule:
- Only end your turn when one of these is true:
  1. you have completed the necessary tool-assisted investigation or work, or
  2. you have determined that no tool use is actually necessary for this request.
- If the task is repository-dependent, a tool-free answer is usually not sufficient.

Tone:
- Be direct, warm, and concise.
- Match the user's language when practical.
- Avoid unnecessary status dumps unless the user explicitly asks for them."#
}

#[derive(Debug, Clone)]
pub struct CommandRequest {
    pub command: String,
    pub shell: CommandShell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandShell {
    Sh,
    Bash,
    PowerShell,
    Cmd,
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
    pub replies: Vec<ReplyEvent>,
    pub debug_rounds: Vec<DebugRound>,
}

#[derive(Debug, Clone)]
pub enum AgentProgressEvent {
    Status {
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
    ReplyPreview {
        message: String,
    },
    Reply(ReplyEvent),
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
pub struct ReplyEvent {
    pub message: String,
    pub wait: bool,
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
    ) -> Result<Self> {
        let mut watcher = FileWatcherService::new()?;
        for path in open_files {
            watcher.watch_file(path)?;
        }

        Ok(Self {
            config,
            watcher,
            history,
            notes: IndexMap::new(),
            locked_files: Vec::new(),
            hints: Vec::new(),
            injections: Vec::new(),
            available_shells: detect_available_shells()?,
            working_directory: std::env::current_dir()
                .context("failed to resolve current directory")?,
        })
    }

    pub fn restore(config: AgentConfig, task: PersistedTask) -> Result<Self> {
        let open_paths = task
            .open_files
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let mut session = Self::new(config, task.history, open_paths)?;
        session.notes = task.notes;
        session.hints = task.hints;
        session.locked_files = task
            .open_files
            .into_iter()
            .filter(|entry| entry.locked)
            .map(|entry| entry.path)
            .collect();
        Ok(session)
    }

    pub async fn handle_user_message(
        &mut self,
        client: &OpenAiCompatibleClient,
        content: impl Into<String>,
    ) -> Result<AgentRunResult> {
        self.handle_user_message_with_events(client, content, |_, _| Ok(()))
            .await
    }

    pub async fn handle_user_message_with_events<F>(
        &mut self,
        client: &OpenAiCompatibleClient,
        content: impl Into<String>,
        mut on_event: F,
    ) -> Result<AgentRunResult>
    where
        F: FnMut(&AgentSession, AgentProgressEvent) -> Result<()>,
    {
        self.add_user_turn(content);

        let mut replies = Vec::new();
        let mut summaries = Vec::new();
        let mut debug_rounds = Vec::new();
        let mut transient_messages: Vec<RequestMessage> = Vec::new();
        let mut iteration = 0usize;

        loop {
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
            let mut conversation = build_messages(&context);
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
                                AgentProgressEvent::ReplyPreview {
                                    message: content_preview.clone(),
                                },
                            )?;
                        }
                    }
                    Ok(())
                })
                .await?;
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
                // No tool calls: the model is done. Plain text output is the final reply.
                // This is the only legitimate turn exit — mirroring Codex's "text output = done" contract.
                let final_message = match assistant_text {
                    Some(text) if !text.trim().is_empty() => text,
                    _ => bail!("provider returned no tool calls and no text; cannot end turn"),
                };
                let reply = ReplyEvent {
                    message: final_message,
                    wait: true,
                };
                self.add_assistant_turn(reply.message.clone(), summaries.clone());
                on_event(self, AgentProgressEvent::Reply(reply.clone()))?;
                replies.push(reply);
                debug_rounds.push(debug_round);
                on_event(
                    self,
                    AgentProgressEvent::RoundCompleted(
                        debug_rounds.last().cloned().expect("debug round just pushed"),
                    ),
                )?;
                return Ok(AgentRunResult {
                    replies,
                    debug_rounds,
                });
            }

            append_assistant_tool_call_message(
                &mut transient_messages,
                assistant_text,
                &response.tool_calls,
            );

            for tool_call in response.tool_calls {
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
                        on_event(
                            self,
                            AgentProgressEvent::ToolFinished {
                                tool_call_id: tool_call.id.clone(),
                                status: AgentToolStatus::Error,
                                summary: tool_summary.clone(),
                                preview: Some(error.to_string()),
                            },
                        )?;
                        return Err(error);
                    }
                };
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

    pub fn add_user_turn(&mut self, content: impl Into<String>) {
        self.history.turns.push(DisplayTurn {
            role: Role::User,
            content: content.into(),
            tool_calls: Vec::new(),
            timestamp: SystemTime::now(),
        });
    }

    pub fn add_assistant_turn(&mut self, content: impl Into<String>, tool_calls: Vec<ToolSummary>) {
        self.history.turns.push(DisplayTurn {
            role: Role::Assistant,
            content: content.into(),
            tool_calls,
            timestamp: SystemTime::now(),
        });
    }

    pub fn add_hint(&mut self, hint: Hint) {
        self.hints.push(hint);
    }

    pub fn write_note(&mut self, id: impl Into<String>, content: impl Into<String>) {
        self.notes.insert(id.into(), NoteEntry::new(content));
    }

    pub fn remove_note(&mut self, id: &str) {
        self.notes.shift_remove(id);
    }

    pub fn open_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.watcher.watch_file(self.resolve_path(path.into()))?;
        Ok(())
    }

    pub fn close_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        let path = self.resolve_path(path.into());
        if self.locked_files.iter().any(|locked| locked == &path) {
            bail!("cannot close locked file {}", path.display());
        }
        self.watcher.unwatch_file(path)?;
        Ok(())
    }

    pub fn lock_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        let path = self.resolve_path(path.into());
        if !self.open_file_snapshots().contains_key(&path) {
            bail!("cannot lock unopened file {}", path.display());
        }
        if !self.locked_files.iter().any(|locked| locked == &path) {
            self.locked_files.push(path);
        }
        Ok(())
    }

    pub fn unlock_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        let path = self.resolve_path(path.into());
        self.locked_files.retain(|locked| locked != &path);
        Ok(())
    }

    pub fn build_context(&mut self) -> AgentContext {
        self.prune_expired_hints();
        let tools = ToolRuntime::for_session(&self.available_shells, &self.working_directory).tools;
        let context = AgentContextBuilder::new(self.config.system_core.clone())
            .with_config(ContextBuildConfig {
                max_recent_chat_turns: self.config.max_recent_turns,
            })
            .injections(self.injections.clone())
            .tools(tools)
            .notes(self.notes.clone())
            .system_status(SystemStatus {
                locked_files: self.locked_files.clone(),
                context_pressure: self.estimate_context_pressure(DEFAULT_CONTEXT_WINDOW_TOKENS),
            })
            .hints(self.hints.clone())
            .history(self.history.clone())
            .build_from_open_files(self.open_file_snapshots());
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
            history: self.history.clone(),
            notes: self.notes.clone(),
            open_files: self
                .open_file_snapshots()
                .keys()
                .map(|path| PersistedOpenFile {
                    path: path.clone(),
                    locked: self.locked_files.iter().any(|locked| locked == path),
                })
                .collect(),
            hints: self.hints.clone(),
            last_active: SystemTime::now(),
        }
    }

    pub fn available_shells(&self) -> &[AvailableShell] {
        &self.available_shells
    }

    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub fn runtime_open_file_snapshots(&self) -> IndexMap<PathBuf, FileSnapshot> {
        self.open_file_snapshots()
    }

    pub fn ui_system_status(&self, context_budget_tokens: usize) -> UiSystemStatusView {
        UiSystemStatusView {
            locked_files: self.locked_files.clone(),
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
                estimate_token_count(&self.config.system_core),
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
                self.notes
                    .values()
                    .map(|note| estimate_token_count(&note.content))
                    .sum(),
            ),
            UiContextUsageSectionView::new(
                "chat",
                self.history
                    .turns
                    .iter()
                    .map(|turn| estimate_token_count(&turn.content))
                    .sum(),
            ),
            UiContextUsageSectionView::new(
                "files",
                self.open_file_snapshots()
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
        let available_shells = self
            .available_shells
            .iter()
            .map(|shell| UiShellView {
                kind: shell.kind.label().to_string(),
                program: shell.program.clone(),
            })
            .collect::<Vec<_>>();

        let open_files = self
            .open_file_snapshots()
            .into_iter()
            .map(|(_, snapshot)| UiFileSnapshotView::from(snapshot))
            .collect::<Vec<_>>();

        UiRuntimeSnapshot::new(
            self.working_directory.clone(),
            available_shells,
            open_files,
            self.ui_system_status(context_budget_tokens),
            self.ui_context_usage(context_budget_tokens),
        )
    }

    fn execute_tool_call(&mut self, tool_call: &ProviderToolCall) -> Result<ToolOutcome> {
        let args: Value = serde_json::from_str(&tool_call.arguments_json).with_context(|| {
            format!(
                "failed to decode arguments for tool {}: {}",
                tool_call.name, tool_call.arguments_json
            )
        })?;

        match tool_call.name.as_str() {
            "run_command" => {
                let args: RunCommandArgs =
                    serde_json::from_value(args).context("invalid run_command args")?;
                let execution = self.run_command(CommandRequest {
                    command: args.command,
                    shell: parse_shell(&args.shell)?,
                })?;
                Ok(ToolOutcome {
                    result_text: format_tool_output(&execution),
                    summary: Some(ToolSummary {
                        name: "run_command".to_string(),
                        summary: format!(
                            "{} (exit code {})",
                            execution.command, execution.exit_code
                        ),
                    }),

                })
            }
            "open_file" => {
                let args: PathArgs =
                    serde_json::from_value(args).context("invalid open_file args")?;
                let path = self.resolve_path(args.path);
                self.open_file(path.clone())?;
                Ok(simple_tool(
                    format!("opened {}", path.display()),
                    "open_file",
                    format!("开始追踪 {}", path.display()),
                ))
            }
            "close_file" => {
                let args: PathArgs =
                    serde_json::from_value(args).context("invalid close_file args")?;
                let path = self.resolve_path(args.path);
                self.close_file(path.clone())?;
                Ok(simple_tool(
                    format!("closed {}", path.display()),
                    "close_file",
                    format!("停止追踪 {}", path.display()),
                ))
            }
            "write_file" => {
                let args: WriteFileArgs =
                    serde_json::from_value(args).context("invalid write_file args")?;
                let path = self.resolve_path(args.path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
                let _guard = if path.exists() {
                    Some(self.watcher.store().begin_agent_write([path.clone()])?)
                } else {
                    None
                };
                fs::write(&path, args.content)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                self.track_written_file(&path)?;
                Ok(simple_tool(
                    format!("wrote {}", path.display()),
                    "write_file",
                    format!("写入了 {}", path.display()),
                ))
            }
            "replace_lines" => {
                let args: ReplaceLinesArgs =
                    serde_json::from_value(args).context("invalid replace_lines args")?;
                let path = self.resolve_path(args.path);
                edit_lines(&path, |lines| {
                    replace_line_range(lines, args.start_line, args.end_line, &args.new_content)
                })?;
                self.refresh_if_watched(&path)?;
                Ok(simple_tool(
                    format!(
                        "replaced lines {}-{} in {}",
                        args.start_line,
                        args.end_line,
                        path.display()
                    ),
                    "replace_lines",
                    format!(
                        "修改了 {} 第 {}-{} 行",
                        path.display(),
                        args.start_line,
                        args.end_line
                    ),
                ))
            }
            "insert_lines" => {
                let args: InsertLinesArgs =
                    serde_json::from_value(args).context("invalid insert_lines args")?;
                let path = self.resolve_path(args.path);
                edit_lines(&path, |lines| {
                    insert_line_block(lines, args.after_line, &args.new_content)
                })?;
                self.refresh_if_watched(&path)?;
                Ok(simple_tool(
                    format!("inserted after {} in {}", args.after_line, path.display()),
                    "insert_lines",
                    format!("在 {} 第 {} 行后插入内容", path.display(), args.after_line),
                ))
            }
            "delete_lines" => {
                let args: DeleteLinesArgs =
                    serde_json::from_value(args).context("invalid delete_lines args")?;
                let path = self.resolve_path(args.path);
                edit_lines(&path, |lines| {
                    delete_line_range(lines, args.start_line, args.end_line)
                })?;
                self.refresh_if_watched(&path)?;
                Ok(simple_tool(
                    format!(
                        "deleted lines {}-{} in {}",
                        args.start_line,
                        args.end_line,
                        path.display()
                    ),
                    "delete_lines",
                    format!(
                        "删除了 {} 第 {}-{} 行",
                        path.display(),
                        args.start_line,
                        args.end_line
                    ),
                ))
            }
            "write_note" => {
                let args: WriteNoteArgs =
                    serde_json::from_value(args).context("invalid write_note args")?;
                self.write_note(args.id.clone(), args.content);
                Ok(simple_tool(
                    format!("stored note {}", args.id),
                    "write_note",
                    format!("更新了 note {}", args.id),
                ))
            }
            "remove_note" => {
                let args: RemoveNoteArgs =
                    serde_json::from_value(args).context("invalid remove_note args")?;
                self.remove_note(&args.id);
                Ok(simple_tool(
                    format!("removed note {}", args.id),
                    "remove_note",
                    format!("移除了 note {}", args.id),
                ))
            }
            other => bail!("unknown tool call: {}", other),
        }
    }

    fn resolve_shell(&self, shell: CommandShell) -> Result<AvailableShell> {
        self.available_shells
            .iter()
            .find(|candidate| candidate.kind == shell)
            .cloned()
            .with_context(|| format!("requested shell {} is not available", shell.label()))
    }

    fn resolve_path(&self, path: PathBuf) -> PathBuf {
        if path.is_absolute() {
            path
        } else {
            self.working_directory.join(path)
        }
    }

    fn open_file_snapshots(&self) -> IndexMap<PathBuf, FileSnapshot> {
        self.watcher.store().snapshots()
    }

    fn refresh_if_watched(&self, path: &Path) -> Result<()> {
        if self.open_file_snapshots().contains_key(path) {
            self.watcher
                .store()
                .refresh_file(path, crate::context::ModifiedBy::Agent)?;
        }
        Ok(())
    }

    fn track_written_file(&mut self, path: &Path) -> Result<()> {
        if self.open_file_snapshots().contains_key(path) {
            self.watcher
                .store()
                .refresh_file(path, crate::context::ModifiedBy::Agent)?;
        } else {
            self.watcher.watch_file(path.to_path_buf())?;
            self.watcher
                .store()
                .refresh_file(path, crate::context::ModifiedBy::Agent)?;
        }
        Ok(())
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
        let size = estimate_token_count(&self.config.system_core)
            + self
                .injections
                .iter()
                .map(|injection| estimate_token_count(&injection.content))
                .sum::<usize>()
            + self
                .notes
                .values()
                .map(|note| estimate_token_count(&note.content))
                .sum::<usize>()
            + self
                .history
                .turns
                .iter()
                .map(|turn| estimate_token_count(&turn.content))
                .sum::<usize>()
            + self
                .open_file_snapshots()
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

/// 这里用轻量估算而不是 provider 专属 tokenizer：
/// - ASCII 文本粗略按 4 chars ≈ 1 token
/// - 非 ASCII 字符（尤其中文）按 1 char ≈ 1 token
/// 这样不会冒充“精确 token”，但比直接用字节数更接近真实上下文消耗。
fn estimate_token_count(text: &str) -> usize {
    let ascii_chars = text.chars().filter(|ch| ch.is_ascii()).count();
    let non_ascii_chars = text.chars().count().saturating_sub(ascii_chars);
    ascii_chars.div_ceil(4) + non_ascii_chars
}


fn summarize_tool_call(name: &str, arguments_json: &str) -> String {
    let args = serde_json::from_str::<Value>(arguments_json).unwrap_or(Value::Null);
    match name {
        "run_command" => {
            let shell = args.get("shell").and_then(Value::as_str).unwrap_or("shell");
            let command = args.get("command").and_then(Value::as_str).unwrap_or("");
            if command.is_empty() {
                "run_command".to_string()
            } else {
                format!("run_command {} {}", shell, command)
            }
        }
        "open_file" | "close_file" | "write_file" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            if path.is_empty() {
                name.to_string()
            } else {
                format!("{name} {path}")
            }
        }
        "replace_lines" | "delete_lines" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            let start_line = args.get("start_line").and_then(Value::as_u64).unwrap_or(0);
            let end_line = args.get("end_line").and_then(Value::as_u64).unwrap_or(0);
            if path.is_empty() || start_line == 0 || end_line == 0 {
                name.to_string()
            } else {
                format!("{name} {path}:{start_line}-{end_line}")
            }
        }
        "insert_lines" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            let after_line = args.get("after_line").and_then(Value::as_u64).unwrap_or(0);
            if path.is_empty() || after_line == 0 {
                name.to_string()
            } else {
                format!("{name} {path}:{after_line}")
            }
        }
        "write_note" | "remove_note" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            if id.is_empty() {
                name.to_string()
            } else {
                format!("{name} {id}")
            }
        }
        _ => name.to_string(),
    }
}

fn preview_tool_result(result_text: &str) -> Option<String> {
    let preview = result_text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("");
    if preview.is_empty() {
        return None;
    }

    if preview.chars().count() > 120 {
        Some(format!(
            "{}…",
            preview.chars().take(120).collect::<String>()
        ))
    } else {
        Some(preview.to_string())
    }
}

fn decode_command_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    if let Ok(text) = String::from_utf8(bytes.to_vec()) {
        return text.trim().to_string();
    }

    #[cfg(windows)]
    {
        let (decoded, _, had_errors) = GBK.decode(bytes);
        if !had_errors {
            return decoded.trim().to_string();
        }
    }

    String::from_utf8_lossy(bytes).trim().to_string()
}


fn simple_tool(result_text: String, name: &str, summary: String) -> ToolOutcome {
    ToolOutcome {
        result_text,
        summary: Some(ToolSummary {
            name: name.to_string(),
            summary,
        }),
    }
}

fn to_request_tool_call(tool_call: &ProviderToolCall) -> ApiToolCallRequest {
    ApiToolCallRequest {
        id: tool_call.id.clone(),
        tool_type: "function".to_string(),
        function: ApiToolFunctionCallRequest {
            name: tool_call.name.clone(),
            arguments: tool_call.arguments_json.clone(),
        },
    }
}

fn append_assistant_tool_call_message(
    transient_messages: &mut Vec<RequestMessage>,
    assistant_text: Option<String>,
    tool_calls: &[ProviderToolCall],
) {
    transient_messages.push(RequestMessage::assistant_tool_calls_with_text(
        assistant_text,
        tool_calls.iter().map(to_request_tool_call).collect(),
    ));
}

fn render_prompt(context: &AgentContext) -> String {
    let mut output = String::new();
    output.push_str("# System Core\n");
    output.push_str(&context.system_core);
    output.push_str("\n\n# Injections\n");
    if context.injections.is_empty() {
        output.push_str("(none)\n");
    } else {
        for injection in &context.injections {
            output.push_str(&format!("## {}\n{}\n", injection.id, injection.content));
        }
    }
    output.push_str("\n# Tools\n");
    output.push_str(
        &ToolRuntime {
            tools: context.tools.clone(),
        }
        .render_prompt_section(),
    );
    output.push_str("\n\n# Open Files\n");
    for snapshot in context.open_files_in_prompt_order() {
        output.push_str(&render_file_snapshot_for_prompt(snapshot));
        output.push('\n');
    }
    output.push_str("# Notes\n");
    if context.notes.is_empty() {
        output.push_str("(none)\n");
    } else {
        for (id, note) in &context.notes {
            output.push_str(&format!("{id}: {}\n", note.content));
        }
    }
    output.push_str("\n# System Status\n");
    if context.system_status.is_empty() {
        output.push_str("(none)\n");
    } else {
        if !context.system_status.locked_files.is_empty() {
            output.push_str("locked_files:\n");
            for path in &context.system_status.locked_files {
                output.push_str(&format!("- {}\n", path.display()));
            }
        }
        if let Some(pressure) = &context.system_status.context_pressure {
            output.push_str(&format!(
                "context_pressure: {}% - {}\n",
                pressure.used_percent, pressure.message
            ));
        }
    }
    output.push_str("\n# Hints\n");
    if context.hints.is_empty() {
        output.push_str("(none)\n");
    } else {
        for hint in &context.hints {
            output.push_str(&format!("- {}\n", hint.content));
        }
    }
    output.push_str("\n# Recent Chat\n");
    for turn in &context.recent_chat {
        output.push_str(&format!("{:?}: {}\n", turn.role, turn.content));
    }
    output
}

fn format_tool_output(execution: &CommandExecution) -> String {
    let mut text = format!(
        "Command: {}\nShell: {:?}\nWorking directory: {}\nExit code: {}\nStarted at: {:?}\nFinished at: {:?}",
        execution.command,
        execution.shell,
        execution.working_directory.display(),
        execution.exit_code,
        execution.started_at,
        execution.finished_at
    );
    if !execution.stdout.is_empty() {
        text.push_str(&format!("\nStdout:\n{}", execution.stdout));
    }
    if !execution.stderr.is_empty() {
        text.push_str(&format!("\nStderr:\n{}", execution.stderr));
    }
    text
}

impl CommandShell {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sh => "sh",
            Self::Bash => "bash",
            Self::PowerShell => "powershell",
            Self::Cmd => "cmd",
        }
    }

    fn candidates(self) -> &'static [&'static str] {
        match self {
            Self::Sh => &["sh"],
            Self::Bash => &["bash"],
            Self::PowerShell => &["pwsh", "powershell"],
            Self::Cmd => &["cmd"],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableShell {
    pub kind: CommandShell,
    pub program: String,
}

fn shell_command(
    shell: CommandShell,
    program: &str,
    command: &str,
    working_directory: &Path,
) -> Result<std::process::Output> {
    match shell {
        CommandShell::Sh => Command::new(program)
            .args(["-lc", command])
            .current_dir(working_directory)
            .output()
            .context("failed to spawn sh"),
        CommandShell::Bash => Command::new(program)
            .args(["-lc", command])
            .current_dir(working_directory)
            .output()
            .context("failed to spawn bash"),
        CommandShell::PowerShell => Command::new(program)
            .args(["-NoProfile", "-Command", command])
            .current_dir(working_directory)
            .output()
            .context("failed to spawn powershell"),
        CommandShell::Cmd => Command::new(program)
            .args(["/C", command])
            .current_dir(working_directory)
            .output()
            .context("failed to spawn cmd"),
    }
}

fn detect_available_shells() -> Result<Vec<AvailableShell>> {
    let mut available = Vec::new();
    for kind in [
        CommandShell::PowerShell,
        CommandShell::Cmd,
        CommandShell::Bash,
        CommandShell::Sh,
    ] {
        if let Some(program) = resolve_shell_program(kind) {
            available.push(AvailableShell { kind, program });
        }
    }
    if available.is_empty() {
        bail!("failed to detect any runnable shell in current PATH");
    }
    Ok(available)
}

fn resolve_shell_program(shell: CommandShell) -> Option<String> {
    resolve_shell_program_with(shell, executable_in_path, shell_probe_succeeds)
}

fn resolve_shell_program_with<L, P>(
    shell: CommandShell,
    locate_program: L,
    probe_program: P,
) -> Option<String>
where
    L: Fn(&str) -> Option<PathBuf>,
    P: Fn(CommandShell, &Path) -> bool,
{
    shell.candidates().iter().find_map(|candidate| {
        let executable = locate_program(candidate)?;
        // Only advertise shells that can complete a minimal command round-trip.
        // This filters out PATH stubs such as WindowsApps\bash.exe when WSL is not installed.
        probe_program(shell, &executable).then(|| (*candidate).to_string())
    })
}

fn shell_probe_succeeds(shell: CommandShell, program: &Path) -> bool {
    let mut command = Command::new(program);
    match shell {
        CommandShell::Sh | CommandShell::Bash => {
            command.args(["-lc", "exit 0"]);
        }
        CommandShell::PowerShell => {
            command.args(["-NoProfile", "-Command", "exit 0"]);
        }
        CommandShell::Cmd => {
            command.args(["/C", "exit 0"]);
        }
    }
    command.output().is_ok_and(|output| output.status.success())
}

fn executable_in_path(program: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    let path_exts = executable_extensions();
    for dir in env::split_paths(&path) {
        for candidate in candidate_paths(&dir, program, &path_exts) {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn candidate_paths(dir: &Path, program: &str, extensions: &[OsString]) -> Vec<PathBuf> {
    let mut candidates = vec![dir.join(program)];
    if Path::new(program).extension().is_none() {
        for ext in extensions {
            let ext = ext.to_string_lossy();
            let suffix = if ext.starts_with('.') {
                ext.to_string()
            } else {
                format!(".{ext}")
            };
            candidates.push(dir.join(format!("{program}{suffix}")));
        }
    }
    candidates
}

fn executable_extensions() -> Vec<OsString> {
    #[cfg(windows)]
    {
        env::var_os("PATHEXT")
            .map(|value| {
                value
                    .to_string_lossy()
                    .split(';')
                    .filter(|ext| !ext.is_empty())
                    .map(OsString::from)
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![".COM", ".EXE", ".BAT", ".CMD"]
                    .into_iter()
                    .map(OsString::from)
                    .collect()
            })
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

fn parse_shell(shell: &str) -> Result<CommandShell> {
    match shell {
        "sh" => Ok(CommandShell::Sh),
        "bash" => Ok(CommandShell::Bash),
        "powershell" | "pwsh" => Ok(CommandShell::PowerShell),
        "cmd" => Ok(CommandShell::Cmd),
        other => bail!("unsupported shell {}", other),
    }
}

fn edit_lines<F>(path: &Path, mutate: F) -> Result<()>
where
    F: FnOnce(&mut Vec<String>) -> Result<()>,
{
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let trailing_newline = content.ends_with('\n');
    let mut lines = if content.is_empty() {
        Vec::new()
    } else {
        content
            .split('\n')
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
    };
    if lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }
    mutate(&mut lines)?;
    let mut output = lines.join("\n");
    if trailing_newline {
        output.push('\n');
    }
    fs::write(path, output).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

fn replace_line_range(
    lines: &mut Vec<String>,
    start_line: usize,
    end_line: usize,
    new_content: &str,
) -> Result<()> {
    validate_line_range(lines, start_line, end_line)?;
    let replacement = new_content
        .trim_end_matches('\n')
        .split('\n')
        .filter(|part| !part.is_empty() || !new_content.is_empty())
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    lines.splice((start_line - 1)..end_line, replacement);
    Ok(())
}

fn insert_line_block(lines: &mut Vec<String>, after_line: usize, new_content: &str) -> Result<()> {
    if after_line > lines.len() {
        bail!(
            "insert_lines after_line {} is out of range for {} lines",
            after_line,
            lines.len()
        );
    }
    let insertion = if new_content.is_empty() {
        Vec::new()
    } else {
        new_content
            .trim_end_matches('\n')
            .split('\n')
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
    };
    lines.splice(after_line..after_line, insertion);
    Ok(())
}

fn delete_line_range(lines: &mut Vec<String>, start_line: usize, end_line: usize) -> Result<()> {
    validate_line_range(lines, start_line, end_line)?;
    lines.drain((start_line - 1)..end_line);
    Ok(())
}

fn validate_line_range(lines: &[String], start_line: usize, end_line: usize) -> Result<()> {
    if start_line == 0 || end_line == 0 || start_line > end_line || end_line > lines.len() {
        bail!(
            "invalid line range {}-{} for {} lines",
            start_line,
            end_line,
            lines.len()
        );
    }
    Ok(())
}


#[derive(Debug, Deserialize)]
struct RunCommandArgs {
    shell: String,
    command: String,
}

#[derive(Debug, Deserialize)]
struct PathArgs {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct WriteFileArgs {
    path: PathBuf,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ReplaceLinesArgs {
    path: PathBuf,
    start_line: usize,
    end_line: usize,
    new_content: String,
}

#[derive(Debug, Deserialize)]
struct InsertLinesArgs {
    path: PathBuf,
    after_line: usize,
    new_content: String,
}

#[derive(Debug, Deserialize)]
struct DeleteLinesArgs {
    path: PathBuf,
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Deserialize)]
struct WriteNoteArgs {
    id: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct RemoveNoteArgs {
    id: String,
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        AgentConfig, AgentSession, CommandShell, append_assistant_tool_call_message,
        decode_command_output, default_system_core, resolve_shell_program_with,
    };
    use crate::context::ConversationHistory;
    use crate::provider::{ProviderToolCall, RequestMessage};

    #[test]
    fn default_system_prompt_includes_chat_and_tool_guidance() {
        let prompt = default_system_core();

        assert!(prompt.contains("If the user is greeting you or making small talk"));
        assert!(
            prompt.contains(
                "Do not assume every user message is a request for a project status report"
            )
        );
        assert!(
            prompt.contains(
                "you must inspect the workspace with one or more tools before giving a substantive answer"
            )
        );
        assert!(prompt.contains("A repository-dependent request answered without tool use is incomplete"));
        assert!(prompt.contains("When all work is complete, output your final response as plain text"));
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

        let mut session =
            AgentSession::new(AgentConfig::default(), ConversationHistory::default(), [])
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
}
