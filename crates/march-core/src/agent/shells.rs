use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use encoding_rs::GBK;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command as TokioCommand};

use crate::agent::TurnCancellation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandShell {
    Sh,
    Bash,
    PowerShell,
    Cmd,
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

pub fn decode_command_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
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

pub fn platform_label() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    }
}

pub fn workspace_entries(working_directory: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(working_directory) else {
        return Vec::new();
    };

    let mut entries = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let file_type = entry.file_type().ok()?;
            let mut name = entry.file_name().to_string_lossy().to_string();
            if file_type.is_dir() {
                name.push('/');
            }
            Some((file_type.is_dir(), name))
        })
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.to_lowercase().cmp(&right.1.to_lowercase()))
            .then_with(|| left.1.cmp(&right.1))
    });

    entries.into_iter().map(|(_, name)| name).collect()
}

pub async fn shell_command_with_cancel(
    shell: CommandShell,
    program: &str,
    command: &str,
    working_directory: &Path,
    timeout: Duration,
    cancellation: &TurnCancellation,
) -> Result<std::process::Output> {
    let mut process = match shell {
        CommandShell::Sh => {
            let mut process = TokioCommand::new(program);
            process.args(["-lc", command]);
            process
        }
        CommandShell::Bash => {
            let mut process = TokioCommand::new(program);
            process.args(["-lc", command]);
            process
        }
        CommandShell::PowerShell => {
            let mut process = TokioCommand::new(program);
            process.args(["-NoProfile", "-Command", command]);
            process
        }
        CommandShell::Cmd => {
            let mut process = TokioCommand::new(program);
            process.args(["/C", command]);
            process
        }
    };

    process
        .current_dir(working_directory)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = process
        .spawn()
        .with_context(|| format!("failed to spawn {}", shell.label()))?;

    collect_child_output(child, command, timeout, cancellation).await
}

async fn collect_child_output(
    mut child: Child,
    command: &str,
    timeout: Duration,
    cancellation: &TurnCancellation,
) -> Result<std::process::Output> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let stdout_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        if let Some(mut stdout) = stdout {
            stdout.read_to_end(&mut bytes).await?;
        }
        Ok::<Vec<u8>, std::io::Error>(bytes)
    });
    let stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        if let Some(mut stderr) = stderr {
            stderr.read_to_end(&mut bytes).await?;
        }
        Ok::<Vec<u8>, std::io::Error>(bytes)
    });

    let status = tokio::select! {
        _ = cancellation.cancelled() => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let (stdout, stderr) = collect_terminated_output(stdout_task, stderr_task).await;
            return Err(anyhow::anyhow!(format_command_interruption(
                "turn cancelled",
                command,
                &stdout,
                &stderr,
            )));
        }
        _ = tokio::time::sleep(timeout) => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let (stdout, stderr) = collect_terminated_output(stdout_task, stderr_task).await;
            return Err(anyhow::anyhow!(format_command_interruption(
                &format!(
                    "command timed out after {:.3}s (timeout {:.3}s)",
                    timeout.as_secs_f64(),
                    timeout.as_secs_f64(),
                ),
                command,
                &stdout,
                &stderr,
            )));
        }
        status = child.wait() => status.context("failed to wait for command completion")?,
    };

    let stdout = join_output(stdout_task, "stdout").await?;
    let stderr = join_output(stderr_task, "stderr").await?;

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

async fn collect_terminated_output(
    stdout_task: tokio::task::JoinHandle<Result<Vec<u8>, std::io::Error>>,
    stderr_task: tokio::task::JoinHandle<Result<Vec<u8>, std::io::Error>>,
) -> (String, String) {
    let stdout = match join_output(stdout_task, "stdout").await {
        Ok(bytes) => decode_command_output(&bytes),
        Err(error) => format!("[failed to collect stdout: {error}]"),
    };
    let stderr = match join_output(stderr_task, "stderr").await {
        Ok(bytes) => decode_command_output(&bytes),
        Err(error) => format!("[failed to collect stderr: {error}]"),
    };
    (stdout, stderr)
}

fn format_command_interruption(reason: &str, command: &str, stdout: &str, stderr: &str) -> String {
    let mut message = format!("{reason}: {command}");
    if !stdout.is_empty() {
        message.push_str("\nPartial stdout:\n");
        message.push_str(stdout);
    }
    if !stderr.is_empty() {
        message.push_str("\nPartial stderr:\n");
        message.push_str(stderr);
    }
    message
}

async fn join_output(
    task: tokio::task::JoinHandle<Result<Vec<u8>, std::io::Error>>,
    stream_name: &str,
) -> Result<Vec<u8>> {
    let bytes = task
        .await
        .context(format!("{stream_name} reader task failed to join"))??;
    Ok(bytes)
}

pub fn detect_available_shells() -> Result<Vec<AvailableShell>> {
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

pub fn parse_shell(shell: &str) -> Result<CommandShell> {
    match shell {
        "sh" => Ok(CommandShell::Sh),
        "bash" => Ok(CommandShell::Bash),
        "powershell" | "pwsh" => Ok(CommandShell::PowerShell),
        "cmd" => Ok(CommandShell::Cmd),
        other => bail!("unsupported shell {}", other),
    }
}

pub fn resolve_shell_program_with<L, P>(
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
        probe_program(shell, &executable).then(|| (*candidate).to_string())
    })
}

fn resolve_shell_program(shell: CommandShell) -> Option<String> {
    resolve_shell_program_with(shell, executable_in_path, shell_probe_succeeds)
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
