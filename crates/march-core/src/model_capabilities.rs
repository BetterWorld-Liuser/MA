use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ModelCapabilities {
    pub context_window: usize,
    pub max_output_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelCapabilitiesFile {
    model_capabilities: HashMap<String, ModelCapabilities>,
    #[serde(skip)]
    _note: String,
}

impl Default for ModelCapabilitiesFile {
    fn default() -> Self {
        Self {
            model_capabilities: HashMap::new(),
            _note: String::new(),
        }
    }
}

lazy_static::lazy_static! {
    static ref CAPABILITIES: ModelCapabilitiesFile = {
        let json_str = include_str!("model_capabilities.json");
        serde_json::from_str(json_str).unwrap_or_default()
    };
}

pub fn get_model_capabilities(model_id: &str) -> Option<ModelCapabilities> {
    CAPABILITIES.model_capabilities.get(model_id).cloned()
}

pub fn estimate_tokens_from_bytes(bytes: usize) -> usize {
    // 粗略估计：1 token ≈ 4 个字符（平均情况）
    // 对于中文等 CJK 文字，token 密度会更高
    // 这里使用保守估计，实际应用应使用真实的 tokenizer
    bytes / 3
}

pub fn get_context_window_for_model(model_id: &str) -> usize {
    get_model_capabilities(model_id)
        .map(|caps| caps.context_window)
        .unwrap_or(128_000) // 默认回退到 128k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_capabilities() {
        let gpt4o = get_model_capabilities("gpt-4o");
        assert!(gpt4o.is_some());
        assert_eq!(gpt4o.unwrap().context_window, 128000);
    }

    #[test]
    fn test_unknown_model() {
        let unknown = get_model_capabilities("unknown-model-xyz");
        assert!(unknown.is_none());
    }
}
