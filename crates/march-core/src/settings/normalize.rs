use reqwest::Url;

use super::ProviderType;

pub(crate) fn normalize_provider_base_url(
    provider_type: ProviderType,
    raw: impl AsRef<str>,
) -> Option<String> {
    let trimmed = raw.as_ref().trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        return None;
    }

    let Ok(default_url) = Url::parse(provider_type.default_base_url()) else {
        return Some(trimmed);
    };
    let default_path = default_url.path().trim_end_matches('/');
    if default_path.is_empty() {
        return Some(trimmed);
    }

    let Ok(mut parsed) = Url::parse(&trimmed) else {
        return Some(trimmed);
    };
    let input_path = parsed.path().trim_end_matches('/');
    if input_path.is_empty() {
        parsed.set_path(default_path);
        return Some(parsed.to_string().trim_end_matches('/').to_string());
    }

    Some(trimmed)
}

pub(crate) fn normalize_optional_string(raw: String) -> Option<String> {
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(crate) fn normalize_agent_name(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace(' ', "-")
}

pub(crate) fn normalize_avatar_color(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "#64748B".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn normalize_agent_description(
    raw: String,
    display_name: &str,
    system_prompt: &str,
) -> String {
    let trimmed = raw.trim().to_string();
    if !trimmed.is_empty() {
        return trimmed;
    }

    system_prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(60).collect::<String>())
        .filter(|line| !line.trim().is_empty())
        .unwrap_or_else(|| format!("负责 {} 相关工作。", display_name))
}
