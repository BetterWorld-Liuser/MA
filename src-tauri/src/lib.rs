use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use march::agent::TurnCancellation;
use serde::Serialize;
use serde_json::Value;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

use march::ui::{
    UiAgentProgressEvent, UiAppBackend, UiCloseOpenFileRequest, UiCreateTaskRequest,
    UiDeleteAgentRequest, UiDeleteMemoryRequest, UiDeleteNoteRequest, UiDeleteProviderModelRequest,
    UiDeleteProviderRequest, UiDeleteTaskRequest, UiGetMemoryRequest, UiListMemoriesRequest,
    UiLoadWorkspaceImageRequest, UiMemoryDetailView, UiMentionTargetView, UiOpenFilesRequest,
    UiProbeProviderModelCapabilitiesRequest, UiProbeProviderModelCapabilitiesView,
    UiProbeProviderModelsRequest, UiProviderModelsView, UiProviderSettingsView,
    UiRestoreMarchPromptRequest, UiSearchSkillsRequest, UiSearchWorkspaceEntriesRequest,
    UiSelectTaskRequest, UiSendMessageRequest, UiSetDefaultModelRequest, UiSetTaskModelRequest,
    UiSetTaskModelSettingsRequest, UiSetTaskWorkingDirectoryRequest, UiSkillSearchView,
    UiTaskHistoryView, UiTaskModelSelectorView, UiTestProviderConnectionRequest,
    UiTestProviderConnectionResult,
    UiToggleOpenFileLockRequest, UiUpsertAgentRequest, UiUpsertMemoryRequest, UiUpsertNoteRequest,
    UiUpsertProviderModelRequest, UiUpsertProviderRequest, UiWorkspaceEntryView,
    UiWorkspaceImageView, UiWorkspaceSnapshot, fetch_probe_model_capabilities, fetch_probe_models,
    fetch_provider_models_for_provider, fetch_task_model_selector,
    test_provider_connection as run_provider_connection_test,
};

struct AppState {
    workspace_path: PathBuf,
    cancellations: Mutex<HashMap<i64, Arc<TurnCancellation>>>,
    turn_cancellations: Mutex<HashMap<String, Arc<TurnCancellation>>>,
    in_flight_turns: Mutex<HashMap<i64, String>>,
    working_turn_counts: Mutex<HashMap<i64, usize>>,
    event_sequences: Mutex<HashMap<i64, u64>>,
    event_buffers: Mutex<HashMap<i64, VecDeque<UiAgentProgressEvent>>>,
    memory_watcher: Mutex<MemoryWatcherState>,
}

struct MemoryWatcherState {
    watcher: notify::RecommendedWatcher,
    watched_dirs: HashSet<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackendNoticePayload {
    level: String,
    source: String,
    message: String,
    timestamp: i64,
}

#[derive(Debug, Clone, Serialize)]
struct TaskWorkingChangedPayload {
    task_id: i64,
    working: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum TaskSubscriptionStatus {
    Subscribed,
    GapTooLarge,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskSubscriptionView {
    status: TaskSubscriptionStatus,
}

impl MemoryWatcherState {
    fn sync_dirs(&mut self, dirs: &[PathBuf]) -> anyhow::Result<()> {
        use notify::{RecursiveMode, Watcher};

        let desired = dirs.iter().cloned().collect::<HashSet<_>>();
        for path in self
            .watched_dirs
            .difference(&desired)
            .cloned()
            .collect::<Vec<_>>()
        {
            self.watcher
                .unwatch(&path)
                .with_context(|| format!("failed to unwatch {}", path.display()))?;
            self.watched_dirs.remove(&path);
        }

        for path in desired
            .difference(&self.watched_dirs)
            .cloned()
            .collect::<Vec<_>>()
        {
            std::fs::create_dir_all(&path)
                .with_context(|| format!("failed to create {}", path.display()))?;
            self.watcher
                .watch(&path, RecursiveMode::Recursive)
                .with_context(|| format!("failed to watch {}", path.display()))?;
            self.watched_dirs.insert(path);
        }

        Ok(())
    }
}

fn collect_memory_dirs(workspace_path: &std::path::Path) -> anyhow::Result<Vec<PathBuf>> {
    let backend = UiAppBackend::open(workspace_path.to_path_buf())?;
    let mut dirs = backend
        .task_working_directories()?
        .into_iter()
        .map(|path| path.join(".march").join("memories"))
        .collect::<Vec<_>>();
    dirs.push(workspace_path.join(".march").join("memories"));
    dirs.sort();
    dirs.dedup();
    Ok(dirs)
}

fn sync_memory_watcher(state: &AppState) -> anyhow::Result<()> {
    let dirs = collect_memory_dirs(&state.workspace_path)?;
    let mut watcher = state
        .memory_watcher
        .lock()
        .map_err(|_| anyhow::anyhow!("failed to acquire memory watcher"))?;
    watcher.sync_dirs(&dirs)
}

fn build_memory_watcher(
    app: &tauri::AppHandle,
    workspace_path: &std::path::Path,
) -> anyhow::Result<MemoryWatcherState> {
    let app_handle = app.clone();
    let watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
        if let Ok(event) = result {
            let _ = app_handle.emit("march://memory-changed", event.paths);
        }
    })
    .context("failed to create memory watcher")?;
    let mut state = MemoryWatcherState {
        watcher,
        watched_dirs: HashSet::new(),
    };
    state.sync_dirs(&collect_memory_dirs(workspace_path)?)?;
    Ok(state)
}

const MAIN_WINDOW_LABEL: &str = "main";
const DEFAULT_WINDOW_WIDTH: u32 = 1440;
const DEFAULT_WINDOW_HEIGHT: u32 = 900;
const WINDOW_WORKAREA_MARGIN: u32 = 32;
const TASK_EVENT_BUFFER_CAPACITY: usize = 1000;

fn emit_backend_notice(
    app: &tauri::AppHandle,
    level: &str,
    source: impl Into<String>,
    message: impl Into<String>,
) {
    let payload = BackendNoticePayload {
        level: level.to_string(),
        source: source.into(),
        message: message.into(),
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default() as i64,
    };
    let _ = app.emit("march://backend-notice", payload);
}

fn report_command_result<T>(
    app: &tauri::AppHandle,
    source: &str,
    result: Result<T, String>,
) -> Result<T, String> {
    if let Err(message) = &result {
        emit_backend_notice(app, "error", source, message.clone());
    }
    result
}

fn extract_tool_stderr(detail: &str) -> Option<&str> {
    detail
        .split_once("\nPartial stderr:\n")
        .map(|(_, stderr)| stderr.trim())
        .or_else(|| {
            detail
                .split_once("\nStderr:\n")
                .map(|(_, stderr)| stderr.trim())
        })
        .filter(|stderr| !stderr.is_empty())
}

fn emit_tool_notice(app: &tauri::AppHandle, event: &UiAgentProgressEvent) {
    let UiAgentProgressEvent::ToolFinished {
        status,
        summary,
        detail,
        preview,
        ..
    } = event
    else {
        return;
    };

    let body = detail
        .as_deref()
        .or(preview.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(body) = body else {
        return;
    };

    if body.contains("timed out after") {
        emit_backend_notice(app, "error", "tool_timeout", format!("{summary}\n\n{body}"));
        return;
    }

    let Some(stderr) = extract_tool_stderr(body) else {
        return;
    };
    let level = match status {
        march::ui::UiAgentToolStatus::Success => "warning",
        march::ui::UiAgentToolStatus::Error => "error",
    };
    emit_backend_notice(app, level, "tool_stderr", format!("{summary}\n\n{stderr}"));
}

fn emit_provider_notice(app: &tauri::AppHandle, event: &UiAgentProgressEvent) {
    let UiAgentProgressEvent::RoundComplete { debug_round, .. } = event else {
        return;
    };

    let Ok(parsed) = serde_json::from_str::<Value>(&debug_round.provider_response_json) else {
        return;
    };

    let delivery_path = parsed
        .get("delivery_path")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if delivery_path != "non_streaming_fallback" {
        return;
    }

    let stream_failure = parsed
        .get("stream_failure")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let Some(stream_failure) = stream_failure else {
        return;
    };

    emit_backend_notice(
        app,
        "warning",
        "provider_fallback",
        format!("Provider streaming failed and fell back to non-streaming.\n\n{stream_failure}"),
    );
}

fn emit_progress_notice(app: &tauri::AppHandle, event: &UiAgentProgressEvent) {
    emit_tool_notice(app, event);
    emit_provider_notice(app, event);
}

fn handle_task_working_transition(
    app: &tauri::AppHandle,
    state: &AppState,
    event: &UiAgentProgressEvent,
) -> Result<(), String> {
    let task_id = match event {
        UiAgentProgressEvent::TurnStarted { task_id, .. }
        | UiAgentProgressEvent::TurnFinished { task_id, .. } => *task_id,
        _ => return Ok(()),
    };

    let mut counts = state
        .working_turn_counts
        .lock()
        .map_err(|_| "failed to acquire working-turn registry".to_string())?;
    let count = counts.entry(task_id).or_insert(0);
    let mut emit_payload = None;

    match event {
        UiAgentProgressEvent::TurnStarted { .. } => {
            let was_idle = *count == 0;
            *count += 1;
            if was_idle {
                emit_payload = Some(TaskWorkingChangedPayload {
                    task_id,
                    working: true,
                });
            }
        }
        UiAgentProgressEvent::TurnFinished { .. } => {
            if *count > 0 {
                *count -= 1;
            }
            if *count == 0 {
                counts.remove(&task_id);
                emit_payload = Some(TaskWorkingChangedPayload {
                    task_id,
                    working: false,
                });
            }
        }
        _ => {}
    }

    drop(counts);

    if let Some(payload) = emit_payload {
        app.emit("march://task-working-changed", &payload)
            .map_err(|error| format!("failed to emit task-working-changed: {error}"))?;
    }

    Ok(())
}

fn sync_turn_cancellation_registry(
    state: &AppState,
    turn_id: String,
    cancellation: Arc<TurnCancellation>,
) -> Result<(), String> {
    let mut turn_cancellations = state
        .turn_cancellations
        .lock()
        .map_err(|_| "failed to acquire turn cancellation registry".to_string())?;
    turn_cancellations.insert(turn_id, cancellation);

    Ok(())
}

fn clear_turn_cancellation_registry(state: &AppState, event: &UiAgentProgressEvent) -> Result<(), String> {
    if let UiAgentProgressEvent::TurnFinished { turn_id, .. } = event {
        let mut turn_cancellations = state
            .turn_cancellations
            .lock()
            .map_err(|_| "failed to acquire turn cancellation registry".to_string())?;
        turn_cancellations.remove(turn_id);
    }

    Ok(())
}

fn task_id_of_event(event: &UiAgentProgressEvent) -> i64 {
    match event {
        UiAgentProgressEvent::UserMessageAppended { task_id, .. }
        | UiAgentProgressEvent::TurnStarted { task_id, .. }
        | UiAgentProgressEvent::MessageStarted { task_id, .. }
        | UiAgentProgressEvent::ToolStarted { task_id, .. }
        | UiAgentProgressEvent::ToolFinished { task_id, .. }
        | UiAgentProgressEvent::AssistantStreamDelta { task_id, .. }
        | UiAgentProgressEvent::MessageFinished { task_id, .. }
        | UiAgentProgressEvent::TurnFinished { task_id, .. }
        | UiAgentProgressEvent::RoundComplete { task_id, .. } => *task_id,
    }
}

fn set_event_seq(event: &mut UiAgentProgressEvent, seq: u64) {
    match event {
        UiAgentProgressEvent::UserMessageAppended { seq: value, .. }
        | UiAgentProgressEvent::TurnStarted { seq: value, .. }
        | UiAgentProgressEvent::MessageStarted { seq: value, .. }
        | UiAgentProgressEvent::ToolStarted { seq: value, .. }
        | UiAgentProgressEvent::ToolFinished { seq: value, .. }
        | UiAgentProgressEvent::AssistantStreamDelta { seq: value, .. }
        | UiAgentProgressEvent::MessageFinished { seq: value, .. }
        | UiAgentProgressEvent::TurnFinished { seq: value, .. }
        | UiAgentProgressEvent::RoundComplete { seq: value, .. } => *value = seq,
    }
}

fn next_task_event_seq(state: &AppState, task_id: i64) -> Result<u64, String> {
    let mut sequences = state
        .event_sequences
        .lock()
        .map_err(|_| "failed to acquire event sequence registry".to_string())?;

    let current = if let Some(current) = sequences.get(&task_id).copied() {
        current
    } else {
        let storage = march::storage::MarchStorage::open(&state.workspace_path)
            .map_err(|error| error.to_string())?;
        let current = storage
            .load_task_last_event_seq(task_id)
            .map_err(|error| error.to_string())?;
        sequences.insert(task_id, current);
        current
    };

    let next = current.saturating_add(1);
    sequences.insert(task_id, next);
    drop(sequences);

    let storage =
        march::storage::MarchStorage::open(&state.workspace_path).map_err(|error| error.to_string())?;
    storage
        .update_task_last_event_seq(task_id, next)
        .map_err(|error| error.to_string())?;
    Ok(next)
}

fn task_progress_event_name(task_id: i64) -> String {
    format!("march://task-progress:{task_id}")
}

fn event_seq(event: &UiAgentProgressEvent) -> u64 {
    match event {
        UiAgentProgressEvent::UserMessageAppended { seq, .. }
        | UiAgentProgressEvent::TurnStarted { seq, .. }
        | UiAgentProgressEvent::MessageStarted { seq, .. }
        | UiAgentProgressEvent::ToolStarted { seq, .. }
        | UiAgentProgressEvent::ToolFinished { seq, .. }
        | UiAgentProgressEvent::AssistantStreamDelta { seq, .. }
        | UiAgentProgressEvent::MessageFinished { seq, .. }
        | UiAgentProgressEvent::TurnFinished { seq, .. }
        | UiAgentProgressEvent::RoundComplete { seq, .. } => *seq,
    }
}

fn buffer_task_event(state: &AppState, event: UiAgentProgressEvent) -> Result<(), String> {
    let task_id = task_id_of_event(&event);
    let mut buffers = state
        .event_buffers
        .lock()
        .map_err(|_| "failed to acquire event buffer registry".to_string())?;
    let buffer = buffers.entry(task_id).or_default();
    buffer.push_back(event);
    while buffer.len() > TASK_EVENT_BUFFER_CAPACITY {
        buffer.pop_front();
    }
    Ok(())
}

/// Keep the first-launch window inside the monitor work area.
///
/// March uses a custom title bar and a roomy three-column layout. On Windows with
/// taskbars and display scaling enabled, a fixed 1440x900 startup size can exceed
/// the monitor's usable work area, which leaves the composer clipped below the
/// bottom edge. We normalize the initial bounds against the monitor work area so
/// the full shell is visible on first paint.
fn normalize_main_window_bounds(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let monitor = window
        .current_monitor()?
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else {
        return Ok(());
    };

    let work_area = monitor.work_area();
    let max_width = work_area.size.width.saturating_sub(WINDOW_WORKAREA_MARGIN);
    let max_height = work_area.size.height.saturating_sub(WINDOW_WORKAREA_MARGIN);

    let scale_factor = monitor.scale_factor();
    let preferred = PhysicalSize::new(
        (f64::from(DEFAULT_WINDOW_WIDTH) * scale_factor).round() as u32,
        (f64::from(DEFAULT_WINDOW_HEIGHT) * scale_factor).round() as u32,
    );
    let target_size = PhysicalSize::new(
        preferred.width.min(max_width),
        preferred.height.min(max_height),
    );

    window.set_size(target_size)?;

    let centered_x =
        work_area.position.x + ((work_area.size.width as i32 - target_size.width as i32) / 2);
    let centered_y =
        work_area.position.y + ((work_area.size.height as i32 - target_size.height as i32) / 2);

    window.set_position(PhysicalPosition::new(
        centered_x.max(work_area.position.x),
        centered_y.max(work_area.position.y),
    ))?;
    Ok(())
}

#[tauri::command]
fn load_workspace_snapshot(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    active_task_id: Option<i64>,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "load_workspace_snapshot",
        backend
            .workspace_snapshot(active_task_id)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn get_task_history(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    task_id: i64,
) -> Result<UiTaskHistoryView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "get_task_history",
        backend.task_history(task_id).map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn subscribe_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    task_id: i64,
    since_seq: u64,
) -> Result<TaskSubscriptionView, String> {
    let replay_events = {
        let buffers = state
            .event_buffers
            .lock()
            .map_err(|_| "failed to acquire event buffer registry".to_string())?;
        let Some(buffer) = buffers.get(&task_id) else {
            return report_command_result(
                &app,
                "subscribe_task",
                Ok(TaskSubscriptionView {
                    status: TaskSubscriptionStatus::Subscribed,
                }),
            );
        };

        if let Some(oldest_seq) = buffer.front().map(event_seq)
            && since_seq > 0
            && oldest_seq > since_seq.saturating_add(1)
        {
            return report_command_result(
                &app,
                "subscribe_task",
                Ok(TaskSubscriptionView {
                    status: TaskSubscriptionStatus::GapTooLarge,
                }),
            );
        }

        buffer
            .iter()
            .filter(|event| event_seq(event) > since_seq)
            .cloned()
            .collect::<Vec<_>>()
    };

    for event in replay_events {
        app.emit(&task_progress_event_name(task_id), &event)
            .map_err(|error| format!("failed to emit task replay event: {error}"))?;
    }

    report_command_result(
        &app,
        "subscribe_task",
        Ok(TaskSubscriptionView {
            status: TaskSubscriptionStatus::Subscribed,
        }),
    )
}

#[tauri::command]
fn unsubscribe_task(app: tauri::AppHandle, task_id: i64) -> Result<(), String> {
    let _ = task_id;
    report_command_result(&app, "unsubscribe_task", Ok(()))
}

#[tauri::command]
fn create_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiCreateTaskRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    let result = backend
        .handle_create_task(input)
        .map_err(|error| error.to_string());
    if result.is_ok()
        && let Err(error) = sync_memory_watcher(&state)
    {
        emit_backend_notice(&app, "warning", "memory_watcher", error.to_string());
    }
    report_command_result(&app, "create_task", result)
}

#[tauri::command]
fn select_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiSelectTaskRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "select_task",
        backend
            .handle_select_task(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn delete_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiDeleteTaskRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    let result = backend
        .handle_delete_task(input)
        .map_err(|error| error.to_string());
    if result.is_ok()
        && let Err(error) = sync_memory_watcher(&state)
    {
        emit_backend_notice(&app, "warning", "memory_watcher", error.to_string());
    }
    report_command_result(&app, "delete_task", result)
}

#[tauri::command]
async fn send_message(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiSendMessageRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    let task_id = backend
        .resolve_or_create_task_id(input.task_id)
        .map_err(|error| error.to_string())?;
    let request_id = input.request_id.clone().unwrap_or_else(|| {
        format!(
            "anonymous-{}-{}",
            task_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis())
                .unwrap_or_default()
        )
    });
    {
        let mut in_flight_turns = state
            .in_flight_turns
            .lock()
            .map_err(|_| "failed to acquire in-flight turn registry".to_string())?;
        if let Some(existing_request_id) = in_flight_turns.get(&task_id) {
            // In dev, editing the running frontend can reload the webview while the
            // original invoke is still executing. Tauri may then retry the same
            // command through its fallback transport, which must not open a second
            // turn for the same task.
            if existing_request_id == &request_id {
                return backend
                    .workspace_snapshot(Some(task_id))
                    .map_err(|error| error.to_string());
            }
            return Err("the active task is still processing the previous turn".to_string());
        }
        in_flight_turns.insert(task_id, request_id.clone());
    }
    let cancellation = {
        let mut cancellations = state
            .cancellations
            .lock()
            .map_err(|_| "failed to acquire cancellation registry".to_string())?;
        let cancellation = Arc::new(TurnCancellation::new());
        cancellations.insert(task_id, cancellation.clone());
        cancellation
    };
    let request = UiSendMessageRequest {
        task_id: Some(task_id),
        request_id: Some(request_id),
        mentions: input.mentions,
        replies: input.replies,
        content_blocks: input.content_blocks,
    };
    let result = backend
        .handle_send_message_with_progress_and_cancel(
            request,
            |mut event| {
                let task_id = task_id_of_event(&event);
                let seq = next_task_event_seq(&state, task_id).map_err(anyhow::Error::msg)?;
                set_event_seq(&mut event, seq);
                buffer_task_event(&state, event.clone()).map_err(anyhow::Error::msg)?;
                clear_turn_cancellation_registry(&state, &event)
                    .map_err(anyhow::Error::msg)?;
                emit_progress_notice(&app, &event);
                handle_task_working_transition(&app, &state, &event)
                    .map_err(anyhow::Error::msg)?;
                app.emit(&task_progress_event_name(task_id), &event)
                    .map_err(|error| {
                    anyhow::anyhow!("failed to emit agent progress event: {}", error)
                })
            },
            |turn_id, turn_cancellation| {
                sync_turn_cancellation_registry(&state, turn_id, turn_cancellation)
                    .map_err(anyhow::Error::msg)
            },
            cancellation.as_ref(),
        )
        .await
        .map_err(|error| error.to_string());
    if let Ok(mut in_flight_turns) = state.in_flight_turns.lock() {
        in_flight_turns.remove(&task_id);
    }
    if let Ok(mut cancellations) = state.cancellations.lock() {
        cancellations.remove(&task_id);
    }
    if result.is_ok()
        && let Err(error) = sync_memory_watcher(&state)
    {
        emit_backend_notice(&app, "warning", "memory_watcher", error.to_string());
    }
    report_command_result(&app, "send_message", result)
}

#[tauri::command]
fn cancel_task(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    task_id: i64,
) -> Result<(), String> {
    let cancellations = state
        .cancellations
        .lock()
        .map_err(|_| "failed to acquire cancellation registry".to_string())?;
    if let Some(cancellation) = cancellations.get(&task_id) {
        cancellation.cancel();
    }
    report_command_result(&app, "cancel_task", Ok(()))
}

#[tauri::command]
fn cancel_turn(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    turn_id: String,
) -> Result<(), String> {
    let turn_cancellations = state
        .turn_cancellations
        .lock()
        .map_err(|_| "failed to acquire turn cancellation registry".to_string())?;
    if let Some(cancellation) = turn_cancellations.get(&turn_id) {
        cancellation.cancel();
    }
    report_command_result(&app, "cancel_turn", Ok(()))
}

#[tauri::command]
fn upsert_note(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiUpsertNoteRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "upsert_note",
        backend
            .handle_upsert_note(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn delete_note(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiDeleteNoteRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "delete_note",
        backend
            .handle_delete_note(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn list_memories(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiListMemoriesRequest,
) -> Result<Vec<UiMemoryDetailView>, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "list_memories",
        backend
            .list_memories(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn get_memory(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiGetMemoryRequest,
) -> Result<UiMemoryDetailView, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "get_memory",
        backend.get_memory(input).map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn upsert_memory(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiUpsertMemoryRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "upsert_memory",
        backend
            .handle_upsert_memory(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn delete_memory(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiDeleteMemoryRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "delete_memory",
        backend
            .handle_delete_memory(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn toggle_open_file_lock(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiToggleOpenFileLockRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "toggle_open_file_lock",
        backend
            .handle_toggle_open_file_lock(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn close_open_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiCloseOpenFileRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "close_open_file",
        backend
            .handle_close_open_file(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn open_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiOpenFilesRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "open_files",
        backend
            .handle_open_files(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
async fn list_provider_models(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    task_id: Option<i64>,
) -> Result<UiTaskModelSelectorView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    let task = backend
        .task_record_for_provider_models(task_id)
        .map_err(|error| error.to_string())?;
    drop(backend);
    report_command_result(
        &app,
        "list_provider_models",
        fetch_task_model_selector(task.as_ref())
            .await
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
async fn list_provider_models_for_settings(
    app: tauri::AppHandle,
    provider_id: i64,
) -> Result<UiProviderModelsView, String> {
    report_command_result(
        &app,
        "list_provider_models_for_settings",
        fetch_provider_models_for_provider(provider_id)
            .await
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
async fn list_probe_models(
    app: tauri::AppHandle,
    input: UiProbeProviderModelsRequest,
) -> Result<UiProviderModelsView, String> {
    report_command_result(
        &app,
        "list_probe_models",
        fetch_probe_models(input)
            .await
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
async fn probe_provider_model_capabilities(
    app: tauri::AppHandle,
    input: UiProbeProviderModelCapabilitiesRequest,
) -> Result<UiProbeProviderModelCapabilitiesView, String> {
    report_command_result(
        &app,
        "probe_provider_model_capabilities",
        fetch_probe_model_capabilities(input)
            .await
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn set_task_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiSetTaskModelRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "set_task_model",
        backend
            .handle_set_task_model(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn set_task_model_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiSetTaskModelSettingsRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "set_task_model_settings",
        backend
            .handle_set_task_model_settings(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn set_task_working_directory(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiSetTaskWorkingDirectoryRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    let result = backend
        .handle_set_task_working_directory(input)
        .map_err(|error| error.to_string());
    if result.is_ok()
        && let Err(error) = sync_memory_watcher(&state)
    {
        emit_backend_notice(&app, "warning", "memory_watcher", error.to_string());
    }
    report_command_result(&app, "set_task_working_directory", result)
}

#[tauri::command]
fn load_provider_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "load_provider_settings",
        backend
            .provider_settings()
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn upsert_provider(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiUpsertProviderRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "upsert_provider",
        backend
            .handle_upsert_provider(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn delete_provider(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiDeleteProviderRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "delete_provider",
        backend
            .handle_delete_provider(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn upsert_provider_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiUpsertProviderModelRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "upsert_provider_model",
        backend
            .handle_upsert_provider_model(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn delete_provider_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiDeleteProviderModelRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "delete_provider_model",
        backend
            .handle_delete_provider_model(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn set_default_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiSetDefaultModelRequest,
) -> Result<UiProviderSettingsView, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "set_default_model",
        backend
            .handle_set_default_model(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn upsert_agent(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiUpsertAgentRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "upsert_agent",
        backend
            .handle_upsert_agent(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn delete_agent(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiDeleteAgentRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "delete_agent",
        backend
            .handle_delete_agent(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn restore_march_prompt(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiRestoreMarchPromptRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "restore_march_prompt",
        backend
            .handle_restore_march_prompt(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
async fn test_provider_connection(
    app: tauri::AppHandle,
    input: UiTestProviderConnectionRequest,
) -> Result<UiTestProviderConnectionResult, String> {
    report_command_result(
        &app,
        "test_provider_connection",
        run_provider_connection_test(input)
            .await
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn search_workspace_entries(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiSearchWorkspaceEntriesRequest,
) -> Result<Vec<UiWorkspaceEntryView>, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "search_workspace_entries",
        backend
            .search_workspace_entries(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn search_mentions(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiSearchWorkspaceEntriesRequest,
) -> Result<Vec<UiMentionTargetView>, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "search_mentions",
        backend
            .search_mentions(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn search_skills(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiSearchSkillsRequest,
) -> Result<Vec<UiSkillSearchView>, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "search_skills",
        backend
            .search_skills(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn load_workspace_image(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    input: UiLoadWorkspaceImageRequest,
) -> Result<UiWorkspaceImageView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    report_command_result(
        &app,
        "load_workspace_image",
        backend
            .load_workspace_image(input)
            .map_err(|error| error.to_string()),
    )
}

#[tauri::command]
fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|error| error.to_string())?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("only http/https URLs are supported".to_string()),
    }

    report_command_result(
        &app,
        "open_external_url",
        webbrowser::open(parsed.as_str())
            .with_context(|| format!("failed to open external URL: {}", parsed))
            .map_err(|error| error.to_string())
            .map(|_| ()),
    )
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = dotenvy::dotenv();
    let workspace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root should exist")
        .to_path_buf();
    std::env::set_current_dir(&workspace_path).expect("failed to switch to workspace root");

    tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
        .setup({
            let watcher_workspace_path = workspace_path.clone();
            move |app| {
                if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                    normalize_main_window_bounds(&window)?;
                }
                let memory_watcher = build_memory_watcher(app.handle(), &watcher_workspace_path)?;
                app.manage(AppState {
                    workspace_path: watcher_workspace_path.clone(),
                    cancellations: Mutex::new(HashMap::new()),
                    turn_cancellations: Mutex::new(HashMap::new()),
                    in_flight_turns: Mutex::new(HashMap::new()),
                    working_turn_counts: Mutex::new(HashMap::new()),
                    event_sequences: Mutex::new(HashMap::new()),
                    event_buffers: Mutex::new(HashMap::new()),
                    memory_watcher: Mutex::new(memory_watcher),
                });
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_workspace_snapshot,
            get_task_history,
            subscribe_task,
            unsubscribe_task,
            create_task,
            select_task,
            delete_task,
            send_message,
            cancel_task,
            cancel_turn,
            upsert_note,
            delete_note,
            list_memories,
            get_memory,
            upsert_memory,
            delete_memory,
            toggle_open_file_lock,
            close_open_file,
            open_files,
            list_provider_models,
            list_provider_models_for_settings,
            list_probe_models,
            probe_provider_model_capabilities,
            set_task_model,
            set_task_model_settings,
            set_task_working_directory,
            load_provider_settings,
            upsert_provider,
            delete_provider,
            upsert_provider_model,
            delete_provider_model,
            set_default_model,
            upsert_agent,
            delete_agent,
            restore_march_prompt,
            test_provider_connection,
            search_workspace_entries,
            search_mentions,
            search_skills,
            load_workspace_image,
            open_external_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running March");
}
