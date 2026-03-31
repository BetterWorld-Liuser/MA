use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::context::AgentContext;
use crate::tools::{ToolDefinition, ToolParameter, ToolRuntime};

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
        self.complete_text_with_tools(system, prompt, None).await
    }

    /// 目前的“工具注入”先以 prompt augmentation 形式存在。
    /// 后续接 provider 原生 tool calling 时，优先从这个入口继续演进。
    pub async fn complete_text_with_tools(
        &self,
        system: &str,
        prompt: &str,
        tool_runtime: Option<&ToolRuntime>,
    ) -> Result<String> {
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
            tools: tool_runtime.map(translate_tool_runtime),
            tool_choice: tool_runtime.map(|_| ToolChoice::None),
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

    /// 翻译层入口：把 Ma 自己构建好的 AgentContext 映射到 provider 请求。
    /// 当前先保持“system_core 独立 + context body 单消息 + tools 独立参数”的最小形态。
    pub async fn complete_context(&self, context: &AgentContext) -> Result<String> {
        let request = ChatCompletionRequest {
            model: self.config.model.clone(),
            temperature: Some(0.2),
            messages: build_messages(context),
            tools: if context.tools.is_empty() {
                None
            } else {
                Some(context.tools.iter().map(translate_tool_definition).collect())
            },
            // 目前还没有完整的 tool execution loop，因此先显式关闭 provider 原生 tool 调用。
            // 这样可以先把“tools 独立参数传递”的结构落地，而不会收到无法消费的 tool call 响应。
            tool_choice: if context.tools.is_empty() {
                None
            } else {
                Some(ToolChoice::None)
            },
        };

        self.send_chat_completion(request).await
    }

    async fn send_chat_completion(&self, request: ChatCompletionRequest) -> Result<String> {
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

fn build_messages(context: &AgentContext) -> Vec<ChatMessage> {
    let mut messages = vec![ChatMessage {
        role: "system".to_string(),
        content: context.system_core.clone(),
    }];

    if !context.injections.is_empty() {
        messages.push(ChatMessage {
            role: "system".to_string(),
            content: render_injections(context),
        });
    }

    messages.push(ChatMessage {
        role: "user".to_string(),
        content: render_context_body(context),
    });

    messages
}

fn render_injections(context: &AgentContext) -> String {
    let mut output = String::from("# Injections\n");

    for injection in &context.injections {
        output.push_str(&format!("## {}\n{}\n", injection.id, injection.content));
    }

    output
}

fn render_context_body(context: &AgentContext) -> String {
    let mut output = String::new();
    output.push_str("# Open Files\n");
    if context.open_files.is_empty() {
        output.push_str("(none)\n");
    } else {
        for snapshot in context.open_files_in_prompt_order() {
            output.push_str(&format!(
                "## {}\nmodified_by={:?} changed={}\n{}\n\n",
                snapshot.path.display(),
                snapshot.last_modified_by,
                snapshot.has_changed_since_watch,
                snapshot.content
            ));
        }
    }

    output.push_str("# Notes\n");
    if context.notes.is_empty() {
        output.push_str("(none)\n");
    } else {
        for (id, content) in &context.notes {
            output.push_str(&format!("{id}: {content}\n"));
        }
    }

    output.push_str("\n# Recent Chat\n");
    for turn in &context.recent_chat {
        output.push_str(&format!("{:?}: {}\n", turn.role, turn.content));
    }

    output
}

fn translate_tool_runtime(tool_runtime: &ToolRuntime) -> Vec<ApiToolDefinition> {
    tool_runtime
        .tools
        .iter()
        .map(translate_tool_definition)
        .collect()
}

fn translate_tool_definition(tool: &ToolDefinition) -> ApiToolDefinition {
    ApiToolDefinition {
        tool_type: "function".to_string(),
        function: ApiFunctionDefinition {
            name: tool.name.to_string(),
            description: Some(render_tool_description(tool)),
            parameters: build_parameters_schema(&tool.parameters),
        },
    }
}

fn render_tool_description(tool: &ToolDefinition) -> String {
    if tool.notes.is_empty() {
        return tool.description.to_string();
    }

    format!("{}\n\nUsage notes:\n- {}", tool.description, tool.notes.join("\n- "))
}

fn build_parameters_schema(parameters: &[ToolParameter]) -> ApiJsonSchema {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for parameter in parameters {
        properties.insert(
            parameter.name.to_string(),
            serde_json::json!({
                "type": json_type_for_parameter(parameter),
                "description": parameter.description,
            }),
        );

        if parameter.required {
            required.push(parameter.name.to_string());
        }
    }

    ApiJsonSchema {
        schema_type: "object".to_string(),
        properties: serde_json::Value::Object(properties),
        required,
        additional_properties: false,
    }
}

fn json_type_for_parameter(parameter: &ToolParameter) -> &'static str {
    match parameter.kind {
        "boolean" => "boolean",
        "integer" => "integer",
        "enum" => "string",
        "path" => "string",
        _ => "string",
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ApiToolDefinition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<ToolChoice>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ApiToolDefinition {
    #[serde(rename = "type")]
    tool_type: String,
    function: ApiFunctionDefinition,
}

#[derive(Debug, Serialize)]
struct ApiFunctionDefinition {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    parameters: ApiJsonSchema,
}

#[derive(Debug, Serialize)]
struct ApiJsonSchema {
    #[serde(rename = "type")]
    schema_type: String,
    properties: serde_json::Value,
    required: Vec<String>,
    additional_properties: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum ToolChoice {
    None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{
        AgentContext, ChatTurn, FileSnapshot, Injection, ModifiedBy, Role,
    };
    use indexmap::IndexMap;

    #[test]
    fn complete_text_with_tools_keeps_system_message_clean() {
        let runtime = ToolRuntime {
            tools: Vec::new(),
        };

        let request = ChatCompletionRequest {
            model: "test-model".to_string(),
            temperature: Some(0.2),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: "system prompt".to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: "prompt".to_string(),
                },
            ],
            tools: Some(translate_tool_runtime(&runtime)),
            tool_choice: Some(ToolChoice::None),
        };

        let payload = serde_json::to_value(request).expect("request json");

        assert_eq!(payload["messages"][0]["content"], "system prompt");
        assert!(payload.get("tools").is_some());
        assert_eq!(payload["tool_choice"], "none");
    }

    #[test]
    fn build_messages_preserves_injections_and_context_layers() {
        let mut open_files = IndexMap::new();
        open_files.insert(
            "src/main.rs".into(),
            FileSnapshot::new(
                "src/main.rs",
                "fn main() {}",
                std::time::SystemTime::UNIX_EPOCH,
                ModifiedBy::Unknown,
            ),
        );

        let mut notes = IndexMap::new();
        notes.insert("target".to_string(), "demo".to_string());

        let context = AgentContext {
            system_core: "system core".to_string(),
            injections: vec![Injection {
                id: "skill:test".to_string(),
                content: "injection body".to_string(),
            }],
            tools: Vec::new(),
            open_files,
            notes,
            recent_chat: vec![ChatTurn {
                role: Role::User,
                content: "hello".to_string(),
                timestamp: std::time::SystemTime::UNIX_EPOCH,
            }],
        };

        let messages = build_messages(&context);

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[0].content, "system core");
        assert!(messages[1].content.contains("skill:test"));
        assert!(messages[2].content.contains("# Open Files"));
        assert!(messages[2].content.contains("# Notes"));
        assert!(messages[2].content.contains("# Recent Chat"));
    }
}
