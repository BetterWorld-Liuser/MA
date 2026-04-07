use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::SystemTime;

use anyhow::{Result, bail};
use indexmap::IndexMap;
use tokio::sync::watch;

use crate::agents::{AgentProfile, MARCH_AGENT_NAME, SHARED_SCOPE, load_agent_profiles};
use crate::context::{
    AgentContext, AgentContextBuilder, ContentBlock, ContextBuildConfig, ContextPressure,
    ConversationHistory, DisplayTurn, FileSnapshot, Hint, Injection, NoteEntry, Role,
    SessionStatus, SystemStatus, ToolSummary, join_text_blocks,
};
use crate::memory::{MemoryIndexView, MemoryManager};
use crate::paths::clean_path;
use crate::storage::{PersistedNote, PersistedOpenFile, PersistedTask, PersistedTaskState};
use crate::tools::ToolRuntime;
use crate::ui::{
    UiContextPressureView, UiContextUsageSectionView, UiContextUsageView, UiFileSnapshotView,
    UiRuntimeSnapshot, UiShellView, UiSkillView, UiSystemStatusView,
};
use crate::watcher::FileWatcherService;

mod editing;
mod prompting;
mod runner;
mod runtime_views;
mod scopes;
mod session;
mod shells;
mod tool_calls;

#[cfg(test)]
use prompting::append_assistant_tool_call_message;
use prompting::normalize_open_files_for_workspace;
pub(crate) use prompting::{base_instructions, default_march_prompt, default_system_core};
use prompting::{load_skills_for_workspace, render_prompt, upsert_injection};
pub use runner::is_turn_cancelled_error;
use shells::decode_command_output;
pub use shells::{AvailableShell, CommandShell};
use shells::{
    detect_available_shells, platform_label, shell_command_with_cancel, workspace_entries,
};

const AGENTS_FILENAME: &str = "AGENTS.md";
const TURN_CANCELLED_ERROR_MESSAGE: &str = "turn cancelled";

#[derive(Debug)]
pub struct TurnCancellation {
    cancelled: watch::Sender<bool>,
}

impl Default for TurnCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnCancellation {
    pub fn new() -> Self {
        let (cancelled, _) = watch::channel(false);
        Self { cancelled }
    }

    pub fn never() -> &'static Self {
        static NEVER: OnceLock<TurnCancellation> = OnceLock::new();
        NEVER.get_or_init(TurnCancellation::new)
    }

    pub fn cancel(&self) {
        self.cancelled.send_replace(true);
    }

    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.borrow()
    }

    pub async fn cancelled(&self) {
        if self.is_cancelled() {
            return;
        }

        let mut receiver = self.cancelled.subscribe();
        while !*receiver.borrow() {
            if receiver.changed().await.is_err() {
                return;
            }
        }
    }
}

pub struct AgentSession {
    config: AgentConfig,
    watcher: FileWatcherService,
    agent_profiles: IndexMap<String, AgentProfile>,
    active_agent: String,
    history: ConversationHistory,
    notes: IndexMap<String, IndexMap<String, NoteEntry>>,
    open_files: Vec<PersistedOpenFile>,
    hints: Vec<Hint>,
    memory_manager: MemoryManager,
    last_memory_index: Option<MemoryIndexView>,
    injections: Vec<Injection>,
    skills: Vec<crate::skills::SkillEntry>,
    available_shells: Vec<AvailableShell>,
    working_directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub system_core: String,
    pub max_recent_turns: usize,
}

pub const DEFAULT_CONTEXT_WINDOW_TOKENS: usize = 128_000;

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            system_core: default_system_core(),
            max_recent_turns: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandRequest {
    pub command: String,
    pub shell: CommandShell,
}

#[derive(Debug, Clone)]
pub struct CommandExecution {
    pub command: String,
    pub working_directory: PathBuf,
    pub shell: CommandShell,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
}

#[derive(Debug, Clone)]
pub struct AgentRunResult {
    pub final_messages: Vec<FinalAssistantMessage>,
    pub debug_rounds: Vec<DebugRound>,
}

#[derive(Debug, Clone)]
pub enum AgentProgressEvent {
    Status {
        agent: String,
        phase: AgentStatusPhase,
        label: String,
    },
    ToolStarted {
        tool_call_id: String,
        tool_name: String,
        summary: String,
    },
    ToolFinished {
        tool_call_id: String,
        status: AgentToolStatus,
        summary: String,
        preview: Option<String>,
    },
    AssistantTextPreview {
        agent: String,
        message: String,
    },
    AssistantMessageCheckpoint(AssistantMessageCheckpoint),
    FinalAssistantMessage(FinalAssistantMessage),
    RoundCompleted(DebugRound),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentStatusPhase {
    BuildingContext,
    WaitingModel,
    RunningTool,
    Streaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentToolStatus {
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct AssistantMessageCheckpoint {
    pub message_id: String,
    pub message: String,
    pub checkpoint_type: AssistantMessageCheckpointType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantMessageCheckpointType {
    Intermediate,
    Final,
}

#[derive(Debug, Clone)]
pub struct FinalAssistantMessage {
    pub message_id: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct DebugRound {
    pub iteration: usize,
    pub context_preview: String,
    pub provider_request_json: String,
    pub provider_raw_response: String,
    pub tool_calls: Vec<DebugToolCall>,
    pub tool_results: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DebugToolCall {
    pub id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug, Clone)]
struct ToolOutcome {
    result_text: String,
    summary: Option<ToolSummary>,
}

fn estimate_token_count(text: &str) -> usize {
    let ascii_chars = text.chars().filter(|ch| ch.is_ascii()).count();
    let non_ascii_chars = text.chars().count().saturating_sub(ascii_chars);
    ascii_chars.div_ceil(4) + non_ascii_chars
}

fn estimate_content_blocks_token_count(content: &[ContentBlock]) -> usize {
    let text_tokens = estimate_token_count(&join_text_blocks(content));
    let image_tokens = content
        .iter()
        .map(ContentBlock::image_token_cost)
        .sum::<usize>();
    text_tokens + image_tokens
}

fn clean_unique_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths.iter().cloned().map(clean_path) {
        if !unique.iter().any(|existing| existing == &path) {
            unique.push(path);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use tokio::runtime::Runtime;
    use tokio::time::{Duration, sleep, timeout};

    use super::shells::resolve_shell_program_with;
    use super::{
        AGENTS_FILENAME, AgentConfig, AgentSession, CommandRequest, CommandShell, TurnCancellation,
        append_assistant_tool_call_message, base_instructions, decode_command_output,
        default_march_prompt, default_system_core, is_turn_cancelled_error,
        normalize_open_files_for_workspace,
    };
    use crate::agents::{MARCH_AGENT_NAME, SHARED_SCOPE};
    use crate::context::{ConversationHistory, Hint};
    use crate::provider::{ProviderToolCall, RequestMessage};
    use crate::storage::{PersistedOpenFile, PersistedTask, TaskRecord, TaskTitleSource};

    #[test]
    fn base_instructions_include_tool_and_handoff_guidance() {
        let base = base_instructions();

        // Tool use rules
        assert!(
            base.contains(
                "you must inspect the workspace with one or more tools before giving a substantive answer"
            )
        );
        assert!(base.contains(
            "Your turn ends ONLY when you output a text response without any tool calls"
        ));
        // Agent collaboration rules
        assert!(base.contains("You may mention another existing agent with `@agent_name`"));
        assert!(base.contains("March will automatically continue the next round as that agent"));
        assert!(base.contains("Do not claim that agent-to-agent handoff is unsupported"));
        assert!(base.contains("Do not reply with meta acknowledgements such as"));
    }

    #[test]
    fn march_prompt_includes_persona_and_behavior() {
        let march = default_march_prompt();

        assert!(march.contains("You are March, an agentic coding partner"));
        assert!(march.contains("If the user is greeting you or making small talk"));
        assert!(
            march.contains(
                "Do not assume every user message is a request for a project status report"
            )
        );
    }

    #[test]
    fn default_system_core_combines_base_and_march() {
        let full = default_system_core();
        assert!(full.contains(base_instructions()));
        assert!(full.contains(default_march_prompt()));
    }

    #[test]
    fn non_march_agents_get_base_instructions_and_own_prompt() {
        let workspace = temp_workspace_dir("ma-agent-system-core");
        let agent_dir = workspace.join(".march").join("agents");
        fs::create_dir_all(&agent_dir).expect("create agents dir");
        fs::write(
            agent_dir.join("reviewer.md"),
            "---\nname: reviewer\ndisplay_name: Code Reviewer\n---\nFocus on implementation risks first.",
        )
        .expect("write reviewer agent");

        let mut session = AgentSession::new(
            AgentConfig::default(),
            ConversationHistory::default(),
            [],
            workspace,
        )
        .expect("create agent session");
        session.set_active_agent("reviewer");

        let prompt = session.system_core_for_active_agent();

        // Has base instructions (shared foundation)
        assert!(prompt.contains("Core operating rule:"));
        assert!(prompt.contains("Tool use:"));
        assert!(prompt.contains("Agent collaboration:"));

        // Has roster with inline (you) marker on active agent
        assert!(prompt.contains("# Available Agents"));
        assert!(
            prompt
                .contains("- reviewer | Code Reviewer | Focus on implementation risks first (you)")
        );
        assert!(!prompt.contains("active_agent:"));

        // Has the reviewer's own system_prompt under Agent Role heading
        assert!(prompt.contains("# Agent Role\n"));
        assert!(prompt.contains("Focus on implementation risks first."));

        // Does NOT have March's persona or the old Active Agent Role section
        assert!(!prompt.contains("You are March, an agentic coding partner"));
        assert!(!prompt.contains("# Active Agent Role"));
        assert!(!prompt.contains("agent_name:"));
    }

    #[test]
    fn write_file_starts_tracking_new_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let temp_path = std::env::current_dir()
            .expect("current dir")
            .join(format!("ma-write-file-{unique}.txt"));

        let mut session = AgentSession::new(
            AgentConfig::default(),
            ConversationHistory::default(),
            [],
            std::env::current_dir().expect("current dir"),
        )
        .expect("create agent session");
        let tool_call = ProviderToolCall {
            id: "call_write".to_string(),
            name: "write_file".to_string(),
            arguments_json: serde_json::json!({
                "path": temp_path,
                "content": "hello from write_file\n",
            })
            .to_string(),
        };

        Runtime::new()
            .expect("create tokio runtime")
            .block_on(session.execute_tool_call(&tool_call, TurnCancellation::never()))
            .expect("write_file should succeed");

        let persisted = session.persisted_state();
        assert_eq!(persisted.open_files.len(), 1);
        assert_eq!(persisted.open_files[0].path, temp_path);
        assert!(
            session
                .runtime_open_file_snapshots()
                .contains_key(&temp_path)
        );

        let _ = std::fs::remove_file(&temp_path);
    }

    #[test]
    fn restore_skips_missing_open_files_from_persisted_state() {
        let missing_path = std::env::current_dir()
            .expect("current dir")
            .join("definitely-missing-open-file.txt");
        let existing_path = std::env::current_dir()
            .expect("current dir")
            .join("Cargo.toml");
        let persisted = PersistedTask {
            task: TaskRecord {
                id: 1,
                name: "test".to_string(),
                title_source: TaskTitleSource::Default,
                title_locked: false,
                working_directory: std::env::current_dir().expect("current dir"),
                selected_model_config_id: None,
                selected_model: None,
                model_temperature: None,
                model_top_p: None,
                model_presence_penalty: None,
                model_frequency_penalty: None,
                model_max_output_tokens: None,
                active_agent: MARCH_AGENT_NAME.to_string(),
                created_at: SystemTime::now(),
                last_active: SystemTime::now(),
            },
            active_agent: MARCH_AGENT_NAME.to_string(),
            history: ConversationHistory::default(),
            notes: Vec::new(),
            open_files: vec![
                PersistedOpenFile {
                    scope: SHARED_SCOPE.to_string(),
                    path: missing_path.clone(),
                    locked: true,
                },
                PersistedOpenFile {
                    scope: SHARED_SCOPE.to_string(),
                    path: existing_path.clone(),
                    locked: false,
                },
            ],
            hints: Vec::<Hint>::new(),
        };

        let session = AgentSession::restore(AgentConfig::default(), persisted)
            .expect("restore should skip missing files");

        let persisted = session.persisted_state();
        assert_eq!(persisted.open_files.len(), 1);
        assert_eq!(persisted.open_files[0].path, existing_path);
        assert!(!persisted.open_files[0].locked);
    }

    #[test]
    fn normalize_open_files_auto_adds_agents_file_as_locked_first() {
        let workspace = temp_workspace_dir("ma-agent-open-files");
        let regular_path = workspace.join("Cargo.toml");
        fs::write(&regular_path, "[package]\nname = \"demo\"\n").expect("write cargo");
        let agents_path = workspace.join(AGENTS_FILENAME);
        fs::write(&agents_path, "# rules\n").expect("write agents");

        let open_files = normalize_open_files_for_workspace(
            &workspace,
            vec![PersistedOpenFile {
                scope: SHARED_SCOPE.to_string(),
                path: regular_path.clone(),
                locked: false,
            }],
        );

        assert_eq!(open_files.len(), 2);
        assert_eq!(open_files[0].path, agents_path);
        assert!(open_files[0].locked);
        assert_eq!(open_files[1].path, regular_path);
        assert!(!open_files[1].locked);
    }

    #[test]
    fn normalize_open_files_preserves_existing_agents_lock_state_and_position() {
        let workspace = temp_workspace_dir("ma-agent-existing-agents");
        let first_path = workspace.join("src").join("main.rs");
        let agents_path = workspace.join(AGENTS_FILENAME);
        fs::create_dir_all(first_path.parent().expect("main parent")).expect("create src");
        fs::write(&first_path, "fn main() {}\n").expect("write main");
        fs::write(&agents_path, "# rules\n").expect("write agents");

        let open_files = normalize_open_files_for_workspace(
            &workspace,
            vec![
                PersistedOpenFile {
                    scope: SHARED_SCOPE.to_string(),
                    path: first_path.clone(),
                    locked: false,
                },
                PersistedOpenFile {
                    scope: SHARED_SCOPE.to_string(),
                    path: agents_path.clone(),
                    locked: false,
                },
            ],
        );

        assert_eq!(open_files.len(), 2);
        assert_eq!(open_files[0].path, first_path);
        assert_eq!(open_files[1].path, agents_path);
        assert!(!open_files[1].locked);
    }

    #[test]
    fn shell_detection_requires_successful_probe() {
        let resolved = resolve_shell_program_with(
            CommandShell::Bash,
            |candidate| Some(PathBuf::from(format!("C:/fake/{candidate}.exe"))),
            |_, _| false,
        );

        assert_eq!(resolved, None);
    }

    #[test]
    fn shell_detection_returns_first_runnable_candidate() {
        let resolved = resolve_shell_program_with(
            CommandShell::PowerShell,
            |candidate| match candidate {
                "powershell" => Some(PathBuf::from("C:/fake/powershell.exe")),
                "pwsh" => Some(PathBuf::from("C:/fake/pwsh.exe")),
                _ => None,
            },
            |_, path| path.ends_with("pwsh.exe"),
        );

        assert_eq!(resolved.as_deref(), Some("pwsh"));
    }

    #[test]
    fn shell_detection_prefers_pwsh_when_multiple_powershells_work() {
        let resolved = resolve_shell_program_with(
            CommandShell::PowerShell,
            |candidate| match candidate {
                "powershell" => Some(PathBuf::from("C:/fake/powershell.exe")),
                "pwsh" => Some(PathBuf::from("C:/fake/pwsh.exe")),
                _ => None,
            },
            |_, _| true,
        );

        assert_eq!(resolved.as_deref(), Some("pwsh"));
    }

    #[test]
    fn decode_command_output_falls_back_to_gbk_on_windows_style_bytes() {
        let decoded = decode_command_output(&[0xB2, 0xE2, 0xCA, 0xD4]);
        assert_eq!(decoded, "测试");
    }

    #[test]
    fn transient_messages_accumulate_tool_rounds() {
        let first_call = ProviderToolCall {
            id: "call_1".to_string(),
            name: "run_command".to_string(),
            arguments_json: serde_json::json!({
                "shell": "cmd",
                "command": "type package.json",
            })
            .to_string(),
        };
        let second_call = ProviderToolCall {
            id: "call_2".to_string(),
            name: "run_command".to_string(),
            arguments_json: serde_json::json!({
                "shell": "cmd",
                "command": "type Cargo.toml",
            })
            .to_string(),
        };

        let mut transient_messages = Vec::<RequestMessage>::new();
        append_assistant_tool_call_message(
            &mut transient_messages,
            None,
            std::slice::from_ref(&first_call),
        );
        transient_messages.push(RequestMessage::tool(
            first_call.id.clone(),
            "Exit code: 0\nStdout:\n{}",
        ));
        append_assistant_tool_call_message(
            &mut transient_messages,
            None,
            std::slice::from_ref(&second_call),
        );

        let payload = serde_json::to_value(&transient_messages).expect("serialize messages");
        let messages = payload.as_array().expect("messages array");

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[1]["role"], "tool");
        assert_eq!(messages[1]["tool_call_id"], "call_1");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["tool_calls"][0]["id"], "call_2");
    }

    #[test]
    fn turn_cancellation_wakes_waiters() {
        let runtime = Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let cancellation = TurnCancellation::new();
            assert!(!cancellation.is_cancelled());
            cancellation.cancel();
            cancellation.cancelled().await;
            assert!(cancellation.is_cancelled());
        });
    }

    #[test]
    fn run_command_returns_early_when_cancelled() {
        let runtime = Runtime::new().expect("create tokio runtime");
        runtime.block_on(async {
            let mut session = AgentSession::new(
                AgentConfig::default(),
                ConversationHistory::default(),
                [],
                std::env::current_dir().expect("current dir"),
            )
            .expect("create agent session");

            let cancellation = Arc::new(TurnCancellation::new());
            let cancel_handle = Arc::clone(&cancellation);
            tokio::spawn(async move {
                sleep(Duration::from_millis(150)).await;
                cancel_handle.cancel();
            });

            let (shell, command) = if cfg!(windows) {
                (
                    CommandShell::PowerShell,
                    "Start-Sleep -Seconds 5".to_string(),
                )
            } else {
                (CommandShell::Sh, "sleep 5".to_string())
            };

            let result = timeout(
                Duration::from_secs(2),
                session.run_command(CommandRequest { command, shell }, cancellation.as_ref()),
            )
            .await
            .expect("cancelled command should return promptly");

            let error = result.expect_err("cancelled command should fail");
            assert!(is_turn_cancelled_error(&error));
        });
    }

    fn temp_workspace_dir(prefix: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("after epoch")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create workspace");
        root
    }
}
