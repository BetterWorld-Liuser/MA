use serde_json::Value;

use super::ProviderResponse;
use super::delivery::{DebugStructuredProviderResponse, DebugStructuredToolCall, DeliveryPath};

pub(super) fn summarize_probe_reply(reply: &str) -> String {
    const MAX_REPLY_CHARS: usize = 48;

    let compact = reply.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = compact.chars();
    let truncated = chars.by_ref().take(MAX_REPLY_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{}…", truncated)
    } else {
        truncated
    }
}

pub fn fallback_task_title(first_user_message: &str) -> Option<String> {
    let trimmed = first_user_message.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut title = trimmed
        .replace('\n', " ")
        .replace('\r', " ")
        .replace('“', "")
        .replace('”', "")
        .replace('"', "")
        .replace('：', " ")
        .replace(':', " ");

    for prefix in [
        "帮我",
        "请你",
        "麻烦",
        "看下",
        "看一下",
        "继续",
        "这个",
        "这里",
        "我想",
        "我需要",
    ] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.trim().to_string();
        }
    }

    sanitize_task_title(&title)
}

pub(super) fn sanitize_task_title(raw: &str) -> Option<String> {
    let first_line = raw.lines().map(str::trim).find(|line| !line.is_empty())?;

    let mut title = first_line
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '“' | '”' | '`'))
        .trim_end_matches(|ch: char| {
            matches!(
                ch,
                '。' | '.' | '！' | '!' | '?' | '？' | '；' | ';' | '：' | ':'
            )
        })
        .trim()
        .to_string();

    for prefix in ["标题：", "标题:", "task:", "Task:", "任务名：", "任务名:"] {
        if let Some(rest) = title.strip_prefix(prefix) {
            title = rest.trim().to_string();
        }
    }

    title = title.split_whitespace().collect::<Vec<_>>().join(" ");

    if title.is_empty() {
        return None;
    }

    let char_count = title.chars().count();
    if char_count <= 2 {
        return None;
    }

    if char_count > 24 {
        title = title
            .chars()
            .take(24)
            .collect::<String>()
            .trim()
            .to_string();
    }

    if title.is_empty() { None } else { Some(title) }
}

pub fn format_provider_response_for_debug(raw_response: &str) -> String {
    if raw_response.trim().is_empty() {
        return "(empty response)".to_string();
    }

    serde_json::from_str::<serde_json::Value>(raw_response)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| raw_response.to_string())
}

pub(super) fn debug_structured_response(
    response: &ProviderResponse,
    delivery_path: DeliveryPath,
    captured_raw_body: Option<Value>,
) -> DebugStructuredProviderResponse {
    DebugStructuredProviderResponse {
        delivery_path: delivery_path.label().to_string(),
        stream_failure: delivery_path.stream_failure(),
        content: response.content.clone(),
        tool_calls: response
            .tool_calls
            .clone()
            .into_iter()
            .map(|tool_call| DebugStructuredToolCall {
                id: tool_call.id,
                name: tool_call.name,
                arguments_json: tool_call.arguments_json,
            })
            .collect(),
        captured_raw_body,
    }
}
