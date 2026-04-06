use std::time::{SystemTime, UNIX_EPOCH};

use crate::agent::DEFAULT_CONTEXT_WINDOW_TOKENS;
use crate::model_capabilities::get_model_capabilities;

pub(super) fn normalize_ui_optional_string(raw: String) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

pub(super) fn mask_api_key(api_key: &str) -> String {
    let trimmed = api_key.trim();
    if trimmed.is_empty() {
        return "未设置".to_string();
    }

    let chars = trimmed.chars().collect::<Vec<_>>();
    let head = chars.iter().take(4).collect::<String>();
    let tail = chars
        .iter()
        .rev()
        .take(4)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{}••••{}", head, tail)
}

pub(super) fn pretty_json_or_original(text: &str) -> String {
    serde_json::from_str::<serde_json::Value>(text)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| text.to_string())
}

pub(super) fn resolve_context_window_fallback(model_id: Option<&str>) -> usize {
    if let Some(override_tokens) = std::env::var("MA_CONTEXT_WINDOW_TOKENS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
    {
        return override_tokens;
    }

    model_id
        .and_then(|model| {
            get_model_capabilities(model)
                .map(|capabilities| capabilities.context_window)
                .or_else(|| guess_context_window_from_model_name(model))
        })
        .unwrap_or(DEFAULT_CONTEXT_WINDOW_TOKENS)
}

fn guess_context_window_from_model_name(model_id: &str) -> Option<usize> {
    let normalized = model_id.trim().to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    for suffix in ['k', 'm'] {
        if let Some(index) = normalized.find(suffix) {
            let digits = normalized[..index]
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

pub(super) fn system_time_to_unix(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}
