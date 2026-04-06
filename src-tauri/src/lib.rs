use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use anyhow::Context;
use tauri::{Emitter, Manager, PhysicalPosition, PhysicalSize};

use ma::ui::{
    UiAppBackend, UiCloseOpenFileRequest, UiCreateTaskRequest, UiDeleteAgentRequest,
    UiDeleteNoteRequest, UiDeleteProviderModelRequest, UiDeleteProviderRequest,
    UiDeleteTaskRequest, UiLoadWorkspaceImageRequest, UiMentionTargetView, UiOpenFilesRequest,
    UiProbeProviderModelsRequest, UiProviderModelsView, UiProviderSettingsView,
    UiRestoreMarchPromptRequest, UiSearchSkillsRequest, UiSearchWorkspaceEntriesRequest,
    UiSelectTaskRequest, UiSendMessageRequest, UiSetDefaultProviderRequest,
    UiSetTaskModelRequest, UiSetTaskModelSettingsRequest, UiSetTaskWorkingDirectoryRequest,
    UiSkillSearchView, UiTaskModelSelectorView, UiTestProviderConnectionRequest,
    UiTestProviderConnectionResult, UiToggleOpenFileLockRequest, UiUpsertAgentRequest,
    UiUpsertNoteRequest, UiUpsertProviderModelRequest,
    UiUpsertProviderRequest, UiWorkspaceEntryView, UiWorkspaceImageView, UiWorkspaceSnapshot,
    fetch_probe_models, fetch_provider_models_for_provider, fetch_task_model_selector,
    test_provider_connection as run_provider_connection_test,
};

struct AppState {
    workspace_path: PathBuf,
    cancellations: Mutex<HashMap<i64, Arc<AtomicBool>>>,
}

const MAIN_WINDOW_LABEL: &str = "main";
const DEFAULT_WINDOW_WIDTH: u32 = 1440;
const DEFAULT_WINDOW_HEIGHT: u32 = 900;
const WINDOW_WORKAREA_MARGIN: u32 = 32;

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
    state: tauri::State<'_, AppState>,
    active_task_id: Option<i64>,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .workspace_snapshot(active_task_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_task(
    state: tauri::State<'_, AppState>,
    input: UiCreateTaskRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_create_task(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn select_task(
    state: tauri::State<'_, AppState>,
    input: UiSelectTaskRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_select_task(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_task(
    state: tauri::State<'_, AppState>,
    input: UiDeleteTaskRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_delete_task(input)
        .map_err(|error| error.to_string())
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
    let cancellation_flag = {
        let mut cancellations = state
            .cancellations
            .lock()
            .map_err(|_| "failed to acquire cancellation registry".to_string())?;
        let flag = Arc::new(AtomicBool::new(false));
        cancellations.insert(task_id, flag.clone());
        flag
    };
    let request = UiSendMessageRequest {
        task_id: Some(task_id),
        content_blocks: input.content_blocks,
    };
    backend
        .handle_send_message_with_progress_and_cancel(
            request,
            |event| {
                app.emit("ma://agent-progress", &event).map_err(|error| {
                    anyhow::anyhow!("failed to emit agent progress event: {}", error)
                })
            },
            || cancellation_flag.load(Ordering::SeqCst),
        )
        .await
        .map_err(|error| error.to_string())
        .inspect(|_| {
            if let Ok(mut cancellations) = state.cancellations.lock() {
                cancellations.remove(&task_id);
            }
        })
        .inspect_err(|_| {
            if let Ok(mut cancellations) = state.cancellations.lock() {
                cancellations.remove(&task_id);
            }
        })
}

#[tauri::command]
fn cancel_turn(state: tauri::State<'_, AppState>, task_id: i64) -> Result<(), String> {
    let cancellations = state
        .cancellations
        .lock()
        .map_err(|_| "failed to acquire cancellation registry".to_string())?;
    if let Some(flag) = cancellations.get(&task_id) {
        flag.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
fn upsert_note(
    state: tauri::State<'_, AppState>,
    input: UiUpsertNoteRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_upsert_note(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_note(
    state: tauri::State<'_, AppState>,
    input: UiDeleteNoteRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_delete_note(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn toggle_open_file_lock(
    state: tauri::State<'_, AppState>,
    input: UiToggleOpenFileLockRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_toggle_open_file_lock(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn close_open_file(
    state: tauri::State<'_, AppState>,
    input: UiCloseOpenFileRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_close_open_file(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_files(
    state: tauri::State<'_, AppState>,
    input: UiOpenFilesRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_open_files(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_provider_models(
    state: tauri::State<'_, AppState>,
    task_id: Option<i64>,
) -> Result<UiTaskModelSelectorView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    let task = backend
        .task_record_for_provider_models(task_id)
        .map_err(|error| error.to_string())?;
    drop(backend);
    fetch_task_model_selector(task.as_ref())
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_provider_models_for_settings(
    provider_id: i64,
) -> Result<UiProviderModelsView, String> {
    fetch_provider_models_for_provider(provider_id)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_probe_models(
    input: UiProbeProviderModelsRequest,
) -> Result<UiProviderModelsView, String> {
    fetch_probe_models(input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_task_model(
    state: tauri::State<'_, AppState>,
    input: UiSetTaskModelRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_set_task_model(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_task_model_settings(
    state: tauri::State<'_, AppState>,
    input: UiSetTaskModelSettingsRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_set_task_model_settings(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_task_working_directory(
    state: tauri::State<'_, AppState>,
    input: UiSetTaskWorkingDirectoryRequest,
) -> Result<UiWorkspaceSnapshot, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_set_task_working_directory(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn load_provider_settings(
    state: tauri::State<'_, AppState>,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .provider_settings()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn upsert_provider(
    state: tauri::State<'_, AppState>,
    input: UiUpsertProviderRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_upsert_provider(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_provider(
    state: tauri::State<'_, AppState>,
    input: UiDeleteProviderRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_delete_provider(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn upsert_provider_model(
    state: tauri::State<'_, AppState>,
    input: UiUpsertProviderModelRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_upsert_provider_model(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_provider_model(
    state: tauri::State<'_, AppState>,
    input: UiDeleteProviderModelRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_delete_provider_model(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_default_provider(
    state: tauri::State<'_, AppState>,
    input: UiSetDefaultProviderRequest,
) -> Result<UiProviderSettingsView, String> {
    let mut backend =
        UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_set_default_provider(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn upsert_agent(
    state: tauri::State<'_, AppState>,
    input: UiUpsertAgentRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_upsert_agent(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn delete_agent(
    state: tauri::State<'_, AppState>,
    input: UiDeleteAgentRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_delete_agent(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn restore_march_prompt(
    state: tauri::State<'_, AppState>,
    input: UiRestoreMarchPromptRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_restore_march_prompt(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn test_provider_connection(
    input: UiTestProviderConnectionRequest,
) -> Result<UiTestProviderConnectionResult, String> {
    run_provider_connection_test(input)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn search_workspace_entries(
    state: tauri::State<'_, AppState>,
    input: UiSearchWorkspaceEntriesRequest,
) -> Result<Vec<UiWorkspaceEntryView>, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .search_workspace_entries(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn search_mentions(
    state: tauri::State<'_, AppState>,
    input: UiSearchWorkspaceEntriesRequest,
) -> Result<Vec<UiMentionTargetView>, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .search_mentions(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn search_skills(
    state: tauri::State<'_, AppState>,
    input: UiSearchSkillsRequest,
) -> Result<Vec<UiSkillSearchView>, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend.search_skills(input).map_err(|error| error.to_string())
}

#[tauri::command]
fn load_workspace_image(
    state: tauri::State<'_, AppState>,
    input: UiLoadWorkspaceImageRequest,
) -> Result<UiWorkspaceImageView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .load_workspace_image(input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    let parsed = url::Url::parse(&url).map_err(|error| error.to_string())?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("only http/https URLs are supported".to_string()),
    }

    webbrowser::open(parsed.as_str())
        .with_context(|| format!("failed to open external URL: {}", parsed))
        .map_err(|error| error.to_string())?;
    Ok(())
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
        .setup(|app| {
            if let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) {
                normalize_main_window_bounds(&window)?;
            }
            Ok(())
        })
        .manage(AppState {
            workspace_path,
            cancellations: Mutex::new(HashMap::new()),
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
            toggle_open_file_lock,
            close_open_file,
            open_files,
            list_provider_models,
            list_provider_models_for_settings,
            list_probe_models,
            set_task_model,
            set_task_model_settings,
            set_task_working_directory,
            load_provider_settings,
            upsert_provider,
            delete_provider,
            upsert_provider_model,
            delete_provider_model,
            set_default_provider,
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
