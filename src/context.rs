use std::path::PathBuf;
use std::time::SystemTime;

use indexmap::IndexMap;

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
    pub notes: IndexMap<String, String>,
    pub recent_chat: Vec<ChatTurn>,
}

impl AgentContext {
    /// open_files 需要保序，因此这里返回其当前插入顺序视图，
    /// 而不是再做一次额外排序，避免 prompt 前缀无意义抖动。
    pub fn open_files_in_prompt_order(&self) -> Vec<&FileSnapshot> {
        self.open_files.values().collect()
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
pub struct FileSnapshot {
    pub path: PathBuf,
    pub content: String,
    pub last_modified: SystemTime,
    pub last_modified_by: ModifiedBy,
    pub has_changed_since_watch: bool,
}

impl FileSnapshot {
    pub fn new(
        path: impl Into<PathBuf>,
        content: impl Into<String>,
        last_modified: SystemTime,
        last_modified_by: ModifiedBy,
    ) -> Self {
        Self {
            path: path.into(),
            content: content.into(),
            last_modified,
            has_changed_since_watch: last_modified_by != ModifiedBy::Unknown,
            last_modified_by,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifiedBy {
    Agent,
    User,
    External,
    Unknown,
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
            max_recent_chat_turns: 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentContextBuilder {
    system_core: String,
    injections: Vec<Injection>,
    tools: Vec<ToolDefinition>,
    open_files: IndexMap<PathBuf, FileSnapshot>,
    notes: IndexMap<String, String>,
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
        self.open_files.insert(snapshot.path.clone(), snapshot);
        self
    }

    pub fn build_from_open_files(
        mut self,
        snapshots: IndexMap<PathBuf, FileSnapshot>,
    ) -> AgentContext {
        self.open_files = snapshots;
        self.build()
    }

    pub fn notes(mut self, notes: IndexMap<String, String>) -> Self {
        self.notes = notes;
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

        let context = AgentContextBuilder::new("system")
            .history(history)
            .build();

        assert_eq!(context.recent_chat.len(), 3);
        assert_eq!(context.recent_chat[0].content, "second");
        assert_eq!(context.recent_chat[1].content, "third");
        assert_eq!(context.recent_chat[2].content, "fourth");
    }

    #[test]
    fn open_files_keep_insertion_order() {
        let first = FileSnapshot::new(
            "src/main.rs",
            "fn main() {}",
            SystemTime::UNIX_EPOCH,
            ModifiedBy::Unknown,
        );
        let second = FileSnapshot::new(
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

        assert_eq!(ordered[0].path, PathBuf::from("src/main.rs"));
        assert_eq!(ordered[1].path, PathBuf::from("src/lib.rs"));
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
