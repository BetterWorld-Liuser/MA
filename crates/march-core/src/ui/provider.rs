use anyhow::Result;

use crate::agent::{AgentSession, DEFAULT_CONTEXT_WINDOW_TOKENS};
use crate::model_capabilities::get_model_capabilities;
use crate::provider::{OpenAiCompatibleClient, OpenAiCompatibleConfig};
use crate::settings::{ProviderModelRecord, ProviderType, SettingsStorage};
use crate::storage::TaskRecord;

use super::util::normalize_ui_optional_string;
use super::{
    UiModelCapabilitiesView, UiProbeProviderModelsRequest, UiProviderModelGroupView,
    UiProviderModelsView, UiTaskModelSelectorView, UiTestProviderConnectionRequest,
    UiTestProviderConnectionResult,
};

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
                model: model.clone(),
                server_tools: resolve_server_tools(&settings, provider.id, &model)?,
                temperature: task.model_temperature,
                top_p: task.model_top_p,
                presence_penalty: task.model_presence_penalty,
                frequency_penalty: task.model_frequency_penalty,
                max_output_tokens: task.model_max_output_tokens,
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
            model: model.clone(),
            server_tools: resolve_server_tools(&settings, provider.id, &model)?,
            temperature: task.model_temperature,
            top_p: task.model_top_p,
            presence_penalty: task.model_presence_penalty,
            frequency_penalty: task.model_frequency_penalty,
            max_output_tokens: task.model_max_output_tokens,
        });
    }

    let mut config = OpenAiCompatibleConfig::from_env()?;
    if let Some(model) = &task.selected_model {
        config.model = model.clone();
    }
    Ok(config)
}

pub(super) fn provider_config_for_session(
    task: &TaskRecord,
    session: &AgentSession,
) -> Result<OpenAiCompatibleConfig> {
    if let Some(profile) = session.active_agent_profile() {
        let settings = SettingsStorage::open()?;
        if let Some(provider_id) = profile.provider_id {
            if let Ok(provider) = settings.load_provider(provider_id) {
                let model = profile
                    .model_id
                    .clone()
                    .or_else(|| task.selected_model.clone())
                    .or(settings.default_model()?)
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| anyhow::anyhow!("missing agent model in settings"))?;
                return Ok(OpenAiCompatibleConfig {
                    provider_type: provider.provider_type,
                    base_url: provider.base_url,
                    api_key: provider.api_key,
                    model: model.clone(),
                    server_tools: resolve_server_tools(&settings, provider.id, &model)?,
                    temperature: task.model_temperature,
                    top_p: task.model_top_p,
                    presence_penalty: task.model_presence_penalty,
                    frequency_penalty: task.model_frequency_penalty,
                    max_output_tokens: task.model_max_output_tokens,
                });
            }
        }
    }

    provider_config_for_task(task)
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
            server_tools: resolve_server_tools(&settings, provider.id, &provider_current_model)?,
            temperature: None,
            top_p: None,
            presence_penalty: None,
            frequency_penalty: None,
            max_output_tokens: None,
        });
        let mut available_models = client.list_models().await.unwrap_or_default();
        let persisted_models = settings
            .list_provider_models_for_provider(provider.id)?
            .into_iter()
            .map(|model| model.model_id)
            .collect::<Vec<_>>();
        available_models.extend(persisted_models);
        if !provider_current_model.is_empty()
            && !available_models
                .iter()
                .any(|model| model == &provider_current_model)
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
        current_temperature: task.and_then(|value| value.model_temperature),
        current_top_p: task.and_then(|value| value.model_top_p),
        current_presence_penalty: task.and_then(|value| value.model_presence_penalty),
        current_frequency_penalty: task.and_then(|value| value.model_frequency_penalty),
        current_max_output_tokens: task.and_then(|value| value.model_max_output_tokens),
        current_model_capabilities: resolve_current_model_capabilities(task)?,
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
        server_tools: resolve_server_tools(&settings, provider_id, &current_model)?,
        temperature: None,
        top_p: None,
        presence_penalty: None,
        frequency_penalty: None,
        max_output_tokens: None,
    });
    let mut available_models = client.list_models().await.unwrap_or_default();
    available_models.extend(
        settings
            .list_provider_models_for_provider(provider_id)?
            .into_iter()
            .map(|model| model.model_id),
    );
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
        server_tools: Vec::new(),
        temperature: None,
        top_p: None,
        presence_penalty: None,
        frequency_penalty: None,
        max_output_tokens: None,
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
        server_tools: Vec::new(),
        temperature: None,
        top_p: None,
        presence_penalty: None,
        frequency_penalty: None,
        max_output_tokens: None,
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
                    model: model.clone(),
                    server_tools: resolve_server_tools(&settings, provider.id, &model)?,
                    temperature: task.model_temperature,
                    top_p: task.model_top_p,
                    presence_penalty: task.model_presence_penalty,
                    frequency_penalty: task.model_frequency_penalty,
                    max_output_tokens: task.model_max_output_tokens,
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
            model: model.clone(),
            server_tools: resolve_server_tools(&settings, provider.id, &model)?,
            temperature: task.and_then(|value| value.model_temperature),
            top_p: task.and_then(|value| value.model_top_p),
            presence_penalty: task.and_then(|value| value.model_presence_penalty),
            frequency_penalty: task.and_then(|value| value.model_frequency_penalty),
            max_output_tokens: task.and_then(|value| value.model_max_output_tokens),
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

fn resolve_server_tools(
    settings: &SettingsStorage,
    provider_id: i64,
    model: &str,
) -> Result<Vec<crate::settings::ServerToolConfig>> {
    Ok(settings
        .load_provider_model_by_model_id(provider_id, model)?
        .map(|record| record.server_tools)
        .unwrap_or_default())
}

fn provider_cache_key(provider_type: &ProviderType, base_url: Option<&str>) -> String {
    match base_url {
        Some(base_url) if !base_url.trim().is_empty() => {
            format!("{}::{}", provider_type.as_db_value(), base_url.trim())
        }
        _ => provider_type.as_db_value().to_string(),
    }
}

fn resolve_current_model_capabilities(
    task: Option<&TaskRecord>,
) -> Result<UiModelCapabilitiesView> {
    let config = resolve_active_provider_config(task)?;
    let settings = SettingsStorage::open()?;
    let selected_provider_id = task
        .and_then(|value| value.selected_provider_id)
        .or(settings.snapshot()?.default_provider_id);
    let persisted = selected_provider_id
        .map(|provider_id| settings.load_provider_model_by_model_id(provider_id, &config.model))
        .transpose()?
        .flatten();

    Ok(resolve_model_capabilities(
        config.provider_type,
        &config.model,
        persisted.as_ref(),
    ))
}

fn resolve_model_capabilities(
    provider_type: ProviderType,
    model_id: &str,
    persisted: Option<&ProviderModelRecord>,
) -> UiModelCapabilitiesView {
    if let Some(model) = persisted {
        return UiModelCapabilitiesView {
            context_window: model.context_window,
            max_output_tokens: model.max_output_tokens,
            supports_tool_use: model.supports_tool_use,
            supports_vision: model.supports_vision,
            supports_audio: model.supports_audio,
            supports_pdf: model.supports_pdf,
            server_tools: model
                .server_tools
                .iter()
                .map(|tool| super::UiServerToolView {
                    capability: tool.capability.as_db_value().to_string(),
                    format: tool.format.as_db_value().to_string(),
                })
                .collect(),
        };
    }

    let builtin = get_model_capabilities(model_id);
    let normalized = model_id.trim().to_ascii_lowercase();
    let context_window = builtin
        .as_ref()
        .map(|value| value.context_window)
        .or_else(|| guess_context_window_from_model_name(&normalized))
        .unwrap_or(DEFAULT_CONTEXT_WINDOW_TOKENS);
    let max_output_tokens = builtin
        .as_ref()
        .map(|value| value.max_output_tokens)
        .unwrap_or(4_096);

    let supports_vision = matches!(provider_type, ProviderType::Gemini)
        || normalized.contains("gpt-4o")
        || normalized.contains("claude-3")
        || normalized.contains("claude-sonnet-4")
        || normalized.contains("gemini")
        || normalized.contains("vision")
        || normalized.contains("vl");
    let supports_tool_use = !matches!(provider_type, ProviderType::Cohere);

    UiModelCapabilitiesView {
        context_window,
        max_output_tokens,
        supports_tool_use,
        supports_vision,
        supports_audio: false,
        supports_pdf: false,
        server_tools: Vec::new(),
    }
}

fn guess_context_window_from_model_name(model_id: &str) -> Option<usize> {
    for suffix in ['k', 'm'] {
        if let Some(index) = model_id.find(suffix) {
            let digits = model_id[..index]
                .chars()
                .rev()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            if digits.is_empty() {
                continue;
            }
            if let Ok(base) = digits.parse::<usize>() {
                return Some(match suffix {
                    'k' => base * 1_000,
                    'm' => base * 1_000_000,
                    _ => unreachable!(),
                });
            }
        }
    }

    None
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
