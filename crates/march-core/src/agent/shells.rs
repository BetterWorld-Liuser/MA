use std::env;
use std::ffi::OsString;
use std::future::pending;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use encoding_rs::GBK;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::agent::{CommandOutputStreamUpdate, TurnCancellation};

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
            return strip_ansi_control_sequences(decoded.trim());
        }
    }

    strip_ansi_control_sequences(String::from_utf8_lossy(bytes).trim())
}

fn strip_ansi_control_sequences(input: &str) -> String {
    let stripped = strip_ansi_escapes::strip(input.as_bytes());
    String::from_utf8_lossy(&stripped)
        .chars()
        .filter(|ch| !ch.is_control() || matches!(ch, '\n' | '\t'))
        .collect()
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
    on_output: &mut impl FnMut(CommandOutputStreamUpdate) -> Result<()>,
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

    collect_child_output(child, command, timeout, cancellation, on_output).await
}

async fn collect_child_output(
    mut child: Child,
    command: &str,
    timeout: Duration,
    cancellation: &TurnCancellation,
    on_output: &mut impl FnMut(CommandOutputStreamUpdate) -> Result<()>,
) -> Result<std::process::Output> {
    const OUTPUT_STREAM_THROTTLE: Duration = Duration::from_millis(50);

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, mut rx) = mpsc::unbounded_channel::<(StreamKind, Vec<u8>)>();
    let mut pipe_readers = Some(PipeReaders {
        stdout_task: spawn_pipe_reader(stdout, StreamKind::Stdout, tx.clone()),
        stderr_task: spawn_pipe_reader(stderr, StreamKind::Stderr, tx),
    });
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut output_cache = OutputSnapshotCache::default();
    let mut timeout_sleep = Box::pin(tokio::time::sleep(timeout));
    let mut output_flush = None;
    let mut pending_output = false;
    let status = loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                finalize_child_output(
                    &mut child,
                    &mut rx,
                    pipe_readers.take().expect("pipe readers should exist until finalized"),
                    &mut stdout_bytes,
                    &mut stderr_bytes,
                    true,
                ).await;
                return Err(command_interruption_error("turn cancelled", command, &stdout_bytes, &stderr_bytes));
            }
            _ = &mut timeout_sleep => {
                finalize_child_output(
                    &mut child,
                    &mut rx,
                    pipe_readers.take().expect("pipe readers should exist until finalized"),
                    &mut stdout_bytes,
                    &mut stderr_bytes,
                    true,
                ).await;
                return Err(command_interruption_error(
                    &format!(
                        "command timed out after {:.3}s (timeout {:.3}s)",
                        timeout.as_secs_f64(),
                        timeout.as_secs_f64(),
                    ),
                    command,
                    &stdout_bytes,
                    &stderr_bytes,
                ));
            }
            status = child.wait() => {
                break status.context("failed to wait for command completion")?;
            }
            Some((stream, chunk)) = rx.recv() => {
                append_stream_chunk(stream, chunk, &mut stdout_bytes, &mut stderr_bytes);
                if !pending_output {
                    output_flush = Some(Box::pin(tokio::time::sleep(OUTPUT_STREAM_THROTTLE)));
                    pending_output = true;
                }
            }
            _ = async {
                if let Some(flush) = output_flush.as_mut() {
                    flush.await;
                } else {
                    pending::<()>().await;
                }
            }, if pending_output => {
                emit_output_update(on_output, &mut output_cache, &stdout_bytes, &stderr_bytes)?;
                pending_output = false;
                output_flush = None;
            }
        }
    };

    finalize_child_output(
        &mut child,
        &mut rx,
        pipe_readers
            .take()
            .expect("pipe readers should exist until finalized"),
        &mut stdout_bytes,
        &mut stderr_bytes,
        false,
    )
    .await;
    if pending_output
        || stdout_bytes.len() != output_cache.stdout_bytes_len
        || stderr_bytes.len() != output_cache.stderr_bytes_len
    {
        emit_output_update(on_output, &mut output_cache, &stdout_bytes, &stderr_bytes)?;
    }

    Ok(std::process::Output {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    })
}

#[derive(Debug, Clone, Copy)]
enum StreamKind {
    Stdout,
    Stderr,
}

struct PipeReaders {
    stdout_task: JoinHandle<Result<(), std::io::Error>>,
    stderr_task: JoinHandle<Result<(), std::io::Error>>,
}

#[derive(Default)]
struct OutputSnapshotCache {
    stdout: String,
    stderr: String,
    stdout_bytes_len: usize,
    stderr_bytes_len: usize,
}

fn spawn_pipe_reader<R>(
    reader: Option<R>,
    stream: StreamKind,
    sender: mpsc::UnboundedSender<(StreamKind, Vec<u8>)>,
) -> tokio::task::JoinHandle<Result<(), std::io::Error>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let Some(mut reader) = reader else {
            return Ok(());
        };

        let mut buffer = [0u8; 4096];
        loop {
            let read = reader.read(&mut buffer).await?;
            if read == 0 {
                break;
            }
            if sender.send((stream, buffer[..read].to_vec())).is_err() {
                break;
            }
        }
        Ok(())
    })
}

fn append_stream_chunk(
    stream: StreamKind,
    chunk: Vec<u8>,
    stdout_bytes: &mut Vec<u8>,
    stderr_bytes: &mut Vec<u8>,
) {
    match stream {
        StreamKind::Stdout => stdout_bytes.extend_from_slice(&chunk),
        StreamKind::Stderr => stderr_bytes.extend_from_slice(&chunk),
    }
}

fn drain_pipe_channel(
    rx: &mut mpsc::UnboundedReceiver<(StreamKind, Vec<u8>)>,
    stdout_bytes: &mut Vec<u8>,
    stderr_bytes: &mut Vec<u8>,
) {
    while let Ok((stream, chunk)) = rx.try_recv() {
        append_stream_chunk(stream, chunk, stdout_bytes, stderr_bytes);
    }
}

fn emit_output_update(
    on_output: &mut impl FnMut(CommandOutputStreamUpdate) -> Result<()>,
    cache: &mut OutputSnapshotCache,
    stdout_bytes: &[u8],
    stderr_bytes: &[u8],
) -> Result<()> {
    update_output_cache(stdout_bytes, &mut cache.stdout, &mut cache.stdout_bytes_len);
    update_output_cache(stderr_bytes, &mut cache.stderr, &mut cache.stderr_bytes_len);
    on_output(CommandOutputStreamUpdate {
        stdout: cache.stdout.clone(),
        stderr: cache.stderr.clone(),
    })
}

fn update_output_cache(bytes: &[u8], cached_text: &mut String, cached_len: &mut usize) {
    if bytes.len() < *cached_len {
        *cached_text = decode_command_output(bytes);
        *cached_len = bytes.len();
        return;
    }

    if bytes.len() == *cached_len {
        return;
    }

    // Stream updates are preview snapshots. We incrementally decode only the
    // newly appended bytes here so long-running commands do not repeatedly
    // re-decode the full accumulated buffer on every throttle tick. The final
    // command result still decodes from the full raw buffers in one pass.
    cached_text.push_str(&decode_command_output(&bytes[*cached_len..]));
    *cached_len = bytes.len();
}

async fn finalize_child_output(
    child: &mut Child,
    rx: &mut mpsc::UnboundedReceiver<(StreamKind, Vec<u8>)>,
    pipe_readers: PipeReaders,
    stdout_bytes: &mut Vec<u8>,
    stderr_bytes: &mut Vec<u8>,
    terminate: bool,
) {
    if terminate {
        request_child_termination(child).await;
        let _ = child.wait().await;
    }

    drain_pipe_channel(rx, stdout_bytes, stderr_bytes);
    join_pipe_readers(pipe_readers).await;
    drain_pipe_channel(rx, stdout_bytes, stderr_bytes);
}

async fn join_pipe_readers(pipe_readers: PipeReaders) {
    let _ = pipe_readers.stdout_task.await;
    let _ = pipe_readers.stderr_task.await;
}

async fn request_child_termination(child: &mut Child) {
    #[cfg(windows)]
    {
        request_child_process_tree_termination_windows(child).await;
    }

    let _ = child.start_kill();
}

#[cfg(windows)]
async fn request_child_process_tree_termination_windows(child: &Child) {
    const TASKKILL_TIMEOUT: Duration = Duration::from_secs(2);

    let Some(pid) = child.id() else {
        return;
    };

    let mut killer = TokioCommand::new("taskkill");
    killer
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = tokio::time::timeout(TASKKILL_TIMEOUT, killer.status()).await;
}

fn command_interruption_error(
    reason: &str,
    command: &str,
    stdout_bytes: &[u8],
    stderr_bytes: &[u8],
) -> anyhow::Error {
    anyhow::anyhow!(format_command_interruption(
        reason,
        command,
        &decode_command_output(stdout_bytes),
        &decode_command_output(stderr_bytes),
    ))
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
