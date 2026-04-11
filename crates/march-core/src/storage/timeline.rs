use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::context::{ContentBlock, ConversationHistory, DisplayTurn, Role, ToolSummary};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistedReplyRef {
    Turn { id: String },
    UserMessage { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistedTurnTrigger {
    User { id: String },
    Turn { id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedAssistantMessageState {
    Streaming,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedTurnState {
    Streaming,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedToolCallState {
    Running,
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistedAssistantTimelineEntry {
    Text {
        text: String,
    },
    Tool {
        tool_call_id: String,
        tool_name: String,
        arguments: String,
        status: PersistedToolCallState,
        preview: Option<String>,
        duration_ms: Option<u64>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedAssistantMessage {
    pub message_id: String,
    pub turn_id: String,
    pub state: PersistedAssistantMessageState,
    pub reasoning: String,
    pub timeline: Vec<PersistedAssistantTimelineEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedUserMessage {
    pub user_message_id: String,
    pub content: Vec<ContentBlock>,
    pub mentions: Vec<String>,
    pub replies: Vec<PersistedReplyRef>,
    pub timestamp: SystemTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedTurn {
    pub turn_id: String,
    pub agent_id: String,
    pub trigger: PersistedTurnTrigger,
    pub state: PersistedTurnState,
    pub error_message: Option<String>,
    pub timestamp: SystemTime,
    pub messages: Vec<PersistedAssistantMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersistedTaskTimelineEntry {
    UserMessage(PersistedUserMessage),
    Turn(PersistedTurn),
}

pub type PersistedTaskTimeline = Vec<PersistedTaskTimelineEntry>;

pub fn history_from_timeline(timeline: &[PersistedTaskTimelineEntry]) -> ConversationHistory {
    let mut turns = Vec::new();

    for entry in timeline {
        match entry {
            PersistedTaskTimelineEntry::UserMessage(message) => {
                turns.push(DisplayTurn {
                    role: Role::User,
                    agent: "user".to_string(),
                    content: message.content.clone(),
                    tool_calls: Vec::new(),
                    timestamp: message.timestamp,
                });
            }
            PersistedTaskTimelineEntry::Turn(turn) => {
                let content = flatten_turn_text(turn);
                let tool_calls = flatten_turn_tool_summaries(turn);
                turns.push(DisplayTurn {
                    role: Role::Assistant,
                    agent: turn.agent_id.clone(),
                    content: vec![ContentBlock::text(content)],
                    tool_calls,
                    timestamp: turn.timestamp,
                });
            }
        }
    }

    ConversationHistory::new(turns)
}

pub fn turn_agent_id<'a>(
    timeline: &'a [PersistedTaskTimelineEntry],
    turn_id: &str,
) -> Option<&'a str> {
    timeline.iter().find_map(|entry| match entry {
        PersistedTaskTimelineEntry::Turn(turn) if turn.turn_id == turn_id => {
            Some(turn.agent_id.as_str())
        }
        PersistedTaskTimelineEntry::Turn(_) => None,
        PersistedTaskTimelineEntry::UserMessage(_) => None,
    })
}

fn flatten_turn_text(turn: &PersistedTurn) -> String {
    turn.messages
        .iter()
        .flat_map(|message| message.timeline.iter())
        .filter_map(|entry| match entry {
            PersistedAssistantTimelineEntry::Text { text } => Some(text.as_str()),
            PersistedAssistantTimelineEntry::Tool { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

fn flatten_turn_tool_summaries(turn: &PersistedTurn) -> Vec<ToolSummary> {
    turn.messages
        .iter()
        .flat_map(|message| message.timeline.iter())
        .filter_map(|entry| match entry {
            PersistedAssistantTimelineEntry::Tool {
                tool_name, preview, ..
            } => Some(ToolSummary {
                name: tool_name.clone(),
                summary: preview.clone().unwrap_or_else(|| tool_name.clone()),
            }),
            PersistedAssistantTimelineEntry::Text { .. } => None,
        })
        .collect()
}
