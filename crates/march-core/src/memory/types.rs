use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLevel {
    Project,
    Global,
}

impl MemoryLevel {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Project => "p",
            Self::Global => "g",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "agent", rename_all = "snake_case")]
pub enum MemoryScope {
    Shared,
    Agent(String),
}

impl MemoryScope {
    pub fn from_storage(raw: &str) -> Self {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("shared") {
            Self::Shared
        } else {
            Self::Agent(trimmed.to_string())
        }
    }

    pub fn as_storage(&self) -> String {
        match self {
            Self::Shared => "shared".to_string(),
            Self::Agent(name) => name.clone(),
        }
    }

    pub fn is_visible_to(&self, active_agent: &str) -> bool {
        matches!(self, Self::Shared)
            || matches!(self, Self::Agent(name) if name.eq_ignore_ascii_case(active_agent))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: String,
    pub level: MemoryLevel,
    pub scope: MemoryScope,
    pub stable_id: String,
    pub memory_type: String,
    pub topic: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub access_count: u32,
    pub skip_count: u32,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}

impl MemoryRecord {
    pub fn prefixed_id(&self) -> String {
        format!("{}:{}", self.level.prefix(), self.id)
    }

    pub(super) fn normalized_tags(&self) -> HashSet<String> {
        self.tags
            .iter()
            .flat_map(|tag| super::search::tokenize_terms(tag))
            .collect::<HashSet<_>>()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryIndexEntry {
    pub id: String,
    pub memory_type: String,
    pub topic: String,
    pub title: String,
    pub level: MemoryLevel,
    pub score: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryIndexView {
    pub entries: Vec<MemoryIndexEntry>,
    pub topic_warnings: Vec<String>,
}

impl MemoryIndexView {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn render_for_prompt(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        output.push_str(&format!(
            "Matched {} relevant memories. Use recall_memory(id) for full details.\n\n",
            self.entries.len()
        ));

        for entry in &self.entries {
            output.push_str(&format!(
                "  {:<24} [{:<10}] {:<12} {}\n",
                entry.id, entry.memory_type, entry.topic, entry.title
            ));
        }

        if !self.topic_warnings.is_empty() {
            output.push('\n');
            for warning in &self.topic_warnings {
                output.push_str(&format!("! {warning}\n"));
            }
        }

        output.trim_end().to_string()
    }
}

#[derive(Debug, Clone, Default)]
pub struct MemoryQuery {
    pub task_name: Option<String>,
    pub latest_user_message: Option<String>,
    pub open_file_paths: Vec<PathBuf>,
    pub recent_assistant_messages: Vec<String>,
    pub active_agent: String,
    pub context_pressure_percent: Option<u8>,
}

#[derive(Debug, Clone)]
pub struct MemorizeRequest {
    pub id: String,
    pub memory_type: String,
    pub topic: String,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub scope: Option<String>,
    pub level: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct UpdateMemoryRequest {
    pub id: String,
    pub title: Option<String>,
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub topic: Option<String>,
    pub memory_type: Option<String>,
}
