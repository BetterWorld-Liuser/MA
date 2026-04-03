use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use tauri::Emitter;

use ma::ui::{
    UiAppBackend, UiCloseOpenFileRequest, UiCreateTaskRequest, UiDeleteNoteRequest,
    UiDeleteProviderRequest, UiDeleteTaskRequest, UiOpenFilesRequest, UiProviderModelsView,
    UiProviderSettingsView, UiSearchWorkspaceEntriesRequest, UiSelectTaskRequest,
    UiSendMessageRequest, UiSetDefaultProviderRequest, UiSetTaskModelRequest,
    UiToggleOpenFileLockRequest, UiUpsertNoteRequest, UiUpsertProviderRequest,
    UiWorkspaceEntryView, UiWorkspaceSnapshot, fetch_provider_models,
    fetch_provider_models_for_provider,
};

struct AppState {
    workspace_path: PathBuf,
    cancellations: Mutex<HashMap<i64, Arc<AtomicBool>>>,
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
        content: input.content,
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
fn cancel_turn(
    state: tauri::State<'_, AppState>,
    task_id: i64,
) -> Result<(), String> {
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
) -> Result<UiProviderModelsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    let selected_model = backend
        .selected_model_for_task(task_id)
        .map_err(|error| error.to_string())?;
    fetch_provider_models(selected_model)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn list_provider_models_for_settings(provider_id: i64) -> Result<UiProviderModelsView, String> {
    fetch_provider_models_for_provider(provider_id)
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
fn load_provider_settings(
    state: tauri::State<'_, AppState>,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend.provider_settings().map_err(|error| error.to_string())
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
fn set_default_provider(
    state: tauri::State<'_, AppState>,
    input: UiSetDefaultProviderRequest,
) -> Result<UiProviderSettingsView, String> {
    let backend = UiAppBackend::open(&state.workspace_path).map_err(|error| error.to_string())?;
    backend
        .handle_set_default_provider(input)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = dotenvy::dotenv();
    let workspace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root should exist")
        .to_path_buf();
    std::env::set_current_dir(&workspace_path).expect("failed to switch to workspace root");

    tauri::Builder::default()
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
            set_task_model,
            load_provider_settings,
            upsert_provider,
            delete_provider,
            set_default_provider,
            search_workspace_entries
        ])
        .run(tauri::generate_context!())
        .expect("error while running March");
}
