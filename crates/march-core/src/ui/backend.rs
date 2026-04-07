use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use base64::Engine;

use crate::agent::{
    AgentConfig, AgentProgressEvent, AgentRunResult, AgentSession, DebugRound, TurnCancellation,
    is_turn_cancelled_error,
};
use crate::agents::SHARED_SCOPE;
use crate::agents::{MARCH_AGENT_NAME, load_agent_profiles};
use crate::context::{ContentBlock, ConversationHistory, join_text_blocks};
use crate::paths::{canonicalize_clean, clean_path};
use crate::provider::{OpenAiCompatibleClient, fallback_task_title};
use crate::settings::{
    ProviderType, ServerToolCapability, ServerToolConfig, ServerToolFormat, SettingsStorage,
};
use crate::storage::{
    PersistedTask, PersistedTaskState, TaskCreateOptions, TaskRecord, TaskTitleSource,
};

use super::provider::provider_config_for_session;
use super::util::{resolve_context_window_fallback, system_time_to_unix};
use super::{
    DEFAULT_TASK_NAME, UI_MAX_RECENT_TURNS, UiAgentFailureStage, UiAgentProfileView,
    UiAgentProgressEvent, UiAppBackend, UiCloseOpenFileRequest, UiComposerContentBlock,
    UiCreateTaskRequest, UiDebugTraceView, UiDeleteAgentRequest, UiDeleteNoteRequest,
    UiDeleteProviderModelRequest, UiDeleteProviderRequest, UiDeleteTaskRequest,
    UiLoadWorkspaceImageRequest, UiMentionTargetView, UiOpenFilesRequest, UiProviderSettingsView,
    UiRestoreMarchPromptRequest, UiSearchSkillsRequest, UiSearchWorkspaceEntriesRequest,
    UiSelectTaskRequest, UiSendMessageRequest, UiSetDefaultModelRequest, UiSetTaskModelRequest,
    UiSetTaskModelSettingsRequest, UiSetTaskWorkingDirectoryRequest, UiSkillSearchView,
    UiTaskSnapshot, UiUpsertAgentRequest, UiUpsertNoteRequest, UiUpsertProviderModelRequest,
    UiUpsertProviderRequest, UiWorkspaceEntryView, UiWorkspaceImageView, UiWorkspaceSnapshot,
};

mod messaging;
mod settings;
mod workspace;

struct PreparedMessageContext {
    persisted_task: PersistedTask,
    session: AgentSession,
    should_auto_title: bool,
}

fn ui_agent_config() -> AgentConfig {
    AgentConfig {
        max_recent_turns: UI_MAX_RECENT_TURNS,
        ..AgentConfig::default()
    }
}

fn should_auto_title(task: &PersistedTask, user_message: &str) -> bool {
    task.task.title_source == TaskTitleSource::Default
        && !task.task.title_locked
        && task.history.turns.is_empty()
        && user_message.trim().chars().count() >= 4
}

fn classify_turn_failure(error: &anyhow::Error) -> (UiAgentFailureStage, bool) {
    let message = error.to_string().to_ascii_lowercase();

    if message.contains("tool ")
        || message.contains("invalid ")
        || message.contains("failed to decode arguments for tool")
        || message.contains("write_file")
        || message.contains("replace_lines")
        || message.contains("insert_lines")
        || message.contains("delete_lines")
        || message.contains("run_command")
        || message.contains("open_file")
    {
        return (UiAgentFailureStage::Tool, true);
    }

    if message.contains("provider")
        || message.contains("chat completion")
        || message.contains("model list")
        || message.contains("request chat")
        || message.contains("stream")
        || message.contains("assistant text without tool calls")
        || message.contains("neither assistant text nor tool calls")
    {
        return (UiAgentFailureStage::Provider, true);
    }

    if message.contains("context") {
        return (UiAgentFailureStage::Context, true);
    }

    (UiAgentFailureStage::Internal, false)
}

async fn resolve_context_window_with_provider(
    provider: &OpenAiCompatibleClient,
    current_model: &str,
) -> Option<usize> {
    provider
        .resolve_model_context_window(current_model)
        .await
        .ok()
        .flatten()
        .or_else(|| Some(resolve_context_window_fallback(Some(current_model))))
}

fn resolve_workspace_path(working_directory: &Path, path: &Path) -> Result<PathBuf> {
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_directory.join(path)
    };

    canonicalize_clean(&candidate)
        .with_context(|| format!("failed to resolve {}", candidate.display()))
}

fn infer_image_media_type(path: &Path) -> Option<&'static str> {
    match path
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

fn content_block_from_ui(block: UiComposerContentBlock) -> ContentBlock {
    match block {
        UiComposerContentBlock::Text { text } => ContentBlock::text(text),
        UiComposerContentBlock::Image {
            media_type,
            data_base64,
            source_path,
            name,
        } => ContentBlock::image(media_type, data_base64, source_path, name),
    }
}

fn detect_agent_mention(text: &str, session: &AgentSession) -> Option<String> {
    text.split_whitespace().find_map(|segment| {
        if !segment.contains('@') {
            return None;
        }
        let candidate = segment
            .trim()
            .trim_start_matches('@')
            .trim_matches(|ch: char| {
                ch == ',' || ch == ':' || ch == '，' || ch == '：' || ch == '。' || ch == '!'
            })
            .to_ascii_lowercase();
        if candidate.is_empty() {
            return None;
        }
        session.has_agent(&candidate).then_some(candidate)
    })
}
