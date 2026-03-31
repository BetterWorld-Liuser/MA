use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use anyhow::{Context, Result};
use indexmap::IndexMap;

use crate::context::{
    AgentContext, AgentContextBuilder, ContextBuildConfig, ConversationHistory, DisplayTurn,
    FileSnapshot, Injection, Role, ToolSummary,
};
use crate::provider::OpenAiCompatibleClient;
use crate::tools::{ToolDefinition, ToolRuntime};
use crate::watcher::FileWatcherService;

/// AgentSession 是当前阶段的主编排对象：
/// 它把外部聊天历史、open file 集、notes、命令执行和模型调用收拢到同一个会话里。
pub struct AgentSession {
    config: AgentConfig,
    watcher: FileWatcherService,
    history: ConversationHistory,
    notes: IndexMap<String, String>,
    injections: Vec<Injection>,
    available_shells: Vec<AvailableShell>,
    working_directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub system_core: String,
    pub max_recent_turns: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_core:
                "You are Ma, an agentic coding assistant whose source of truth is the filesystem."
                    .to_string(),
            max_recent_turns: 3,
        }
    }
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

impl AgentSession {
    /// 会话初始化时先建立 open file watcher，这样后续 prompt 都基于同一份实时文件视图构建。
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
            injections: Vec::new(),
            available_shells: detect_available_shells()?,
            working_directory: std::env::current_dir()
                .context("failed to resolve current directory")?,
        })
    }

    pub fn open_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.watcher.watch_file(path.into())?;
        Ok(())
    }

    pub fn close_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.watcher.unwatch_file(path.into())?;
        Ok(())
    }

    pub fn add_injection(&mut self, id: impl Into<String>, content: impl Into<String>) {
        let id = id.into();
        let content = content.into();

        if let Some(injection) = self.injections.iter_mut().find(|injection| injection.id == id) {
            injection.content = content;
        } else {
            self.injections.push(Injection { id, content });
        }
    }

    pub fn write_note(&mut self, id: impl Into<String>, content: impl Into<String>) {
        let id = id.into();
        let content = content.into();

        self.notes.insert(id, content);
    }

    pub fn remove_note(&mut self, id: &str) {
        self.notes.shift_remove(id);
    }

    pub fn add_user_turn(&mut self, content: impl Into<String>) {
        self.history.turns.push(DisplayTurn {
            role: Role::User,
            content: content.into(),
            tool_calls: Vec::new(),
            timestamp: SystemTime::now(),
        });
    }

    pub fn add_assistant_turn(
        &mut self,
        content: impl Into<String>,
        tool_calls: Vec<ToolSummary>,
    ) {
        self.history.turns.push(DisplayTurn {
            role: Role::Assistant,
            content: content.into(),
            tool_calls,
            timestamp: SystemTime::now(),
        });
    }

    pub fn build_context(&self) -> AgentContext {
        let tools = ToolRuntime::for_session(&self.available_shells, &self.working_directory).tools;

        AgentContextBuilder::new(self.config.system_core.clone())
            .with_config(ContextBuildConfig {
                max_recent_chat_turns: self.config.max_recent_turns,
            })
            .injections(self.injections.clone())
            .tools(tools)
            .notes(self.notes.clone())
            .history(self.history.clone())
            .build_from_open_files(self.open_file_snapshots())
    }

    pub fn render_prompt(&self) -> String {
        let context = self.build_context();
        render_prompt(&context, &self.available_shells, &self.working_directory)
    }

    pub async fn generate_assistant_reply(
        &mut self,
        client: &OpenAiCompatibleClient,
    ) -> Result<String> {
        let context = self.build_context();
        let reply = client.complete_context(&context).await?;

        self.add_assistant_turn(
            reply.clone(),
            vec![ToolSummary {
                name: "chat.completions".to_string(),
                summary: "generated assistant reply from OpenAI-compatible provider".to_string(),
            }],
        );

        Ok(reply)
    }

    /// run_command 负责调用外部环境能力；
    /// 命令执行日志属于当前轮执行态，不会混入跨轮 recent_chat。
    pub fn run_command(&mut self, request: CommandRequest) -> Result<CommandExecution> {
        let started_at = SystemTime::now();
        let selected_shell = self.resolve_shell(request.shell)?;
        let tracked_paths = self.open_file_snapshots().keys().cloned().collect::<Vec<_>>();
        let _agent_write_guard = self
            .watcher
            .store()
            .begin_agent_write(tracked_paths.clone())?;
        let output = shell_command(
            selected_shell.kind,
            &selected_shell.program,
            &request.command,
            &self.working_directory,
        )
        .with_context(|| {
            format!(
                "failed to run command via {} ({}): {}",
                selected_shell.kind.label(),
                selected_shell.program,
                request.command
            )
        })?;
        let finished_at = SystemTime::now();

        for path in tracked_paths {
            if path.exists() {
                self.watcher
                    .store()
                    .refresh_file(path, crate::context::ModifiedBy::Agent)?;
            }
        }

        let execution = CommandExecution {
            command: request.command.clone(),
            working_directory: self.working_directory.clone(),
            shell: selected_shell.kind,
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            started_at,
            finished_at,
        };

        self.history.turns.push(DisplayTurn {
            role: Role::Tool,
            content: format_tool_output(&execution),
            tool_calls: vec![ToolSummary {
                name: "run_command".to_string(),
                summary: format!("{} (exit code {})", execution.command, execution.exit_code),
            }],
            timestamp: finished_at,
        });

        Ok(execution)
    }

    pub fn history(&self) -> &ConversationHistory {
        &self.history
    }

    pub fn available_shells(&self) -> &[AvailableShell] {
        &self.available_shells
    }

    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    fn open_file_snapshots(&self) -> IndexMap<PathBuf, FileSnapshot> {
        self.watcher.store().snapshots()
    }

    fn resolve_shell(&self, shell: CommandShell) -> Result<AvailableShell> {
        self.available_shells
            .iter()
            .find(|candidate| candidate.kind == shell)
            .cloned()
            .with_context(|| {
                format!(
                    "requested shell {} is not available in current environment; available shells: {}",
                    shell.label(),
                    format_available_shells_for_error(&self.available_shells)
                )
            })
    }
}

/// 这里先用纯文本 prompt，把“上下文构建链路”跑通。
fn render_prompt(
    context: &AgentContext,
    _available_shells: &[AvailableShell],
    _working_directory: &Path,
) -> String {
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
    output.push_str(&render_tools_layer(&context.tools));
    output.push_str("\n\n");
    output.push_str(&render_prompt_body(context));
    output
}

fn render_prompt_body(context: &AgentContext) -> String {
    let mut output = String::new();
    output.push_str("# Open Files\n");

    for snapshot in context.open_files_in_prompt_order() {
        output.push_str(&format!(
            "## {}\nmodified_by={:?} changed={}\n{}\n\n",
            snapshot.path.display(),
            snapshot.last_modified_by,
            snapshot.has_changed_since_watch,
            snapshot.content
        ));
    }

    output.push_str("# Notes\n");
    if context.notes.is_empty() {
        output.push_str("(none)\n");
    } else {
        for (id, content) in &context.notes {
            output.push_str(&format!("{id}: {content}\n"));
        }
    }

    output.push_str("\n# Recent Chat\n");
    for turn in &context.recent_chat {
        output.push_str(&format!("{:?}: {}\n", turn.role, turn.content));
    }

    output
}

fn render_tools_layer(tools: &[ToolDefinition]) -> String {
    let runtime = ToolRuntime {
        tools: tools.to_vec(),
    };
    runtime.render_prompt_section()
}

/// Tool turn 目前保留命令、目录、退出码和 stdout/stderr，方便调试与展示；
/// 但这份内容只留在 display history，不进入 recent_chat。
fn format_tool_output(execution: &CommandExecution) -> String {
    let mut text = format!(
        "Command: {}\nShell: {:?}\nWorking directory: {}\nExit code: {}",
        execution.command,
        execution.shell,
        execution.working_directory.display(),
        execution.exit_code
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
    pub fn default_for_current_platform() -> Self {
        #[cfg(windows)]
        {
            Self::PowerShell
        }

        #[cfg(not(windows))]
        {
            Self::Sh
        }
    }

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
            Self::PowerShell => &["powershell", "pwsh"],
            Self::Cmd => &["cmd"],
        }
    }
}

/// 用调用方指定的 shell 执行命令，保持“命令文本”和“解释器环境”都在显式输入里。
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableShell {
    pub kind: CommandShell,
    pub program: String,
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
        anyhow::bail!("failed to detect any runnable shell in current PATH");
    }

    Ok(available)
}

fn resolve_shell_program(shell: CommandShell) -> Option<String> {
    shell
        .candidates()
        .iter()
        .find_map(|candidate| executable_in_path(candidate).map(|_| (*candidate).to_string()))
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
    let base = dir.join(program);
    let mut candidates = vec![base.clone()];

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
                vec![
                    OsString::from(".COM"),
                    OsString::from(".EXE"),
                    OsString::from(".BAT"),
                    OsString::from(".CMD"),
                ]
            })
    }

    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

fn format_available_shells_for_error(available_shells: &[AvailableShell]) -> String {
    available_shells
        .iter()
        .map(|shell| shell.kind.label())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_prompt_contains_context_layers() {
        let cwd = std::env::current_dir().expect("cwd");
        let history = ConversationHistory::new(vec![DisplayTurn {
            role: Role::User,
            content: "帮我读取 main.rs".to_string(),
            tool_calls: Vec::new(),
            timestamp: SystemTime::UNIX_EPOCH,
        }]);
        let mut session = AgentSession::new(
            AgentConfig::default(),
            history,
            [cwd.join("src").join("main.rs")],
        )
        .expect("session");
        session.add_injection("skill:test", "testing injection");
        session.write_note("target", "读取入口文件");

        let prompt = session.render_prompt();

        assert!(prompt.contains("# System Core"));
        assert!(prompt.contains("# Injections"));
        assert!(prompt.contains("## skill:test"));
        assert!(prompt.contains("# Tools"));
        assert!(prompt.contains("## run_command"));
        assert!(prompt.contains("# Open Files"));
        assert!(prompt.contains("# Notes"));
        assert!(prompt.contains("target: 读取入口文件"));
        assert!(prompt.contains("# Recent Chat"));
        assert!(prompt.contains("src\\main.rs") || prompt.contains("src/main.rs"));
        assert!(prompt.contains("帮我读取 main.rs"));
    }

    #[test]
    fn run_command_records_tool_turn() {
        let cwd = std::env::current_dir().expect("cwd");
        let mut session =
            AgentSession::new(AgentConfig::default(), ConversationHistory::new(vec![]), [])
                .expect("session");
        let (command, shell) = test_command_and_shell();

        let result = session
            .run_command(CommandRequest {
                command: command.to_string(),
                shell,
            })
            .expect("command");

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello from ma"));
        assert_eq!(result.shell, shell);
        assert_eq!(result.working_directory, cwd);
        assert_eq!(session.history().turns.len(), 1);
        assert_eq!(session.history().turns[0].role, Role::Tool);
    }

    #[test]
    fn build_context_excludes_tool_turns_from_recent_chat() {
        let mut session =
            AgentSession::new(AgentConfig::default(), ConversationHistory::new(vec![]), [])
                .expect("session");
        session.add_user_turn("first");
        session.add_assistant_turn("second", Vec::new());
        session.history.turns.push(DisplayTurn {
            role: Role::Tool,
            content: "tool".to_string(),
            tool_calls: Vec::new(),
            timestamp: SystemTime::now(),
        });

        let context = session.build_context();

        assert_eq!(context.recent_chat.len(), 2);
        assert_eq!(context.recent_chat[0].content, "first");
        assert_eq!(context.recent_chat[1].content, "second");
    }

    #[test]
    fn notes_support_overwrite_and_remove() {
        let mut session =
            AgentSession::new(AgentConfig::default(), ConversationHistory::new(vec![]), [])
                .expect("session");

        session.write_note("target", "first");
        session.write_note("target", "second");
        session.remove_note("target");

        assert!(session.build_context().notes.is_empty());
    }

    #[cfg(windows)]
    fn test_command_and_shell() -> (&'static str, CommandShell) {
        ("Write-Output 'hello from ma'", CommandShell::PowerShell)
    }

    #[cfg(not(windows))]
    fn test_command_and_shell() -> (&'static str, CommandShell) {
        ("printf 'hello from ma\\n'", CommandShell::Sh)
    }
}
