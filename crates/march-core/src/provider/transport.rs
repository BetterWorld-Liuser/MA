use anyhow::{Context, Result};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json::Value;

use crate::settings::{ProviderType, normalize_provider_base_url};

use super::RuntimeProviderConfig;

pub(super) async fn list_model_descriptors(
    http: &HttpClient,
    config: &RuntimeProviderConfig,
) -> Result<Vec<ModelDescriptor>> {
    let Some(mut endpoint) = model_list_endpoint(config) else {
        return Ok(Vec::new());
    };
    if config.provider_type.uses_gemini_api() && !config.api_key.trim().is_empty() {
        endpoint.push_str("?key=");
        endpoint.push_str(&config.api_key);
    }

    let mut request = http.get(endpoint);
    if !config.api_key.trim().is_empty() {
        request = if config.provider_type.uses_anthropic_api() {
            request.header("x-api-key", &config.api_key)
        } else if config.provider_type.uses_gemini_api() {
            request
        } else {
            request.bearer_auth(&config.api_key)
        };
    }
    if config.provider_type.uses_anthropic_api() {
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

    let entries = if payload.data.is_empty() {
        payload.models
    } else {
        payload.data
    };

    entries
        .into_iter()
        .map(ModelDescriptor::from_value)
        .collect()
}

pub(super) fn provider_base_url(config: &RuntimeProviderConfig) -> String {
    config
        .base_url
        .as_deref()
        .and_then(|base_url| normalize_provider_base_url(config.provider_type, base_url))
        .unwrap_or_else(|| default_base_url_for_provider(config.provider_type).to_string())
}

pub(super) fn default_base_url_for_provider(provider_type: ProviderType) -> &'static str {
    provider_type.default_base_url()
}

fn model_list_endpoint(config: &RuntimeProviderConfig) -> Option<String> {
    if config.provider_type.uses_anthropic_api() {
        None
    } else {
        Some(format!("{}/models", provider_base_url(config)))
    }
}

#[derive(Debug, Deserialize)]
struct ModelListResponse {
    #[serde(default)]
    data: Vec<Value>,
    #[serde(default)]
    models: Vec<Value>,
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
            .or_else(|| object.get("name"))
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
