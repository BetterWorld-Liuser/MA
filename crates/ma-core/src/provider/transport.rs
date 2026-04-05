use anyhow::{Context, Result};
use genai::adapter::AdapterKind;
use genai::resolver::{AuthData, Endpoint};
use genai::{ModelIden, ServiceTarget};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json::Value;

use crate::settings::ProviderType;

use super::RuntimeProviderConfig;

pub(super) async fn list_model_descriptors(
    http: &HttpClient,
    config: &RuntimeProviderConfig,
) -> Result<Vec<ModelDescriptor>> {
    let base_url = config
        .base_url
        .as_deref()
        .context("provider base url is required for model list")?;
    let mut request = http.get(format!("{}/models", base_url));
    if !config.api_key.trim().is_empty() {
        request = request.bearer_auth(&config.api_key);
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

pub(super) fn build_service_target(config: &RuntimeProviderConfig) -> ServiceTarget {
    let adapter_kind = adapter_kind_for_provider(config.provider_type, &config.model);
    let endpoint = config
        .base_url
        .as_ref()
        .map(|url| Endpoint::from_owned(endpoint_base_url_for_genai(url)))
        .unwrap_or_else(|| default_endpoint_for_provider(config.provider_type));
    let auth = if config.api_key.trim().is_empty() {
        AuthData::from_single("ollama")
    } else {
        AuthData::from_single(config.api_key.clone())
    };

    ServiceTarget {
        endpoint,
        auth,
        model: ModelIden::new(adapter_kind, config.model.clone()),
    }
}

fn endpoint_base_url_for_genai(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.ends_with('/') {
        trimmed.to_string()
    } else {
        format!("{trimmed}/")
    }
}

fn adapter_kind_for_provider(provider_type: ProviderType, model: &str) -> AdapterKind {
    match provider_type {
        ProviderType::OpenAiCompat => AdapterKind::OpenAI,
        ProviderType::OpenAi => AdapterKind::from_model(model).unwrap_or(AdapterKind::OpenAI),
        ProviderType::Anthropic => AdapterKind::Anthropic,
        ProviderType::Gemini => AdapterKind::Gemini,
        ProviderType::Fireworks => AdapterKind::Fireworks,
        ProviderType::Together => AdapterKind::Together,
        ProviderType::Groq => AdapterKind::Groq,
        ProviderType::Mimo => AdapterKind::Mimo,
        ProviderType::Nebius => AdapterKind::Nebius,
        ProviderType::Xai => AdapterKind::Xai,
        ProviderType::DeepSeek => AdapterKind::DeepSeek,
        ProviderType::Zai => AdapterKind::Zai,
        ProviderType::BigModel => AdapterKind::BigModel,
        ProviderType::Cohere => AdapterKind::Cohere,
        ProviderType::Ollama => AdapterKind::Ollama,
    }
}

fn default_endpoint_for_provider(provider_type: ProviderType) -> Endpoint {
    match provider_type {
        ProviderType::OpenAiCompat | ProviderType::OpenAi => {
            Endpoint::from_static("https://api.openai.com/v1/")
        }
        ProviderType::Anthropic => Endpoint::from_static("https://api.anthropic.com/v1/"),
        ProviderType::Gemini => {
            Endpoint::from_static("https://generativelanguage.googleapis.com/v1beta/")
        }
        ProviderType::Fireworks => Endpoint::from_static("https://api.fireworks.ai/inference/v1/"),
        ProviderType::Together => Endpoint::from_static("https://api.together.xyz/v1/"),
        ProviderType::Groq => Endpoint::from_static("https://api.groq.com/openai/v1/"),
        ProviderType::Mimo => Endpoint::from_static("https://api.mimo.org/v1/"),
        ProviderType::Nebius => Endpoint::from_static("https://api.studio.nebius.com/v1/"),
        ProviderType::Xai => Endpoint::from_static("https://api.x.ai/v1/"),
        ProviderType::DeepSeek => Endpoint::from_static("https://api.deepseek.com/v1/"),
        ProviderType::Zai => Endpoint::from_static("https://api.z.ai/api/paas/v4/"),
        ProviderType::BigModel => Endpoint::from_static("https://open.bigmodel.cn/api/paas/v4/"),
        ProviderType::Cohere => Endpoint::from_static("https://api.cohere.com/v2/"),
        ProviderType::Ollama => Endpoint::from_static("http://localhost:11434/v1/"),
    }
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
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
