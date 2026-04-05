use anyhow::Result;

use crate::provider::{OpenAiCompatibleClient, OpenAiCompatibleConfig};
use crate::settings::{ProviderType, SettingsStorage};
use crate::storage::TaskRecord;

use super::{
    UiProbeProviderModelsRequest, UiProviderModelGroupView, UiProviderModelsView,
    UiTaskModelSelectorView, UiTestProviderConnectionRequest, UiTestProviderConnectionResult,
};
use super::util::normalize_ui_optional_string;

pub(super) fn provider_config_for_task(task: &TaskRecord) -> Result<OpenAiCompatibleConfig> {
    let settings = SettingsStorage::open()?;

    if let Some(provider_id) = task.selected_provider_id {
        if let Ok(provider) = settings.load_provider(provider_id) {
            let model = task
                .selected_model
                .clone()
                .or(settings.default_model()?)
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("missing task model in settings"))?;

            return Ok(OpenAiCompatibleConfig {
                provider_type: provider.provider_type,
                base_url: provider.base_url,
                api_key: provider.api_key,
                model,
            });
        }
    }

    if let Some(provider) = settings.default_provider()? {
        let model = task
            .selected_model
            .clone()
            .or(settings.default_model()?)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing default model in settings"))?;

        return Ok(OpenAiCompatibleConfig {
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            api_key: provider.api_key,
            model,
        });
    }

    let mut config = OpenAiCompatibleConfig::from_env()?;
    if let Some(model) = &task.selected_model {
        config.model = model.clone();
    }
    Ok(config)
}

pub async fn fetch_provider_models_for_task(
    task: Option<&TaskRecord>,
) -> Result<UiProviderModelsView> {
    let config = resolve_active_provider_config(task)?;
    let current_model = config.model.clone();
    let suggested_models = suggested_models_for_provider_type(config.provider_type);
    let provider_cache_key = provider_cache_key(&config.provider_type, config.base_url.as_deref());
    let client = OpenAiCompatibleClient::new(config);
    let mut available_models = client.list_models().await.unwrap_or_default();
    if !available_models.iter().any(|model| model == &current_model) {
        available_models.insert(0, current_model.clone());
    }
    available_models.sort();
    available_models.dedup();

    Ok(UiProviderModelsView {
        current_model,
        available_models,
        suggested_models,
        provider_cache_key,
    })
}

pub async fn fetch_task_model_selector(
    task: Option<&TaskRecord>,
) -> Result<UiTaskModelSelectorView> {
    let settings = SettingsStorage::open()?;
    let snapshot = settings.snapshot()?;
    let default_model = settings.default_model()?.unwrap_or_default();
    let current_provider_id = task
        .and_then(|value| value.selected_provider_id)
        .or(snapshot.default_provider_id);
    let current_model = task
        .and_then(|value| value.selected_model.clone())
        .filter(|value| !value.trim().is_empty())
        .or_else(|| (!default_model.trim().is_empty()).then(|| default_model.clone()))
        .or_else(|| {
            resolve_active_provider_config(task)
                .ok()
                .map(|config| config.model)
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_default();

    let mut providers = Vec::new();
    for provider in snapshot.providers {
        let suggested_models = suggested_models_for_provider_type(provider.provider_type);
        let provider_current_model = if Some(provider.id) == current_provider_id {
            current_model.clone()
        } else if !default_model.trim().is_empty() {
            default_model.clone()
        } else if let Some(first_suggested) = suggested_models.first() {
            first_suggested.clone()
        } else {
            default_probe_model(provider.provider_type).to_string()
        };
        let provider_cache_key =
            provider_cache_key(&provider.provider_type, provider.base_url.as_deref());
        let client = OpenAiCompatibleClient::new(OpenAiCompatibleConfig {
            provider_type: provider.provider_type,
            base_url: provider.base_url.clone(),
            api_key: provider.api_key.clone(),
            model: provider_current_model.clone(),
        });
        let mut available_models = client.list_models().await.unwrap_or_default();
        if !provider_current_model.is_empty()
            && !available_models.iter().any(|model| model == &provider_current_model)
        {
            available_models.insert(0, provider_current_model);
        }
        available_models.sort();
        available_models.dedup();

        providers.push(UiProviderModelGroupView {
            provider_id: Some(provider.id),
            provider_name: provider.name,
            provider_type: provider.provider_type.as_db_value().to_string(),
            provider_cache_key,
            available_models,
            suggested_models,
        });
    }

    if providers.is_empty() {
        let fallback = fetch_provider_models_for_task(task).await?;
        providers.push(UiProviderModelGroupView {
            provider_id: None,
            provider_name: "当前环境".to_string(),
            provider_type: "env".to_string(),
            provider_cache_key: fallback.provider_cache_key,
            available_models: fallback.available_models,
            suggested_models: fallback.suggested_models,
        });
    }

    Ok(UiTaskModelSelectorView {
        current_provider_id,
        current_model,
        providers,
    })
}

pub async fn fetch_provider_models_for_provider(provider_id: i64) -> Result<UiProviderModelsView> {
    let settings = SettingsStorage::open()?;
    let provider = settings.load_provider(provider_id)?;
    let current_model = settings.default_model()?.unwrap_or_default();
    let suggested_models = suggested_models_for_provider_type(provider.provider_type);
    let provider_cache_key =
        provider_cache_key(&provider.provider_type, provider.base_url.as_deref());
    let client = OpenAiCompatibleClient::new(OpenAiCompatibleConfig {
        provider_type: provider.provider_type,
        base_url: provider.base_url,
        api_key: provider.api_key,
        model: current_model.clone(),
    });
    let mut available_models = client.list_models().await.unwrap_or_default();
    if !current_model.is_empty() && !available_models.iter().any(|model| model == &current_model) {
        available_models.insert(0, current_model.clone());
    }
    available_models.sort();
    available_models.dedup();

    Ok(UiProviderModelsView {
        current_model,
        available_models,
        suggested_models,
        provider_cache_key,
    })
}

pub async fn fetch_probe_models(
    request: UiProbeProviderModelsRequest,
) -> Result<UiProviderModelsView> {
    let provider_type = ProviderType::from_db_value(&request.provider_type)
        .ok_or_else(|| anyhow::anyhow!("unsupported provider type {}", request.provider_type))?;
    let settings = SettingsStorage::open()?;
    let persisted_api_key = match request.id {
        Some(id) => settings.load_provider(id)?.api_key,
        None => String::new(),
    };
    let api_key = if request.api_key.trim().is_empty() {
        persisted_api_key
    } else {
        request.api_key.trim().to_string()
    };
    let suggested_models = suggested_models_for_provider_type(provider_type);
    let current_model = request
        .probe_model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| suggested_models.first().cloned())
        .or(settings.default_model()?)
        .unwrap_or_else(|| default_probe_model(provider_type).to_string());
    let base_url = normalize_ui_optional_string(request.base_url);
    let provider_cache_key = provider_cache_key(&provider_type, base_url.as_deref());
    let client = OpenAiCompatibleClient::new(OpenAiCompatibleConfig {
        provider_type,
        base_url,
        api_key,
        model: current_model.clone(),
    });
    let mut available_models = client.list_models().await.unwrap_or_default();
    if !current_model.is_empty() && !available_models.iter().any(|model| model == &current_model) {
        available_models.insert(0, current_model.clone());
    }
    available_models.sort();
    available_models.dedup();

    Ok(UiProviderModelsView {
        current_model,
        available_models,
        suggested_models,
        provider_cache_key,
    })
}

pub async fn test_provider_connection(
    request: UiTestProviderConnectionRequest,
) -> Result<UiTestProviderConnectionResult> {
    let provider_type = ProviderType::from_db_value(&request.provider_type)
        .ok_or_else(|| anyhow::anyhow!("unsupported provider type {}", request.provider_type))?;
    let settings = SettingsStorage::open()?;
    let suggested_model = suggested_models_for_provider_type(provider_type)
        .into_iter()
        .next();
    let persisted_api_key = match request.id {
        Some(id) => settings.load_provider(id)?.api_key,
        None => String::new(),
    };
    let api_key = if request.api_key.trim().is_empty() {
        persisted_api_key
    } else {
        request.api_key.trim().to_string()
    };
    let model = request
        .probe_model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| suggested_model.clone())
        .or(settings.default_model()?)
        .unwrap_or_else(|| default_probe_model(provider_type).to_string());
    let provider = OpenAiCompatibleClient::new(OpenAiCompatibleConfig {
        provider_type,
        base_url: normalize_ui_optional_string(request.base_url),
        api_key,
        model: model.clone(),
    });
    let message = provider.test_connection().await?;
    Ok(UiTestProviderConnectionResult {
        success: true,
        message,
        suggested_model: Some(model),
    })
}

fn resolve_active_provider_config(task: Option<&TaskRecord>) -> Result<OpenAiCompatibleConfig> {
    let settings = SettingsStorage::open()?;
    if let Some(task) = task {
        if let Some(provider_id) = task.selected_provider_id {
            if let Ok(provider) = settings.load_provider(provider_id) {
                let model = task
                    .selected_model
                    .clone()
                    .or(settings.default_model()?)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow::anyhow!("missing task model in settings"))?;
                return Ok(OpenAiCompatibleConfig {
                    provider_type: provider.provider_type,
                    base_url: provider.base_url,
                    api_key: provider.api_key,
                    model,
                });
            }
        }
    }

    if let Some(provider) = settings.default_provider()? {
        let model = task
            .and_then(|value| value.selected_model.clone())
            .or(settings.default_model()?)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("missing default model in settings"))?;
        return Ok(OpenAiCompatibleConfig {
            provider_type: provider.provider_type,
            base_url: provider.base_url,
            api_key: provider.api_key,
            model,
        });
    }

    let mut config = OpenAiCompatibleConfig::from_env()?;
    if let Some(model) = task
        .and_then(|value| value.selected_model.clone())
        .filter(|value| !value.trim().is_empty())
    {
        config.model = model;
    }
    Ok(config)
}

fn provider_cache_key(provider_type: &ProviderType, base_url: Option<&str>) -> String {
    match base_url {
        Some(base_url) if !base_url.trim().is_empty() => {
            format!("{}::{}", provider_type.as_db_value(), base_url.trim())
        }
        _ => provider_type.as_db_value().to_string(),
    }
}

fn suggested_models_for_provider_type(provider_type: ProviderType) -> Vec<String> {
    match provider_type {
        ProviderType::OpenAiCompat => vec![],
        ProviderType::OpenAi => vec![
            "gpt-5.4".to_string(),
            "gpt-5".to_string(),
            "gpt-5-mini".to_string(),
        ],
        ProviderType::Anthropic => vec![
            "claude-sonnet-4-5".to_string(),
            "claude-3-7-sonnet-latest".to_string(),
            "claude-3-5-haiku-latest".to_string(),
        ],
        ProviderType::Gemini => vec![
            "gemini-2.5-pro".to_string(),
            "gemini-2.5-flash".to_string(),
            "gemini-2.0-flash".to_string(),
        ],
        ProviderType::Fireworks => vec![
            "accounts/fireworks/models/deepseek-v3".to_string(),
            "accounts/fireworks/models/llama-v3p1-70b-instruct".to_string(),
        ],
        ProviderType::Together => vec![
            "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo".to_string(),
            "deepseek-ai/DeepSeek-V3".to_string(),
        ],
        ProviderType::Groq => vec![
            "llama-3.3-70b-versatile".to_string(),
            "deepseek-r1-distill-llama-70b".to_string(),
        ],
        ProviderType::Mimo => vec!["moonshot-v1-8k".to_string(), "moonshot-v1-32k".to_string()],
        ProviderType::Nebius => vec![
            "nebius::Qwen/Qwen2.5-Coder-32B-Instruct".to_string(),
            "nebius::meta-llama/Meta-Llama-3.1-70B-Instruct".to_string(),
        ],
        ProviderType::Xai => vec!["grok-3-mini".to_string(), "grok-3".to_string()],
        ProviderType::DeepSeek => {
            vec!["deepseek-chat".to_string(), "deepseek-reasoner".to_string()]
        }
        ProviderType::Zai => vec!["glm-4.6".to_string(), "coding::glm-4.6".to_string()],
        ProviderType::BigModel => vec!["glm-4-plus".to_string(), "glm-4-air".to_string()],
        ProviderType::Cohere => vec!["command-r-plus".to_string(), "command-r".to_string()],
        ProviderType::Ollama => vec![
            "qwen2.5-coder:32b".to_string(),
            "llama3.3:70b".to_string(),
            "deepseek-r1:32b".to_string(),
        ],
    }
}

fn default_probe_model(provider_type: ProviderType) -> &'static str {
    match provider_type {
        ProviderType::OpenAiCompat => "gpt-4o-mini",
        ProviderType::OpenAi => "gpt-5-mini",
        ProviderType::Anthropic => "claude-3-5-haiku-latest",
        ProviderType::Gemini => "gemini-2.0-flash",
        ProviderType::Fireworks => "accounts/fireworks/models/llama-v3p1-70b-instruct",
        ProviderType::Together => "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
        ProviderType::Groq => "llama-3.3-70b-versatile",
        ProviderType::Mimo => "moonshot-v1-8k",
        ProviderType::Nebius => "nebius::Qwen/Qwen2.5-Coder-32B-Instruct",
        ProviderType::Xai => "grok-3-mini",
        ProviderType::DeepSeek => "deepseek-chat",
        ProviderType::Zai => "glm-4.6",
        ProviderType::BigModel => "glm-4-air",
        ProviderType::Cohere => "command-r",
        ProviderType::Ollama => "qwen2.5-coder:32b",
    }
}
