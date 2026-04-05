use anyhow::{Context, Result};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json::Value;

use crate::settings::ProviderType;

use super::RuntimeProviderConfig;

pub(super) async fn list_model_descriptors(
    http: &HttpClient,
    config: &RuntimeProviderConfig,
) -> Result<Vec<ModelDescriptor>> {
    let Some(endpoint) = model_list_endpoint(config) else {
        return Ok(Vec::new());
    };

    let mut request = http.get(endpoint);
    if !config.api_key.trim().is_empty() {
        request = match config.provider_type {
            ProviderType::Anthropic => request.header("x-api-key", &config.api_key),
            ProviderType::Gemini => request,
            _ => request.bearer_auth(&config.api_key),
        };
    }
    if config.provider_type == ProviderType::Anthropic {
        request = request.header("anthropic-version", "2023-06-01");
    }

    let response = request
        .send()
        .await
        .context("failed to request model list")?
        .error_for_status()
        .context("model list request failed")?;

    let payload: ModelListResponse = response
        .json()
        .await
        .context("failed to decode model list response")?;

    payload
        .data
        .into_iter()
        .map(ModelDescriptor::from_value)
        .collect()
}

pub(super) fn provider_base_url(config: &RuntimeProviderConfig) -> String {
    config
        .base_url
        .as_deref()
        .unwrap_or_else(|| default_base_url_for_provider(config.provider_type))
        .trim_end_matches('/')
        .to_string()
}

pub(super) fn default_base_url_for_provider(provider_type: ProviderType) -> &'static str {
    match provider_type {
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

fn model_list_endpoint(config: &RuntimeProviderConfig) -> Option<String> {
    match config.provider_type {
        ProviderType::Anthropic => None,
        ProviderType::Gemini => None,
        _ => Some(format!("{}/models", provider_base_url(config))),
    }
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    #[serde(default)]
    data: Vec<Value>,
}

#[derive(Debug, Clone)]
pub(super) struct ModelDescriptor {
    pub id: String,
    pub context_window_tokens: Option<usize>,
}

impl ModelDescriptor {
    fn from_value(value: Value) -> Result<Self> {
        let object = value
            .as_object()
            .context("provider model entry was not an object")?;
        let id = object
            .get("id")
            .and_then(Value::as_str)
            .context("provider model entry missing string id")?
            .to_string();

        let context_window_tokens = [
            object.get("context_window"),
            object.get("max_input_tokens"),
            object.get("input_token_limit"),
        ]
        .into_iter()
        .flatten()
        .find_map(parse_context_window_value);

        Ok(Self {
            id,
            context_window_tokens,
        })
    }
}

fn parse_context_window_value(value: &Value) -> Option<usize> {
    match value {
        Value::Number(number) => number
            .as_u64()
            .and_then(|value| usize::try_from(value).ok()),
        Value::String(text) => text.trim().parse::<usize>().ok(),
        _ => None,
    }
}
