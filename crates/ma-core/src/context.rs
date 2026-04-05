use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::tools::ToolDefinition;

/// AI 每轮实际收到的上下文。
/// 分层顺序直接对应设计文档中的 cache 稳定性排序，
/// 方便后续把不同层映射到 provider 的独立输入结构。
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub system_core: String,
    pub injections: Vec<Injection>,
    pub tools: Vec<ToolDefinition>,
    pub open_files: IndexMap<PathBuf, FileSnapshot>,
    pub notes: IndexMap<String, NoteEntry>,
    pub session_status: SessionStatus,
    pub runtime_status: RuntimeStatus,
    pub hints: Vec<Hint>,
    pub recent_chat: Vec<ChatTurn>,
}

impl AgentContext {
    /// open_files 需要保序，因此这里返回其当前插入顺序视图，
    /// 而不是再做一次额外排序，避免 prompt 前缀无意义抖动。
    pub fn open_files_in_prompt_order(&self) -> Vec<&FileSnapshot> {
        self.open_files.values().collect()
    }
}

pub fn render_file_snapshot_for_prompt(snapshot: &FileSnapshot) -> String {
    match snapshot {
        FileSnapshot::Available { path, content, .. } => format!(
            "## {}\n{}\n",
            path.display(),
            render_file_content_for_prompt(content)
        ),
        FileSnapshot::Deleted { path, .. } => {
            format!("## {}\nstatus=deleted\n", path.display())
        }
        FileSnapshot::Moved { path, new_path, .. } => format!(
            "## {}\nstatus=moved new_path={}\n",
            path.display(),
            new_path.display()
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Injection {
    pub id: String,
    pub content: String,
}

/// watcher 维护的单文件真实状态。
/// 这份数据表达的是“磁盘现在是什么样”，而不是“上一轮模型以为它是什么样”。
#[derive(Debug, Clone)]
pub enum FileSnapshot {
    Available {
        path: PathBuf,
        content: String,
        last_modified: SystemTime,
        last_modified_by: ModifiedBy,
    },
    Deleted {
        path: PathBuf,
        last_seen_at: SystemTime,
        last_modified_by: ModifiedBy,
    },
    Moved {
        path: PathBuf,
        new_path: PathBuf,
        last_seen_at: SystemTime,
        last_modified_by: ModifiedBy,
    },
}

impl FileSnapshot {
    pub fn available(
        path: impl Into<PathBuf>,
        content: impl Into<String>,
        last_modified: SystemTime,
        last_modified_by: ModifiedBy,
    ) -> Self {
        Self::Available {
            path: path.into(),
            content: content.into(),
            last_modified,
            last_modified_by,
        }
    }

    pub fn deleted(
        path: impl Into<PathBuf>,
        last_seen_at: SystemTime,
        last_modified_by: ModifiedBy,
    ) -> Self {
        Self::Deleted {
            path: path.into(),
            last_seen_at,
            last_modified_by,
        }
    }

    pub fn moved(
        path: impl Into<PathBuf>,
        new_path: impl Into<PathBuf>,
        last_seen_at: SystemTime,
        last_modified_by: ModifiedBy,
    ) -> Self {
        Self::Moved {
            path: path.into(),
            new_path: new_path.into(),
            last_seen_at,
            last_modified_by,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Self::Available { path, .. }
            | Self::Deleted { path, .. }
            | Self::Moved { path, .. } => path.as_path(),
        }
    }

    pub fn last_modified_by(&self) -> ModifiedBy {
        match self {
            Self::Available {
                last_modified_by, ..
            }
            | Self::Deleted {
                last_modified_by, ..
            }
            | Self::Moved {
                last_modified_by, ..
            } => *last_modified_by,
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifiedBy {
    Agent,
    User,
    External,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteEntry {
    pub content: String,
}

impl NoteEntry {
    pub fn new(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
        }
    }
}

/// Session 级环境信息。
/// 这层变化较少，适合放在 prompt 靠前位置，帮助模型先建立工作区方位感。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionStatus {
    pub workspace_root: PathBuf,
    pub platform: String,
    pub shell: String,
    pub available_shells: Vec<String>,
    pub workspace_entries: Vec<String>,
}

impl SessionStatus {
    pub fn is_empty(&self) -> bool {
        self.workspace_root.as_os_str().is_empty()
            && self.platform.is_empty()
            && self.shell.is_empty()
            && self.available_shells.is_empty()
            && self.workspace_entries.is_empty()
    }
}

/// Ma 维护、AI 只读的运行时状态区。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemStatus {
    pub locked_files: Vec<PathBuf>,
    pub context_pressure: Option<ContextPressure>,
}

impl SystemStatus {
    pub fn is_empty(&self) -> bool {
        self.locked_files.is_empty() && self.context_pressure.is_none()
    }
}

pub type RuntimeStatus = SystemStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPressure {
    pub used_percent: u8,
    pub message: String,
}

/// Hints 是外部世界的短期通知。
/// session 每轮构建前清理过期项，构建后再衰减轮次 TTL。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub content: String,
    pub expires_at: Option<SystemTime>,
    pub turns_remaining: Option<u32>,
}

impl Hint {
    pub fn new(
        content: impl Into<String>,
        expires_at: Option<SystemTime>,
        turns_remaining: Option<u32>,
    ) -> Self {
        Self {
            content: content.into(),
            expires_at,
            turns_remaining,
        }
    }

    pub fn is_expired_at(&self, now: SystemTime) -> bool {
        self.expires_at.is_some_and(|expires_at| now >= expires_at)
            || self.turns_remaining == Some(0)
    }

    pub fn tick_turn(&mut self) {
        if let Some(turns_remaining) = &mut self.turns_remaining {
            *turns_remaining = turns_remaining.saturating_sub(1);
        }
    }
}

/// 用户看到的完整会话历史。
/// 这层是展示真相，不参与“给 AI 什么”的裁剪决策。
#[derive(Debug, Clone, Default)]
pub struct ConversationHistory {
    pub turns: Vec<DisplayTurn>,
}

impl ConversationHistory {
    pub fn new(turns: Vec<DisplayTurn>) -> Self {
        Self { turns }
    }
}

#[derive(Debug, Clone)]
pub struct DisplayTurn {
    pub role: Role,
    pub content: String,
    pub tool_calls: Vec<ToolSummary>,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSummary {
    pub name: String,
    pub summary: String,
}

/// recent_chat 只保留人类↔AI 的外部对话，不混入工具执行记录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatTurn {
    pub role: Role,
    pub content: String,
    pub timestamp: SystemTime,
}

pub fn render_chat_turn_for_prompt(turn: &ChatTurn) -> String {
    format!(
        "{:?} @ {}: {}",
        turn.role,
        format_prompt_timestamp(turn.timestamp),
        turn.content
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone)]
pub struct ContextBuildConfig {
    pub max_recent_chat_turns: usize,
}

impl Default for ContextBuildConfig {
    fn default() -> Self {
        Self {
            max_recent_chat_turns: 10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentContextBuilder {
    system_core: String,
    injections: Vec<Injection>,
    tools: Vec<ToolDefinition>,
    open_files: IndexMap<PathBuf, FileSnapshot>,
    notes: IndexMap<String, NoteEntry>,
    session_status: SessionStatus,
    runtime_status: RuntimeStatus,
    hints: Vec<Hint>,
    history: ConversationHistory,
    config: ContextBuildConfig,
}

impl AgentContextBuilder {
    /// Builder 的职责是把稳定层与可变层重新拼成一轮 AgentContext。
    pub fn new(system_core: impl Into<String>) -> Self {
        Self {
            system_core: system_core.into(),
            injections: Vec::new(),
            tools: Vec::new(),
            open_files: IndexMap::new(),
            notes: IndexMap::new(),
            session_status: SessionStatus::default(),
            runtime_status: RuntimeStatus::default(),
            hints: Vec::new(),
            history: ConversationHistory::default(),
            config: ContextBuildConfig::default(),
        }
    }

    pub fn with_config(mut self, config: ContextBuildConfig) -> Self {
        self.config = config;
        self
    }

    pub fn injections(mut self, injections: Vec<Injection>) -> Self {
        self.injections = injections;
        self
    }

    pub fn tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn open_file(mut self, snapshot: FileSnapshot) -> Self {
        self.open_files
            .insert(snapshot.path().to_path_buf(), snapshot);
        self
    }

    pub fn build_from_open_files(
        mut self,
        snapshots: IndexMap<PathBuf, FileSnapshot>,
    ) -> AgentContext {
        self.open_files = snapshots;
        self.build()
    }

    pub fn notes(mut self, notes: IndexMap<String, NoteEntry>) -> Self {
        self.notes = notes;
        self
    }

    pub fn session_status(mut self, session_status: SessionStatus) -> Self {
        self.session_status = session_status;
        self
    }

    pub fn runtime_status(mut self, runtime_status: RuntimeStatus) -> Self {
        self.runtime_status = runtime_status;
        self
    }

    pub fn hints(mut self, hints: Vec<Hint>) -> Self {
        self.hints = hints;
        self
    }

    pub fn history(mut self, history: ConversationHistory) -> Self {
        self.history = history;
        self
    }

    pub fn build(self) -> AgentContext {
        let max_recent_chat_turns = self.config.max_recent_chat_turns.max(1);
        let recent_chat = self
            .history
            .turns
            .into_iter()
            .filter_map(|turn| ChatTurn::from_display_turn(&turn))
            .rev()
            .take(max_recent_chat_turns)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();

        AgentContext {
            system_core: self.system_core,
            injections: self.injections,
            tools: self.tools,
            open_files: self.open_files,
            notes: self.notes,
            session_status: self.session_status,
            runtime_status: self.runtime_status,
            hints: self.hints,
            recent_chat,
        }
    }
}

impl ChatTurn {
    fn from_display_turn(turn: &DisplayTurn) -> Option<Self> {
        match turn.role {
            Role::User | Role::Assistant => Some(Self {
                role: turn.role,
                content: turn.content.clone(),
                timestamp: turn.timestamp,
            }),
            Role::System | Role::Tool => None,
        }
    }
}

const MAX_RENDER_LINES: usize = 2_000;
const MAX_LINE_CHARS: usize = 1_000;
const MAX_RENDER_BYTES: usize = 100 * 1024;

fn render_file_content_for_prompt(content: &str) -> String {
    let lines = content.lines().collect::<Vec<_>>();
    let total_lines = lines.len();
    let mut rendered = String::new();
    let mut emitted_lines = 0usize;

    if total_lines > MAX_RENDER_LINES {
        rendered.push_str(&format!(
            "[文件共 {} 行，仅显示前 {} 行。如需查看其他部分，请使用 run_command 辅助定位。]\n",
            total_lines, MAX_RENDER_LINES
        ));
    }

    for (index, line) in lines.into_iter().enumerate() {
        if emitted_lines >= MAX_RENDER_LINES || rendered.len() >= MAX_RENDER_BYTES {
            break;
        }

        let truncated_line = truncate_line_for_prompt(line);
        let next = format!("{:>4} | {}\n", index + 1, truncated_line);
        if rendered.len() + next.len() > MAX_RENDER_BYTES {
            rendered.push_str("[内容过长，已在 100KB 处截断。]\n");
            break;
        }

        rendered.push_str(&next);
        emitted_lines += 1;
    }

    if total_lines == 0 {
        rendered.push_str("[空文件]\n");
    }

    rendered
}

fn truncate_line_for_prompt(line: &str) -> String {
    let char_count = line.chars().count();
    if char_count <= MAX_LINE_CHARS {
        return line.to_string();
    }

    let truncated = line.chars().take(MAX_LINE_CHARS).collect::<String>();
    format!("{truncated}...[+{} chars]", char_count - MAX_LINE_CHARS)
}

fn format_prompt_timestamp(timestamp: SystemTime) -> String {
    let datetime = DateTime::<Local>::from(timestamp);
    datetime.format("%Y-%m-%d %H:%M:%S %:z").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_keeps_only_recent_external_chat_turns() {
        let history = ConversationHistory::new(vec![
            display_turn(Role::User, "first"),
            display_turn(Role::Tool, "tool output"),
            display_turn(Role::Assistant, "second"),
            display_turn(Role::User, "third"),
            display_turn(Role::Assistant, "fourth"),
        ]);

        let context = AgentContextBuilder::new("system").history(history).build();

        assert_eq!(context.recent_chat.len(), 4);
        assert_eq!(context.recent_chat[0].content, "first");
        assert_eq!(context.recent_chat[1].content, "second");
        assert_eq!(context.recent_chat[2].content, "third");
        assert_eq!(context.recent_chat[3].content, "fourth");
    }

    #[test]
    fn prompt_chat_render_includes_role_and_timestamp() {
        let rendered = render_chat_turn_for_prompt(&ChatTurn {
            role: Role::User,
            content: "hello".to_string(),
            timestamp: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(42),
        });

        assert!(rendered.starts_with("User @ "));
        assert!(rendered.ends_with(": hello"));
        assert!(!rendered.contains("unix:"));
    }

    #[test]
    fn open_files_keep_insertion_order() {
        let first = FileSnapshot::available(
            "src/main.rs",
            "fn main() {}",
            SystemTime::UNIX_EPOCH,
            ModifiedBy::Unknown,
        );
        let second = FileSnapshot::available(
            "src/lib.rs",
            "pub mod context;",
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10),
            ModifiedBy::Agent,
        );

        let context = AgentContextBuilder::new("system")
            .open_file(first)
            .open_file(second)
            .build();

        let ordered = context.open_files_in_prompt_order();

        assert_eq!(ordered[0].path(), PathBuf::from("src/main.rs"));
        assert_eq!(ordered[1].path(), PathBuf::from("src/lib.rs"));
    }

    #[test]
    fn hints_support_time_and_turn_expiration() {
        let now = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10);
        let mut hint = Hint::new(
            "temporary",
            Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(20)),
            Some(2),
        );

        assert!(!hint.is_expired_at(now));

        hint.tick_turn();
        assert_eq!(hint.turns_remaining, Some(1));
        assert!(!hint.is_expired_at(now));

        hint.tick_turn();
        assert!(hint.is_expired_at(now));
    }

    fn display_turn(role: Role, content: &str) -> DisplayTurn {
        DisplayTurn {
            role,
            content: content.to_string(),
            tool_calls: Vec::new(),
            timestamp: SystemTime::UNIX_EPOCH,
        }
    }
}
