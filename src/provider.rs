use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// OpenAI-compatible provider 的最小运行时配置。
/// 先通过环境变量注入，避免把敏感信息硬编码进仓库。
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleConfig {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

impl OpenAiCompatibleConfig {
    /// 这里约定所有 provider 相关配置都走 MA_OPENAI_* 前缀，
    /// 方便本地 `.env` 和未来多环境注入保持一致。
    pub fn from_env() -> Result<Self> {
        let base_url = std::env::var("MA_OPENAI_BASE_URL")
            .context("missing MA_OPENAI_BASE_URL environment variable")?;
        let api_key = std::env::var("MA_OPENAI_API_KEY")
            .context("missing MA_OPENAI_API_KEY environment variable")?;
        let model = std::env::var("MA_OPENAI_MODEL")
            .context("missing MA_OPENAI_MODEL environment variable")?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key,
            model,
        })
    }
}

/// 一个刻意做薄的兼容层：
/// 只封装当前阶段必需的 endpoints，避免过早把 provider 能力做得过重。
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleClient {
    http: Client,
    config: OpenAiCompatibleConfig,
}

impl OpenAiCompatibleClient {
    pub fn new(config: OpenAiCompatibleConfig) -> Self {
        Self {
            http: Client::new(),
            config,
        }
    }

    /// list_models 主要用于 smoke test 和联通性检查。
    pub async fn list_models(&self) -> Result<Vec<String>> {
        let response = self
            .http
            .get(format!("{}/models", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .context("failed to request model list")?
            .error_for_status()
            .context("model list request failed")?;

        let payload: ModelListResponse = response
            .json()
            .await
            .context("failed to decode model list response")?;

        Ok(payload.data.into_iter().map(|model| model.id).collect())
    }

    /// 当前实现直接走 chat completions 文本模式，
    /// 这样最容易和现有 AgentSession 的纯文本 prompt/output 对上。
    pub async fn complete_text(&self, system: &str, prompt: &str) -> Result<String> {
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            temperature: Some(0.2),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: prompt.to_string(),
                },
            ],
        };

        let response = self
            .http
            .post(format!("{}/chat/completions", self.config.base_url))
            .bearer_auth(&self.config.api_key)
            .json(&request)
            .send()
            .await
            .context("failed to request chat completion")?
            .error_for_status()
            .context("chat completion request failed")?;

        let payload: ChatCompletionResponse = response
            .json()
            .await
            .context("failed to decode chat completion response")?;
        let Some(choice) = payload.choices.into_iter().next() else {
            bail!("chat completion returned no choices");
        };

        Ok(choice.message.content)
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    data: Vec<ModelInfo>,
}

#[derive(Debug, Deserialize)]
struct ModelInfo {
    id: String,
}
