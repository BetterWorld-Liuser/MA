use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::SystemTime;

use anyhow::{Context, Result};

use crate::context::{
    AgentContext, AgentContextBuilder, ContextBuildConfig, ConversationHistory, DisplayTurn, Role,
    ToolSummary,
};
use crate::provider::OpenAiCompatibleClient;
use crate::watcher::FileWatcherService;

/// AgentSession 是当前阶段的主编排对象：
/// 它把历史、watcher、命令执行和模型调用收拢到同一个会话里。
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub system_prompt: String,
    pub max_recent_turns: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_prompt:
                "You are Ma, an agentic coding assistant whose source of truth is the filesystem."
                    .to_string(),
            max_recent_turns: 6,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandRequest {
    pub command: String,
    pub working_directory: PathBuf,
    pub shell: Option<CommandShell>,
    pub touched_files: Vec<PathBuf>,
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

pub struct AgentSession {
    config: AgentConfig,
    watcher: FileWatcherService,
    history: ConversationHistory,
    available_shells: Vec<AvailableShell>,
}

impl AgentSession {
    /// 会话初始化时先建立 watcher，这样后续所有 prompt 都能基于同一份实时文件视图构建。
    pub fn new(
        config: AgentConfig,
        history: ConversationHistory,
        watched_files: impl IntoIterator<Item = PathBuf>,
    ) -> Result<Self> {
        let mut watcher = FileWatcherService::new()?;
        for path in watched_files {
            watcher.watch_file(path)?;
        }

        Ok(Self {
            config,
            watcher,
            history,
            available_shells: detect_available_shells()?,
        })
    }

    pub fn watch_file(&mut self, path: impl Into<PathBuf>) -> Result<()> {
        self.watcher.watch_file(path.into())?;
        Ok(())
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

    pub fn build_context(&self) -> AgentContext {
        AgentContextBuilder::new(self.config.system_prompt.clone())
            .with_config(ContextBuildConfig {
                max_recent_turns: self.config.max_recent_turns,
            })
            .history(self.history.clone())
            .build_from_snapshots(self.watcher.store().snapshots())
    }

    pub fn render_prompt(&self) -> String {
        let context = self.build_context();
        render_prompt(&context, &self.available_shells)
    }

    pub async fn generate_assistant_reply(
        &mut self,
        client: &OpenAiCompatibleClient,
    ) -> Result<String> {
        // 目前的最小模型调用路径：
        // 直接把当前上下文渲染成 prompt 发给兼容接口，再把返回文本记录成 assistant turn。
        let context = self.build_context();
        let prompt = render_prompt(&context, &self.available_shells);
        let reply = client.complete_text(&context.system, &prompt).await?;

        self.add_assistant_turn(
            reply.clone(),
            vec![ToolSummary {
                name: "chat.completions".to_string(),
                summary: "generated assistant reply from OpenAI-compatible provider".to_string(),
            }],
        );

        Ok(reply)
    }

    /// 当前暴露通用 run_command，并允许调用方显式声明命令运行在哪个 shell 环境里。
    /// 这里会把命令执行、agent 写入归因、watcher 刷新和 tool turn 记录一次串起来。
    pub fn run_command(&mut self, request: CommandRequest) -> Result<CommandExecution> {
        let started_at = SystemTime::now();
        let selected_shell = request
            .shell
            .map(|shell| self.resolve_shell(shell))
            .transpose()?
            .unwrap_or_else(|| self.default_shell());
        let touched_files = request
            .touched_files
            .iter()
            .map(|path| absolutize(path, &request.working_directory))
            .collect::<Result<Vec<_>>>()?;

        for path in &touched_files {
            if !self.watcher.store().snapshots().contains_key(path) && path.exists() {
                self.watch_file(path.clone())?;
            }
        }

        let _agent_write_guard = self
            .watcher
            .store()
            .begin_agent_write(touched_files.clone())?;
        // 真正的命令执行保持平台相关逻辑在 shell_command 里，
        // session 只关心“命令跑了”和“跑完后文件真实状态是什么”。
        let output = shell_command(
            selected_shell.kind,
            &selected_shell.program,
            &request.command,
            &request.working_directory,
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

        for path in touched_files {
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

        let execution = CommandExecution {
            command: request.command.clone(),
            working_directory: request.working_directory.clone(),
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

    fn resolve_shell(&self, shell: CommandShell) -> Result<AvailableShell> {
        self.available_shells
            .iter()
            .find(|candidate| candidate.kind == shell)
            .cloned()
            .with_context(|| {
                format!(
                    "requested shell {} is not available in current environment; available shells: {}",
                    shell.label(),
                    format_available_shells(&self.available_shells)
                )
            })
    }

    fn default_shell(&self) -> AvailableShell {
        self.available_shells
            .iter()
            .find(|shell| shell.kind == CommandShell::default_for_current_platform())
            .cloned()
            .or_else(|| self.available_shells.first().cloned())
            .expect("session must have at least one available shell")
    }
}

/// 这里先用纯文本 prompt，目的是快速把“上下文构建链路”跑通。
/// 后面如果切到更结构化的输入格式，也应保留这个函数作为单一出口。
fn render_prompt(context: &AgentContext, available_shells: &[AvailableShell]) -> String {
    let mut output = String::new();
    output.push_str("# System\n");
    output.push_str(&context.system);
    output.push_str("\n\n# Tooling\n");
    output.push_str(&render_tooling_instructions(available_shells));
    output.push_str("\n\n# Watched Files\n");

    for snapshot in context.watched_files_in_prompt_order() {
        output.push_str(&format!(
            "## {}\nmodified_by={:?} changed={}\n{}\n\n",
            snapshot.path.display(),
            snapshot.last_modified_by,
            snapshot.has_changed_since_watch,
            snapshot.content
        ));
    }

    output.push_str("# Messages\n");
    for message in &context.messages {
        output.push_str(&format!("{:?}: {}\n", message.role, message.content));
    }

    output
}

/// Tool turn 目前保留命令、目录、退出码和 stdout/stderr，
/// 既方便调试，也方便后续做更细粒度的 tool summary。
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
/// 这里不尝试解析命令语义，命令本身由上层或模型决定。
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

fn render_tooling_instructions(available_shells: &[AvailableShell]) -> String {
    let default_shell = available_shells
        .iter()
        .find(|shell| shell.kind == CommandShell::default_for_current_platform())
        .unwrap_or(&available_shells[0]);

    let mut text = String::new();
    text.push_str("run_command shells are discovered from the current environment at session start.\n");
    text.push_str(&format!(
        "Available shells: {}\n",
        format_available_shells(available_shells)
    ));
    text.push_str(&format!(
        "Default shell: {}\n",
        default_shell.kind.label()
    ));
    text.push_str(
        "When calling run_command, choose the shell that matches the command syntax and only select from the available shells above.",
    );
    text
}

fn format_available_shells(available_shells: &[AvailableShell]) -> String {
    available_shells
        .iter()
        .map(|shell| {
            if shell.program == shell.kind.label() {
                shell.kind.label().to_string()
            } else {
                format!("{} ({})", shell.kind.label(), shell.program)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// touched_files 既可能来自相对路径，也可能来自绝对路径。
/// 统一绝对化后，watcher 和 context 层才不会把同一个文件视为不同实体。
fn absolutize(path: &Path, working_directory: &Path) -> Result<PathBuf> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_directory.join(path)
    };

    if candidate.exists() {
        candidate
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", candidate.display()))
    } else {
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_prompt_contains_watched_files_and_messages() {
        let cwd = std::env::current_dir().expect("cwd");
        let history = ConversationHistory::new(vec![DisplayTurn {
            role: Role::User,
            content: "帮我读取 main.rs".to_string(),
            tool_calls: Vec::new(),
            timestamp: SystemTime::UNIX_EPOCH,
        }]);
        let session = AgentSession::new(
            AgentConfig::default(),
            history,
            [cwd.join("src").join("main.rs")],
        )
        .expect("session");

        let prompt = session.render_prompt();

        assert!(prompt.contains("# Tooling"));
        assert!(prompt.contains("Available shells:"));
        assert!(prompt.contains("# Watched Files"));
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
                working_directory: cwd,
                shell: Some(shell),
                touched_files: Vec::new(),
            })
            .expect("command");

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello from ma"));
        assert_eq!(result.shell, shell);
        assert_eq!(session.history().turns.len(), 1);
        assert_eq!(session.history().turns[0].role, Role::Tool);
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
