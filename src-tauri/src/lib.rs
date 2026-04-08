use std::collections::{HashMap, HashSet};
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
    UiTaskModelSelectorView, UiTestProviderConnectionRequest, UiTestProviderConnectionResult,
    UiToggleOpenFileLockRequest, UiUpsertAgentRequest, UiUpsertMemoryRequest, UiUpsertNoteRequest,
    UiUpsertProviderModelRequest, UiUpsertProviderRequest, UiWorkspaceEntryView,
    UiWorkspaceImageView, UiWorkspaceSnapshot, fetch_probe_model_capabilities, fetch_probe_models,
    fetch_provider_models_for_provider, fetch_task_model_selector,
    test_provider_connection as run_provider_connection_test,
};

struct AppState {
    workspace_path: PathBuf,
    cancellations: Mutex<HashMap<i64, Arc<TurnCancellation>>>,
    in_flight_turns: Mutex<HashMap<i64, String>>,
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
        content_blocks: input.content_blocks,
    };
    let result = backend
        .handle_send_message_with_progress_and_cancel(
            request,
            |event| {
                emit_progress_notice(&app, &event);
                app.emit("march://agent-progress", &event).map_err(|error| {
                    anyhow::anyhow!("failed to emit agent progress event: {}", error)
                })
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
fn cancel_turn(
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
                    in_flight_turns: Mutex::new(HashMap::new()),
                    memory_watcher: Mutex::new(memory_watcher),
                });
                Ok(())
            }
        })
        .invoke_handler(tauri::generate_handler![
            load_workspace_snapshot,
            create_task,
            select_task,
            delete_task,
            send_message,
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
