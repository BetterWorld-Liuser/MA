use std::path::PathBuf;

use indexmap::IndexMap;

use crate::agent::{AgentSession, AgentStatusPhase, AgentToolStatus, DebugRound, DebugToolCall};
use crate::agents::{AgentProfile, AgentProfileSource, MARCH_AGENT_NAME};
use crate::context::{
    ContentBlock, ContextPressure, DisplayTurn, FileSnapshot, Hint, ModifiedBy, Role, SystemStatus,
    ToolSummary, join_text_blocks,
};
use crate::paths::clean_path;
use crate::provider::format_provider_response_for_debug;
use crate::settings::{ProviderModelRecord, ProviderRecord, ProviderSettingsSnapshot};
use crate::storage::{PersistedOpenFile, PersistedTask, TaskRecord};

use super::util::{mask_api_key, pretty_json_or_original, system_time_to_unix};
use super::{
    UiAgentProfileView, UiAgentStatusPhase, UiAgentToolStatus, UiContextPressureView,
    UiContextUsageSectionView, UiContextUsageView, UiDebugRoundView, UiDebugToolCallView,
    UiDebugTraceView, UiFileSnapshotView, UiHintView, UiImageAttachmentView,
    UiModelCapabilitiesView, UiModifiedByView, UiNoteView, UiOpenFileView, UiProviderModelView,
    UiProviderSettingsView, UiProviderView, UiRoleView, UiRuntimeSnapshot, UiShellView,
    UiSkillView, UiSystemStatusView, UiTaskSnapshot, UiTaskSummary, UiToolSummaryView, UiTurnView,
};

impl UiTaskSnapshot {
    pub fn from_persisted(task: PersistedTask) -> Self {
        let PersistedTask {
            task,
            active_agent,
            history,
            notes,
            open_files,
            hints,
        } = task;

        Self {
            task: UiTaskSummary::from(task),
            active_agent,
            history: history.turns.into_iter().map(UiTurnView::from).collect(),
            notes: notes
                .into_iter()
                .map(|note| UiNoteView {
                    scope: note.scope,
                    id: note.id,
                    content: note.entry.content,
                })
                .collect(),
            open_files: open_files.into_iter().map(UiOpenFileView::from).collect(),
            hints: hints.into_iter().map(UiHintView::from).collect(),
            system_status: UiSystemStatusView {
                locked_files: Vec::new(),
                context_pressure: None,
            },
            runtime: None,
            debug_trace: None,
        }
    }

    pub fn with_agent_display_names(mut self, session: &AgentSession) -> Self {
        for turn in &mut self.history {
            turn.agent_display_name = session.display_name_for_agent(&turn.agent);
        }
        self
    }

    pub fn with_runtime(mut self, runtime: &UiRuntimeSnapshot) -> Self {
        let runtime_snapshot_map = runtime
            .open_files
            .iter()
            .map(|snapshot| (snapshot.path().to_path_buf(), snapshot.clone()))
            .collect::<IndexMap<_, _>>();

        for open_file in &mut self.open_files {
            if let Some(snapshot) = runtime_snapshot_map.get(&open_file.path) {
                open_file.snapshot = Some(snapshot.clone());
            }
        }

        self.system_status = runtime.system_status.clone();
        self.runtime = Some(runtime.clone());
        self
    }

    pub fn with_debug_trace(mut self, debug_trace: UiDebugTraceView) -> Self {
        self.debug_trace = Some(debug_trace);
        self
    }
}

impl UiProviderSettingsView {
    pub fn from_snapshot(database_path: PathBuf, snapshot: ProviderSettingsSnapshot) -> Self {
        let provider_models = snapshot.provider_models;
        Self {
            database_path: clean_path(database_path),
            providers: snapshot
                .providers
                .into_iter()
                .map(|provider| UiProviderView::from_record(provider, &provider_models))
                .collect(),
            agents: snapshot
                .agent_profiles
                .into_iter()
                .map(UiAgentProfileView::from)
                .collect(),
            default_provider_id: snapshot.default_provider_id,
            default_model: snapshot.default_model,
        }
    }
}

impl From<crate::settings::AgentProfileRecord> for UiAgentProfileView {
    fn from(profile: crate::settings::AgentProfileRecord) -> Self {
        Self {
            id: Some(profile.id),
            name: profile.name,
            display_name: profile.display_name,
            description: profile.description,
            system_prompt: profile.system_prompt,
            avatar_color: profile.avatar_color,
            provider_id: profile.provider_id,
            model_id: profile.model_id,
            is_built_in: false,
            source: "user".to_string(),
        }
    }
}

impl From<&AgentProfile> for UiAgentProfileView {
    fn from(profile: &AgentProfile) -> Self {
        Self {
            id: None,
            name: profile.name.clone(),
            display_name: profile.display_name.clone(),
            description: profile.description.clone(),
            system_prompt: profile.system_prompt.clone(),
            avatar_color: profile.avatar_color.clone(),
            provider_id: profile.provider_id,
            model_id: profile.model_id.clone(),
            is_built_in: profile.name == MARCH_AGENT_NAME,
            source: match profile.source {
                AgentProfileSource::BuiltIn => "built_in",
                AgentProfileSource::User => "user",
                AgentProfileSource::Project => "project",
            }
            .to_string(),
        }
    }
}

impl UiRuntimeSnapshot {
    pub fn new(
        working_directory: PathBuf,
        available_shells: Vec<UiShellView>,
        open_files: Vec<UiFileSnapshotView>,
        skills: Vec<UiSkillView>,
        system_status: UiSystemStatusView,
        context_usage: UiContextUsageView,
    ) -> Self {
        Self {
            working_directory,
            available_shells,
            open_files,
            skills,
            system_status,
            context_usage,
        }
    }

    pub fn from_session(session: &AgentSession, context_budget_tokens: usize) -> Self {
        session.ui_runtime_snapshot(context_budget_tokens)
    }
}

impl UiDebugTraceView {
    pub fn from_rounds(rounds: &[DebugRound]) -> Self {
        Self {
            rounds: rounds.iter().cloned().map(UiDebugRoundView::from).collect(),
        }
    }
}

impl UiTaskSummary {
    pub fn from_task(task: TaskRecord) -> Self {
        task.into()
    }
}

impl UiProviderView {
    fn from_record(provider: ProviderRecord, provider_models: &[ProviderModelRecord]) -> Self {
        Self {
            id: provider.id,
            name: provider.name,
            provider_type: provider.provider_type.as_db_value().to_string(),
            base_url: provider.base_url,
            api_key_hint: mask_api_key(&provider.api_key),
            created_at: system_time_to_unix(provider.created_at),
            models: provider_models
                .iter()
                .filter(|model| model.provider_id == provider.id)
                .cloned()
                .map(UiProviderModelView::from)
                .collect(),
        }
    }
}

impl From<ProviderModelRecord> for UiProviderModelView {
    fn from(model: ProviderModelRecord) -> Self {
        Self {
            id: model.id,
            provider_id: model.provider_id,
            model_id: model.model_id,
            display_name: model.display_name,
            capabilities: UiModelCapabilitiesView {
                context_window: model.context_window,
                max_output_tokens: model.max_output_tokens,
                supports_tool_use: model.supports_tool_use,
                supports_vision: model.supports_vision,
                supports_audio: model.supports_audio,
                supports_pdf: model.supports_pdf,
            },
        }
    }
}

impl From<TaskRecord> for UiTaskSummary {
    fn from(task: TaskRecord) -> Self {
        Self {
            id: task.id,
            name: task.name,
            title_source: task.title_source.as_db_value().to_string(),
            title_locked: task.title_locked,
            working_directory: clean_path(task.working_directory),
            selected_model: task.selected_model,
            created_at: system_time_to_unix(task.created_at),
            last_active: system_time_to_unix(task.last_active),
        }
    }
}

impl From<DisplayTurn> for UiTurnView {
    fn from(turn: DisplayTurn) -> Self {
        Self {
            role: UiRoleView::from(turn.role),
            agent_display_name: turn.agent.clone(),
            agent: turn.agent,
            content: join_text_blocks(&turn.content),
            images: turn
                .content
                .iter()
                .enumerate()
                .filter_map(|(index, block)| image_attachment_from_content_block(block, index))
                .collect(),
            tool_summaries: turn
                .tool_calls
                .into_iter()
                .map(UiToolSummaryView::from)
                .collect(),
            timestamp: system_time_to_unix(turn.timestamp),
        }
    }
}

fn image_attachment_from_content_block(
    block: &ContentBlock,
    index: usize,
) -> Option<UiImageAttachmentView> {
    let ContentBlock::Image {
        media_type,
        data_base64,
        source_path,
        name,
    } = block
    else {
        return None;
    };

    let display_name = name.clone().or_else(|| {
        source_path.as_ref().and_then(|path| {
            path.file_name()
                .map(|value| value.to_string_lossy().into_owned())
        })
    });
    let normalized_source_path = source_path.clone().map(clean_path);
    Some(UiImageAttachmentView {
        id: normalized_source_path
            .as_ref()
            .map(|path| format!("{}:{index}", path.display()))
            .unwrap_or_else(|| format!("inline-image-{index}")),
        name: display_name.unwrap_or_else(|| format!("image-{index}")),
        media_type: media_type.clone(),
        data_url: format!("data:{};base64,{}", media_type, data_base64),
        source_path: normalized_source_path,
    })
}

impl From<Role> for UiRoleView {
    fn from(role: Role) -> Self {
        match role {
            Role::System => Self::System,
            Role::User => Self::User,
            Role::Assistant => Self::Assistant,
            Role::Tool => Self::Tool,
        }
    }
}

impl From<ToolSummary> for UiToolSummaryView {
    fn from(summary: ToolSummary) -> Self {
        Self {
            name: summary.name,
            summary: summary.summary,
        }
    }
}

impl From<Hint> for UiHintView {
    fn from(hint: Hint) -> Self {
        Self {
            content: hint.content,
            expires_at: hint.expires_at.map(system_time_to_unix),
            turns_remaining: hint.turns_remaining,
        }
    }
}

impl From<DebugRound> for UiDebugRoundView {
    fn from(round: DebugRound) -> Self {
        Self {
            iteration: round.iteration,
            context_preview: round.context_preview,
            provider_request_json: pretty_json_or_original(&round.provider_request_json),
            provider_response_json: format_provider_response_for_debug(
                &round.provider_raw_response,
            ),
            provider_response_raw: pretty_json_or_original(&round.provider_raw_response),
            tool_calls: round
                .tool_calls
                .into_iter()
                .map(UiDebugToolCallView::from)
                .collect(),
            tool_results: round.tool_results,
        }
    }
}

impl From<DebugToolCall> for UiDebugToolCallView {
    fn from(tool_call: DebugToolCall) -> Self {
        Self {
            id: tool_call.id,
            name: tool_call.name,
            arguments_json: tool_call.arguments_json,
        }
    }
}

impl From<PersistedOpenFile> for UiOpenFileView {
    fn from(open_file: PersistedOpenFile) -> Self {
        Self {
            scope: open_file.scope,
            path: clean_path(open_file.path),
            locked: open_file.locked,
            snapshot: None,
        }
    }
}

impl From<FileSnapshot> for UiFileSnapshotView {
    fn from(snapshot: FileSnapshot) -> Self {
        match snapshot {
            FileSnapshot::Available {
                path,
                content,
                last_modified,
                last_modified_by,
            } => Self::Available {
                path: clean_path(path),
                content,
                last_modified_at: system_time_to_unix(last_modified),
                modified_by: last_modified_by.into(),
            },
            FileSnapshot::Deleted {
                path,
                last_seen_at,
                last_modified_by,
            } => Self::Deleted {
                path: clean_path(path),
                last_seen_at: system_time_to_unix(last_seen_at),
                modified_by: last_modified_by.into(),
            },
            FileSnapshot::Moved {
                path,
                new_path,
                last_seen_at,
                last_modified_by,
            } => Self::Moved {
                path: clean_path(path),
                new_path: clean_path(new_path),
                last_seen_at: system_time_to_unix(last_seen_at),
                modified_by: last_modified_by.into(),
            },
        }
    }
}

impl From<ModifiedBy> for UiModifiedByView {
    fn from(value: ModifiedBy) -> Self {
        match value {
            ModifiedBy::Agent => Self::Agent,
            ModifiedBy::User => Self::User,
            ModifiedBy::External => Self::External,
            ModifiedBy::Unknown => Self::Unknown,
        }
    }
}

impl From<ContextPressure> for UiContextPressureView {
    fn from(value: ContextPressure) -> Self {
        Self {
            used_percent: value.used_percent,
            message: value.message,
        }
    }
}

impl UiSystemStatusView {
    pub fn from_system_status(status: SystemStatus) -> Self {
        Self {
            locked_files: status.locked_files.into_iter().map(clean_path).collect(),
            context_pressure: status.context_pressure.map(Into::into),
        }
    }
}

impl UiContextUsageView {
    pub fn new(
        used_tokens: usize,
        budget_tokens: usize,
        sections: Vec<UiContextUsageSectionView>,
    ) -> Self {
        let used_percent = if budget_tokens == 0 {
            0
        } else {
            (((used_tokens as f64 / budget_tokens as f64) * 100.0).round()).clamp(0.0, 100.0) as u8
        };

        Self {
            used_tokens,
            budget_tokens,
            used_percent,
            sections,
        }
    }
}

impl UiContextUsageSectionView {
    pub fn new(name: impl Into<String>, tokens: usize) -> Self {
        Self {
            name: name.into(),
            tokens,
        }
    }
}

impl From<AgentStatusPhase> for UiAgentStatusPhase {
    fn from(value: AgentStatusPhase) -> Self {
        match value {
            AgentStatusPhase::BuildingContext => Self::BuildingContext,
            AgentStatusPhase::WaitingModel => Self::WaitingModel,
            AgentStatusPhase::RunningTool => Self::RunningTool,
            AgentStatusPhase::Streaming => Self::Streaming,
        }
    }
}

impl From<AgentToolStatus> for UiAgentToolStatus {
    fn from(value: AgentToolStatus) -> Self {
        match value {
            AgentToolStatus::Success => Self::Success,
            AgentToolStatus::Error => Self::Error,
        }
    }
}
