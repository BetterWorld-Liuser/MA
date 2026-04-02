use std::path::PathBuf;

use tauri::Emitter;

use ma::ui::{
    UiAppBackend, UiCloseOpenFileRequest, UiCreateTaskRequest, UiDeleteNoteRequest,
    UiDeleteTaskRequest, UiSelectTaskRequest, UiSendMessageRequest, UiToggleOpenFileLockRequest,
    UiUpsertNoteRequest, UiWorkspaceSnapshot,
};

#[derive(Clone)]
struct AppState {
    workspace_path: PathBuf,
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
    backend
        .handle_send_message_with_progress(input, |event| {
            app.emit("ma://agent-progress", &event)
                .map_err(|error| anyhow::anyhow!("failed to emit agent progress event: {}", error))
        })
        .await
        .map_err(|error| error.to_string())
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = dotenvy::dotenv();
    let workspace_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root should exist")
        .to_path_buf();
    std::env::set_current_dir(&workspace_path).expect("failed to switch to workspace root");

    tauri::Builder::default()
        .manage(AppState { workspace_path })
        .invoke_handler(tauri::generate_handler![
            load_workspace_snapshot,
            create_task,
            select_task,
            delete_task,
            send_message,
            upsert_note,
            delete_note,
            toggle_open_file_lock,
            close_open_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running March");
}
