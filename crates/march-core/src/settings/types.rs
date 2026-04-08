use std::time::SystemTime;

/// ProviderType 对应设置页和运行时可选的 provider 入口。
/// 旧版本只有 OpenAI-compatible，这里保留 compat 作为自定义端点类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderType {
    OpenAiCompat,
    OpenAi,
    Anthropic,
    Gemini,
    Fireworks,
    Together,
    Groq,
    Mimo,
    Nebius,
    Xai,
    DeepSeek,
    Zai,
    BigModel,
    Cohere,
    Ollama,
}

impl ProviderType {
    pub fn as_db_value(self) -> &'static str {
        match self {
            ProviderType::OpenAiCompat => "openai_compat",
            ProviderType::OpenAi => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::Gemini => "gemini",
            ProviderType::Fireworks => "fireworks",
            ProviderType::Together => "together",
            ProviderType::Groq => "groq",
            ProviderType::Mimo => "mimo",
            ProviderType::Nebius => "nebius",
            ProviderType::Xai => "xai",
            ProviderType::DeepSeek => "deepseek",
            ProviderType::Zai => "zai",
            ProviderType::BigModel => "bigmodel",
            ProviderType::Cohere => "cohere",
            ProviderType::Ollama => "ollama",
        }
    }

    pub fn from_db_value(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai_compat" => Some(Self::OpenAiCompat),
            "openai" => Some(Self::OpenAi),
            "anthropic" => Some(Self::Anthropic),
            "gemini" => Some(Self::Gemini),
            "fireworks" => Some(Self::Fireworks),
            "together" => Some(Self::Together),
            "groq" => Some(Self::Groq),
            "mimo" => Some(Self::Mimo),
            "nebius" => Some(Self::Nebius),
            "xai" => Some(Self::Xai),
            "deepseek" => Some(Self::DeepSeek),
            "zai" => Some(Self::Zai),
            "bigmodel" => Some(Self::BigModel),
            "cohere" => Some(Self::Cohere),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }

    pub fn requires_api_key(self) -> bool {
        !matches!(self, Self::Ollama)
    }

    pub fn base_url_required(self) -> bool {
        matches!(self, Self::OpenAiCompat)
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            ProviderType::OpenAiCompat | ProviderType::OpenAi => "https://api.openai.com/v1",
            ProviderType::Anthropic => "https://api.anthropic.com/v1",
            ProviderType::Gemini => "https://generativelanguage.googleapis.com/v1beta",
            ProviderType::Fireworks => "https://api.fireworks.ai/inference/v1",
            ProviderType::Together => "https://api.together.xyz/v1",
            ProviderType::Groq => "https://api.groq.com/openai/v1",
            ProviderType::Mimo => "https://api.mimo.org/v1",
            ProviderType::Nebius => "https://api.studio.nebius.com/v1",
            ProviderType::Xai => "https://api.x.ai/v1",
            ProviderType::DeepSeek => "https://api.deepseek.com/v1",
            ProviderType::Zai => "https://api.z.ai/api/paas/v4",
            ProviderType::BigModel => "https://open.bigmodel.cn/api/paas/v4",
            ProviderType::Cohere => "https://api.cohere.com/v2",
            ProviderType::Ollama => "http://localhost:11434/v1",
        }
    }

    pub fn uses_anthropic_api(self) -> bool {
        matches!(self, Self::Anthropic)
    }

    pub fn uses_gemini_api(self) -> bool {
        matches!(self, Self::Gemini)
    }

    pub fn uses_openai_responses_api(self) -> bool {
        matches!(self, Self::OpenAi)
    }

    pub fn uses_openai_api(self) -> bool {
        !self.uses_anthropic_api() && !self.uses_gemini_api()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerToolCapability {
    WebSearch,
    CodeExecution,
    FileSearch,
}

impl ServerToolCapability {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::WebSearch => "web_search",
            Self::CodeExecution => "code_execution",
            Self::FileSearch => "file_search",
        }
    }

    pub fn from_db_value(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "web_search" => Some(Self::WebSearch),
            "code_execution" => Some(Self::CodeExecution),
            "file_search" => Some(Self::FileSearch),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerToolFormat {
    Anthropic,
    OpenAiResponses,
    OpenAiChatCompletions,
    Gemini,
}

impl ServerToolFormat {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAiResponses => "openai_responses",
            Self::OpenAiChatCompletions => "openai_chat_completions",
            Self::Gemini => "gemini",
        }
    }

    pub fn from_db_value(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Some(Self::Anthropic),
            "openai_responses" => Some(Self::OpenAiResponses),
            "openai_chat_completions" => Some(Self::OpenAiChatCompletions),
            "gemini" => Some(Self::Gemini),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerToolConfig {
    pub capability: ServerToolCapability,
    pub format: ServerToolFormat,
}

#[derive(Debug, Clone)]
pub struct ProviderRecord {
    pub id: i64,
    pub name: String,
    pub provider_type: ProviderType,
    pub api_key: String,
    pub base_url: Option<String>,
    pub created_at: SystemTime,
}

#[derive(Debug, Clone)]
pub struct ModelConfigRecord {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub supports_tool_use: bool,
    pub supports_vision: bool,
    pub supports_audio: bool,
    pub supports_pdf: bool,
    pub probed_at: Option<i64>,
    pub server_tools: Vec<ServerToolConfig>,
}

pub type ProviderModelRecord = ModelConfigRecord;

#[derive(Debug, Clone)]
pub struct ProviderSettingsSnapshot {
    pub providers: Vec<ProviderRecord>,
    pub model_configs: Vec<ModelConfigRecord>,
    pub agent_profiles: Vec<AgentProfileRecord>,
    pub default_model_config_id: Option<i64>,
    pub custom_system_core: Option<String>,
    pub use_custom_system_core: bool,
}

#[derive(Debug, Clone)]
pub struct AgentProfileRecord {
    pub id: i64,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub system_prompt: String,
    pub avatar_color: String,
    pub model_config_id: Option<i64>,
    pub provider_id: Option<i64>,
    pub model_id: Option<String>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
}
