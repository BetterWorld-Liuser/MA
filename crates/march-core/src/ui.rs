use std::path::PathBuf;

mod backend;
mod provider;
mod types;
mod util;
mod view;
mod workspace;

pub use provider::{
    fetch_probe_model_capabilities, fetch_probe_models, fetch_provider_models_for_provider,
    fetch_provider_models_for_task, fetch_task_model_selector, test_provider_connection,
};
pub use types::*;

const DEFAULT_TASK_NAME: &str = "默认任务";
const UI_MAX_RECENT_TURNS: usize = 10;

pub struct UiAppBackend {
    workspace_path: PathBuf,
    storage: crate::storage::MarchStorage,
}
