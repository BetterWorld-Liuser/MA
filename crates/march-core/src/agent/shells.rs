use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result, bail};
#[cfg(windows)]
use encoding_rs::GBK;
#[cfg(not(windows))]
use encoding_rs::UTF_8;
use encoding_rs::{CoderResult, Decoder, Encoding};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::MissedTickBehavior;

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

    let mut decoder = StreamOutputDecoder::new(command_output_encoding());
    decoder.push(bytes, true);
    decoder.finish()
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
    let shell_args: &[&str] = match shell {
        CommandShell::Sh | CommandShell::Bash => &["-lc", command],
        CommandShell::PowerShell => &["-NoProfile", "-Command", command],
        CommandShell::Cmd => &["/C", command],
    };

    let mut process = TokioCommand::new(program);
    process
        .args(shell_args)
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
    const CHILD_TERMINATION_WAIT_TIMEOUT: Duration = Duration::from_secs(2);
    const CHILD_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(50);

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, mut rx) = mpsc::unbounded_channel::<(StreamKind, Vec<u8>)>();
    let pipe_readers = PipeReaders {
        stdout_task: spawn_pipe_reader(stdout, StreamKind::Stdout, tx.clone()),
        stderr_task: spawn_pipe_reader(stderr, StreamKind::Stderr, tx),
    };
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut output_cache = OutputSnapshotCache::default();
    let mut timeout_sleep = Box::pin(tokio::time::sleep(timeout));
    let mut output_interval = tokio::time::interval(OUTPUT_STREAM_THROTTLE);
    let mut child_exit_poll = tokio::time::interval(CHILD_EXIT_POLL_INTERVAL);
    output_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    child_exit_poll.set_missed_tick_behavior(MissedTickBehavior::Skip);
    output_interval.tick().await;
    child_exit_poll.tick().await;
    let mut pending_output = false;

    let child_exit = loop {
        tokio::select! {
            _ = cancellation.cancelled() => {
                break ChildExit::Interrupted("turn cancelled".to_string());
            }
            _ = &mut timeout_sleep => {
                break ChildExit::Interrupted(format!(
                    "command timed out after {:.3}s (timeout {:.3}s)",
                    timeout.as_secs_f64(),
                    timeout.as_secs_f64(),
                ));
            }
            Some((stream, chunk)) = rx.recv() => {
                append_stream_chunk(stream, chunk, &mut stdout_bytes, &mut stderr_bytes);
                pending_output = true;
            }
            _ = child_exit_poll.tick() => {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        break ChildExit::Completed(status);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        return Err(error).context("failed to poll command completion");
                    }
                }
            }
            _ = output_interval.tick(), if pending_output => {
                emit_output_update(on_output, &mut output_cache, &stdout_bytes, &stderr_bytes, false)?;
                pending_output = false;
            }
        }
    };

    let terminate = matches!(child_exit, ChildExit::Interrupted(_));
    let shutdown_status = finalize_child_output(
        &mut child,
        &mut rx,
        pipe_readers,
        &mut stdout_bytes,
        &mut stderr_bytes,
        terminate,
        CHILD_TERMINATION_WAIT_TIMEOUT,
    )
    .await;
    append_shutdown_warnings(&mut stderr_bytes, terminate, shutdown_status);

    if pending_output
        || stdout_bytes.len() != output_cache.stdout_bytes_len
        || stderr_bytes.len() != output_cache.stderr_bytes_len
    {
        emit_output_update(
            on_output,
            &mut output_cache,
            &stdout_bytes,
            &stderr_bytes,
            true,
        )?;
    }

    match child_exit {
        ChildExit::Completed(status) => Ok(std::process::Output {
            status,
            stdout: stdout_bytes,
            stderr: stderr_bytes,
        }),
        ChildExit::Interrupted(reason) => Err(command_interruption_error(
            &reason,
            command,
            &stdout_bytes,
            &stderr_bytes,
        )),
    }
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
    stdout_decoder: StreamOutputDecoder,
    stderr_decoder: StreamOutputDecoder,
}

enum ChildExit {
    Completed(std::process::ExitStatus),
    Interrupted(String),
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

async fn drain_pipe_channel_until_closed(
    rx: &mut mpsc::UnboundedReceiver<(StreamKind, Vec<u8>)>,
    stdout_bytes: &mut Vec<u8>,
    stderr_bytes: &mut Vec<u8>,
) {
    while let Some((stream, chunk)) = rx.recv().await {
        append_stream_chunk(stream, chunk, stdout_bytes, stderr_bytes);
    }
}

fn emit_output_update(
    on_output: &mut impl FnMut(CommandOutputStreamUpdate) -> Result<()>,
    cache: &mut OutputSnapshotCache,
    stdout_bytes: &[u8],
    stderr_bytes: &[u8],
    final_chunk: bool,
) -> Result<()> {
    update_output_cache(
        stdout_bytes,
        &mut cache.stdout,
        &mut cache.stdout_bytes_len,
        &mut cache.stdout_decoder,
        final_chunk,
    );
    update_output_cache(
        stderr_bytes,
        &mut cache.stderr,
        &mut cache.stderr_bytes_len,
        &mut cache.stderr_decoder,
        final_chunk,
    );
    on_output(CommandOutputStreamUpdate {
        stdout: cache.stdout.clone(),
        stderr: cache.stderr.clone(),
    })
}

fn update_output_cache(
    bytes: &[u8],
    cached_text: &mut String,
    cached_len: &mut usize,
    decoder: &mut StreamOutputDecoder,
    final_chunk: bool,
) {
    if bytes.len() == *cached_len {
        if final_chunk {
            *cached_text = decoder.finish();
        }
        return;
    }

    // Reader tasks only append bytes, so we can safely feed just the unseen
    // suffix into a stateful decoder. This keeps throttled updates O(delta)
    // without breaking on chunk boundaries that split a multibyte character or
    // ANSI escape sequence.
    decoder.push(&bytes[*cached_len..], final_chunk);
    *cached_text = decoder.current_text().to_string();
    *cached_len = bytes.len();
}

async fn finalize_child_output(
    child: &mut Child,
    rx: &mut mpsc::UnboundedReceiver<(StreamKind, Vec<u8>)>,
    pipe_readers: PipeReaders,
    stdout_bytes: &mut Vec<u8>,
    stderr_bytes: &mut Vec<u8>,
    terminate: bool,
    wait_timeout: Duration,
) -> ShutdownStatus {
    let mut status = ShutdownStatus::default();

    if terminate {
        request_child_termination(child).await;
        if tokio::time::timeout(wait_timeout, child.wait())
            .await
            .is_err()
        {
            status.child_wait_timed_out = true;
        }
    }

    status.pipe_reader_join_timed_out =
        !join_pipe_readers_with_timeout(pipe_readers, wait_timeout).await;
    status.pipe_channel_drain_timed_out =
        !drain_pipe_channel_until_closed_with_timeout(rx, stdout_bytes, stderr_bytes, wait_timeout)
            .await;
    status
}

async fn join_pipe_readers_with_timeout(pipe_readers: PipeReaders, wait_timeout: Duration) -> bool {
    let PipeReaders {
        mut stdout_task,
        mut stderr_task,
    } = pipe_readers;
    let deadline = tokio::time::Instant::now() + wait_timeout;

    if !await_join_handle_until(&mut stdout_task, deadline).await {
        stderr_task.abort();
        let _ = stderr_task.await;
        return false;
    }

    if !await_join_handle_until(&mut stderr_task, deadline).await {
        return false;
    }

    true
}

async fn await_join_handle_until<T>(
    handle: &mut JoinHandle<Result<T, std::io::Error>>,
    deadline: tokio::time::Instant,
) -> bool {
    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if tokio::time::timeout(remaining, &mut *handle).await.is_ok() {
        return true;
    }

    handle.abort();
    let _ = handle.await;
    false
}

async fn drain_pipe_channel_until_closed_with_timeout(
    rx: &mut mpsc::UnboundedReceiver<(StreamKind, Vec<u8>)>,
    stdout_bytes: &mut Vec<u8>,
    stderr_bytes: &mut Vec<u8>,
    wait_timeout: Duration,
) -> bool {
    if tokio::time::timeout(
        wait_timeout,
        drain_pipe_channel_until_closed(rx, stdout_bytes, stderr_bytes),
    )
    .await
    .is_ok()
    {
        return true;
    }

    while let Ok((stream, chunk)) = rx.try_recv() {
        append_stream_chunk(stream, chunk, stdout_bytes, stderr_bytes);
    }
    false
}

async fn request_child_termination(child: &mut Child) {
    #[cfg(windows)]
    {
        request_child_process_tree_termination_windows(child).await;
    }

    let _ = child.start_kill();
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ShutdownStatus {
    child_wait_timed_out: bool,
    pipe_reader_join_timed_out: bool,
    pipe_channel_drain_timed_out: bool,
}

fn append_shutdown_warnings(
    stderr_bytes: &mut Vec<u8>,
    terminated_early: bool,
    status: ShutdownStatus,
) {
    if !terminated_early {
        return;
    }

    if status.child_wait_timed_out {
        append_shutdown_warning(
            stderr_bytes,
            "March cleanup warning: timed out while waiting for the shell process to exit after interruption.",
        );
    }
    if status.pipe_reader_join_timed_out {
        append_shutdown_warning(
            stderr_bytes,
            "March cleanup warning: timed out while waiting for command output readers to finish after interruption.",
        );
    }
    if status.pipe_channel_drain_timed_out {
        append_shutdown_warning(
            stderr_bytes,
            "March cleanup warning: timed out while draining remaining command output after interruption.",
        );
    }
}

fn append_shutdown_warning(stderr_bytes: &mut Vec<u8>, warning: &str) {
    if stderr_bytes.ends_with(b"\n") || stderr_bytes.is_empty() {
        stderr_bytes.extend_from_slice(warning.as_bytes());
        stderr_bytes.push(b'\n');
        return;
    }

    stderr_bytes.push(b'\n');
    stderr_bytes.extend_from_slice(warning.as_bytes());
    stderr_bytes.push(b'\n');
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

struct StreamOutputDecoder {
    decoder: Option<Decoder>,
    ansi_stripper: IncrementalAnsiStripper,
    text: String,
}

impl Default for StreamOutputDecoder {
    fn default() -> Self {
        Self::new(command_output_encoding())
    }
}

impl StreamOutputDecoder {
    fn new(encoding: &'static Encoding) -> Self {
        Self {
            decoder: Some(encoding.new_decoder_without_bom_handling()),
            ansi_stripper: IncrementalAnsiStripper::default(),
            text: String::new(),
        }
    }

    fn current_text(&self) -> &str {
        self.text.trim()
    }

    fn push(&mut self, bytes: &[u8], is_last: bool) {
        if bytes.is_empty() && !is_last {
            return;
        }

        let sanitized = self.ansi_stripper.push(bytes, is_last);
        if sanitized.is_empty() && !is_last {
            return;
        }

        let decoded = self.decode_sanitized_bytes(&sanitized, is_last, sanitized.len().max(32));
        self.text.push_str(&decoded);

        if is_last {
            self.decoder = None;
        }
    }

    fn finish(&mut self) -> String {
        if self.decoder.is_some() {
            self.push(&[], true);
        }
        self.current_text().to_string()
    }

    fn decode_sanitized_bytes(
        &mut self,
        sanitized: &[u8],
        is_last: bool,
        initial_capacity: usize,
    ) -> String {
        let mut decoded = String::new();
        decoded.reserve(initial_capacity);
        if let Some(decoder) = self.decoder.as_mut() {
            let mut remaining = sanitized;
            loop {
                let (result, read, _) = decoder.decode_to_string(remaining, &mut decoded, is_last);
                remaining = &remaining[read..];
                match result {
                    CoderResult::InputEmpty => break,
                    CoderResult::OutputFull => {
                        // `encoding_rs` writes into the provided String buffer. When the
                        // buffer has no spare capacity, it can report `OutputFull` without
                        // consuming input. We must grow the buffer here, otherwise this
                        // loop can spin forever on small stdout chunks in release builds.
                        decoded.reserve(remaining.len().max(32));
                        continue;
                    }
                }
            }
        }
        decoded
    }
}

fn command_output_encoding() -> &'static Encoding {
    #[cfg(windows)]
    {
        GBK
    }

    #[cfg(not(windows))]
    {
        UTF_8
    }
}

#[derive(Default)]
struct IncrementalAnsiStripper {
    state: AnsiState,
}

#[derive(Default)]
enum AnsiState {
    #[default]
    Ground,
    Escape,
    Csi,
    Osc,
    OscEscape,
    EscapeSequence,
}

impl IncrementalAnsiStripper {
    fn push(&mut self, bytes: &[u8], is_last: bool) -> Vec<u8> {
        let mut output = Vec::with_capacity(bytes.len());
        for &byte in bytes {
            match self.state {
                AnsiState::Ground => match byte {
                    0x1B => self.state = AnsiState::Escape,
                    b'\n' | b'\t' => output.push(byte),
                    _ if !(byte as char).is_ascii_control() => output.push(byte),
                    _ => {}
                },
                AnsiState::Escape => match byte {
                    b'[' => self.state = AnsiState::Csi,
                    b']' => self.state = AnsiState::Osc,
                    _ => self.state = AnsiState::EscapeSequence,
                },
                AnsiState::Csi => {
                    if (0x40..=0x7E).contains(&byte) {
                        self.state = AnsiState::Ground;
                    }
                }
                AnsiState::Osc => match byte {
                    0x07 => self.state = AnsiState::Ground,
                    0x1B => self.state = AnsiState::OscEscape,
                    _ => {}
                },
                AnsiState::OscEscape => {
                    self.state = if byte == b'\\' {
                        AnsiState::Ground
                    } else {
                        AnsiState::Osc
                    };
                }
                AnsiState::EscapeSequence => {
                    self.state = AnsiState::Ground;
                }
            }
        }

        if is_last {
            self.state = AnsiState::Ground;
        }

        output
    }
}

#[cfg(test)]
mod tests {
    use std::{future::pending, process::Stdio, time::Instant};

    use tokio::{process::Command as TokioCommand, runtime::Runtime, sync::mpsc};

    use super::{
        PipeReaders, ShutdownStatus, StreamKind, append_shutdown_warnings, finalize_child_output,
    };

    #[test]
    fn finalize_child_output_stops_waiting_when_pipe_readers_never_finish() {
        let runtime = Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let mut child = if cfg!(windows) {
                TokioCommand::new("powershell")
                    .args(["-NoProfile", "-Command", "exit 0"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("spawn powershell")
            } else {
                TokioCommand::new("sh")
                    .args(["-lc", "exit 0"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .expect("spawn sh")
            };

            let (tx, mut rx) = mpsc::unbounded_channel::<(StreamKind, Vec<u8>)>();
            let stdout_task = tokio::spawn(async move {
                let _keep_sender_alive = tx;
                pending::<Result<(), std::io::Error>>().await
            });
            let stderr_task = tokio::spawn(async { pending::<Result<(), std::io::Error>>().await });
            let pipe_readers = PipeReaders {
                stdout_task,
                stderr_task,
            };
            let mut stdout_bytes = Vec::new();
            let mut stderr_bytes = Vec::new();
            let started = Instant::now();

            let status = finalize_child_output(
                &mut child,
                &mut rx,
                pipe_readers,
                &mut stdout_bytes,
                &mut stderr_bytes,
                false,
                std::time::Duration::from_millis(100),
            )
            .await;

            assert!(started.elapsed() < std::time::Duration::from_secs(1));
            assert_eq!(
                status,
                ShutdownStatus {
                    child_wait_timed_out: false,
                    pipe_reader_join_timed_out: true,
                    pipe_channel_drain_timed_out: false,
                }
            );
        });
    }

    #[test]
    fn append_shutdown_warnings_adds_human_readable_cleanup_notes() {
        let mut stderr_bytes = b"Partial stderr".to_vec();

        append_shutdown_warnings(
            &mut stderr_bytes,
            true,
            ShutdownStatus {
                child_wait_timed_out: true,
                pipe_reader_join_timed_out: true,
                pipe_channel_drain_timed_out: false,
            },
        );

        let stderr = String::from_utf8(stderr_bytes).expect("valid utf8");
        assert!(stderr.contains("Partial stderr"));
        assert!(stderr.contains("timed out while waiting for the shell process to exit"));
        assert!(stderr.contains("timed out while waiting for command output readers to finish"));
    }

    #[test]
    fn stream_output_decoder_grows_buffer_when_decoder_reports_output_full() {
        let mut decoder = super::StreamOutputDecoder::new(super::command_output_encoding());
        let decoded = decoder.decode_sanitized_bytes(b"abc", false, 0);

        assert_eq!(decoded, "abc");
    }
}
