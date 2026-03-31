use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

/// 每轮真正发给模型的上下文视图。
/// 这里刻意和“用户看到的完整历史”分离，方便后续做裁剪、压缩和 prefix cache 优化。
#[derive(Debug, Clone)]
pub struct AgentContext {
    pub system: String,
    pub watched_files: BTreeMap<PathBuf, FileSnapshot>,
    pub messages: Vec<Message>,
}

impl AgentContext {
    /// 按 prompt 构建顺序返回文件快照：
    /// 稳定文件优先，最近变过的文件靠后，以减少前缀缓存被频繁打断。
    pub fn watched_files_in_prompt_order(&self) -> Vec<&FileSnapshot> {
        let mut stable = Vec::new();
        let mut changed = Vec::new();

        for snapshot in self.watched_files.values() {
            if snapshot.has_changed_since_watch {
                changed.push(snapshot);
            } else {
                stable.push(snapshot);
            }
        }

        stable.sort_by(|left, right| left.path.cmp(&right.path));
        changed.sort_by(|left, right| left.last_modified.cmp(&right.last_modified));

        stable.into_iter().chain(changed).collect()
    }
}

/// watcher 维护的单文件真实状态。
/// 这份数据的目标不是表达“模型上一次怎么改的”，而是表达“磁盘现在是什么样”。
#[derive(Debug, Clone)]
pub struct FileSnapshot {
    pub path: PathBuf,
    pub content: String,
    pub last_modified: SystemTime,
    pub last_modified_by: ModifiedBy,
    pub has_changed_since_watch: bool,
}

impl FileSnapshot {
    /// Unknown 表示只是初始加载，不代表文件“未变”；
    /// 这里只是避免把初次注册也误当成一次有效修改。
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

/// 给用户展示的完整会话历史。
/// 后续即使做摘要、压缩，原始展示层也应该独立保留。
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct ToolSummary {
    pub name: String,
    pub summary: String,
}

#[derive(Debug, Clone)]
pub struct Message {
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
    pub max_recent_turns: usize,
}

impl Default for ContextBuildConfig {
    fn default() -> Self {
        Self {
            max_recent_turns: 6,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AgentContextBuilder {
    system: String,
    watched_files: BTreeMap<PathBuf, FileSnapshot>,
    history: ConversationHistory,
    config: ContextBuildConfig,
}

impl AgentContextBuilder {
    /// Builder 的职责是把稳定 system、最新文件快照和裁剪后的消息历史重新拼成一轮上下文。
    pub fn new(system: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            watched_files: BTreeMap::new(),
            history: ConversationHistory { turns: Vec::new() },
            config: ContextBuildConfig::default(),
        }
    }

    pub fn with_config(mut self, config: ContextBuildConfig) -> Self {
        self.config = config;
        self
    }

    pub fn watch_file(mut self, snapshot: FileSnapshot) -> Self {
        self.watched_files.insert(snapshot.path.clone(), snapshot);
        self
    }

    pub fn build_from_snapshots(
        mut self,
        snapshots: BTreeMap<PathBuf, FileSnapshot>,
    ) -> AgentContext {
        // 允许上层直接把 watcher 当前维护的整份快照表灌进来，
        // 避免 session 层重复逐个 watch_file 调 builder。
        self.watched_files = snapshots;
        self.build()
    }

    pub fn history(mut self, history: ConversationHistory) -> Self {
        self.history = history;
        self
    }

    pub fn build(self) -> AgentContext {
        // 当前阶段先用“保留最近 N 轮”的朴素策略，
        // 后面可以在这里替换成摘要、分层保留等更复杂的压缩逻辑。
        let max_recent_turns = self.config.max_recent_turns.max(1);
        let kept_turns = self
            .history
            .turns
            .into_iter()
            .rev()
            .take(max_recent_turns)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|turn| Message {
                role: turn.role,
                content: summarize_turn(&turn),
                timestamp: turn.timestamp,
            })
            .collect();

        AgentContext {
            system: self.system,
            watched_files: self.watched_files,
            messages: kept_turns,
        }
    }
}

/// 对用户可见 turn 做轻量摘要，让工具信息能进入模型上下文，
/// 同时避免未来把完整工具原始输出无差别塞回 prompt。
fn summarize_turn(turn: &DisplayTurn) -> String {
    if turn.tool_calls.is_empty() {
        return turn.content.clone();
    }

    let tool_summaries = turn
        .tool_calls
        .iter()
        .map(|tool| format!("{}: {}", tool.name, tool.summary))
        .collect::<Vec<_>>()
        .join("; ");

    format!("{}\n\nTool summary: {}", turn.content, tool_summaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_keeps_only_recent_turns() {
        let history = ConversationHistory::new(vec![
            display_turn("first"),
            display_turn("second"),
            display_turn("third"),
        ]);

        let context = AgentContextBuilder::new("system")
            .with_config(ContextBuildConfig {
                max_recent_turns: 2,
            })
            .history(history)
            .build();

        assert_eq!(context.messages.len(), 2);
        assert_eq!(context.messages[0].content, "second");
        assert_eq!(context.messages[1].content, "third");
    }

    #[test]
    fn prompt_order_puts_changed_files_last() {
        let stable = FileSnapshot::new(
            "src/lib.rs",
            "pub mod context;",
            SystemTime::UNIX_EPOCH,
            ModifiedBy::Unknown,
        );
        let changed = FileSnapshot::new(
            "src/main.rs",
            "fn main() {}",
            SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(10),
            ModifiedBy::Agent,
        );

        let context = AgentContextBuilder::new("system")
            .watch_file(changed)
            .watch_file(stable)
            .build();

        let ordered = context.watched_files_in_prompt_order();

        assert_eq!(ordered[0].path, PathBuf::from("src/lib.rs"));
        assert_eq!(ordered[1].path, PathBuf::from("src/main.rs"));
    }

    fn display_turn(content: &str) -> DisplayTurn {
        DisplayTurn {
            role: Role::User,
            content: content.to_string(),
            tool_calls: Vec::new(),
            timestamp: SystemTime::UNIX_EPOCH,
        }
    }
}
