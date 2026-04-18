use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};

use crate::context::{AgentContext, ChatTurn, ContentBlock, render_file_snapshot_for_prompt};
use crate::settings::{ServerToolCapability, ServerToolConfig, ServerToolFormat};
use crate::tools::{ToolDefinition, ToolParameter};

/// RequestMessage 保持 March 自己的结构，避免上下文层先翻译成第三方 SDK 类型，
/// 再被二次翻译成 provider wire format。
#[derive(Debug, Clone, Serialize)]
pub struct RequestMessage {
    pub role: String,
    pub content: Option<MessageContent>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Vec<ApiToolCallRequest>,
}

impl RequestMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(MessageContent::from_text(content)),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn user(content: impl Into<MessageContent>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant_text(content: impl Into<MessageContent>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant_tool_calls_with_text(
        content: Option<MessageContent>,
        tool_calls: Vec<ApiToolCallRequest>,
    ) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            tool_call_id: None,
            tool_calls,
        }
    }

    pub fn assistant_tool_calls(tool_calls: Vec<ApiToolCallRequest>) -> Self {
        Self::assistant_tool_calls_with_text(None, tool_calls)
    }

    pub fn tool(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: "tool".to_string(),
            content: Some(MessageContent::from_text(content)),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MessageContent {
    parts: Vec<MessageContentPart>,
}

impl MessageContent {
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            parts: vec![MessageContentPart::Text(text.into())],
        }
    }

    pub fn from_parts(parts: Vec<MessageContentPart>) -> Self {
        Self { parts }
    }

    pub fn parts(&self) -> &[MessageContentPart] {
        &self.parts
    }

    pub fn into_parts(self) -> Vec<MessageContentPart> {
        self.parts
    }

    pub fn joined_texts(&self) -> Option<String> {
        let text = self
            .parts
            .iter()
            .filter_map(|part| match part {
                MessageContentPart::Text(text) => Some(text.as_str()),
                MessageContentPart::Image { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("");
        (!text.trim().is_empty()).then_some(text)
    }
}

impl From<String> for MessageContent {
    fn from(value: String) -> Self {
        Self::from_text(value)
    }
}

impl From<&str> for MessageContent {
    fn from(value: &str) -> Self {
        Self::from_text(value)
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum MessageContentPart {
    Text(String),
    Image {
        media_type: String,
        data_base64: String,
        name: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiToolCallRequest {
    pub id: String,
    pub tool_type: String,
    pub function: ApiToolFunctionCallRequest,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApiToolFunctionCallRequest {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone)]
pub struct FunctionToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

#[derive(Debug, Clone)]
pub struct RequestOptions {
    pub model: String,
    pub stream: bool,
    pub temperature: f32,
    pub top_p: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
    pub max_output_tokens: Option<u32>,
}

impl RequestOptions {
    pub fn for_chat(
        model: impl Into<String>,
        stream: bool,
        temperature: Option<f32>,
        top_p: Option<f32>,
        presence_penalty: Option<f32>,
        frequency_penalty: Option<f32>,
        max_output_tokens: Option<u32>,
    ) -> Self {
        Self {
            model: model.into(),
            stream,
            temperature: temperature.unwrap_or(0.2),
            top_p,
            presence_penalty,
            frequency_penalty,
            max_output_tokens,
        }
    }
}

pub fn build_messages(context: &AgentContext) -> Vec<RequestMessage> {
    let mut messages = vec![RequestMessage::system(context.system_core.clone())];

    if !context.injections.is_empty() {
        messages.push(RequestMessage::system(render_injections(context)));
    }

    messages.push(RequestMessage::user(MessageContent::from_text(
        render_context_body(context),
    )));
    for turn in &context.recent_chat {
        messages.push(request_message_from_chat_turn(turn));
    }
    messages
}

pub fn function_tools_for_context(context: &AgentContext) -> Vec<FunctionToolDefinition> {
    context
        .tools
        .iter()
        .map(translate_tool_definition)
        .collect()
}

pub fn server_tool_definition(tool: &ServerToolConfig) -> Value {
    match (tool.capability, tool.format) {
        (ServerToolCapability::WebSearch, ServerToolFormat::OpenAiResponses) => {
            // OpenAI Responses API expects the built-in web tool as "web_search".
            json!({ "type": "web_search" })
        }
        (ServerToolCapability::CodeExecution, ServerToolFormat::OpenAiResponses) => {
            json!({ "type": "code_interpreter" })
        }
        (ServerToolCapability::FileSearch, ServerToolFormat::OpenAiResponses) => {
            json!({ "type": "file_search" })
        }
        (ServerToolCapability::WebSearch, ServerToolFormat::OpenAiChatCompletions) => {
            json!({ "type": "web_search_preview" })
        }
        (ServerToolCapability::CodeExecution, ServerToolFormat::OpenAiChatCompletions) => {
            json!({ "type": "code_interpreter" })
        }
        (ServerToolCapability::FileSearch, ServerToolFormat::OpenAiChatCompletions) => {
            json!({ "type": "file_search" })
        }
        (ServerToolCapability::WebSearch, ServerToolFormat::Anthropic) => {
            json!({ "type": "web_search_20250305" })
        }
        (ServerToolCapability::CodeExecution, ServerToolFormat::Anthropic) => {
            json!({ "type": "code_execution_20250522" })
        }
        (ServerToolCapability::WebSearch, ServerToolFormat::Gemini) => {
            json!({ "googleSearch": {} })
        }
        (ServerToolCapability::CodeExecution, ServerToolFormat::Gemini) => {
            json!({ "codeExecution": {} })
        }
        // UI 已经避免非法组合；这里保留保守 fallback，避免旧数据直接炸掉整个请求。
        (ServerToolCapability::FileSearch, ServerToolFormat::Anthropic)
        | (ServerToolCapability::FileSearch, ServerToolFormat::Gemini) => {
            json!({ "type": "file_search" })
        }
    }
}

pub fn serialize_tool_arguments(arguments_json: &str) -> Value {
    serde_json::from_str(arguments_json)
        .unwrap_or_else(|_| Value::String(arguments_json.to_string()))
}

pub fn render_tool_description(tool: &ToolDefinition) -> String {
    if tool.notes.is_empty() {
        return tool.description.to_string();
    }

    format!(
        "{}\n\nUsage notes:\n- {}",
        tool.description,
        tool.notes.join("\n- ")
    )
}

fn render_injections(context: &AgentContext) -> String {
    let mut output = String::from("[injections]\n");

    for injection in &context.injections {
        output.push_str(&format!("## {}\n{}\n", injection.id, injection.content));
    }

    output
}

fn render_context_body(context: &AgentContext) -> String {
    let mut output = String::new();
    output.push_str("[session_status]\n");
    if context.session_status.is_empty() {
        output.push_str("(none)\n");
    } else {
        output.push_str(&format!(
            "workspace_root: {}\nplatform: {}\ndefault_shell: {}\n",
            context.session_status.workspace_root.display(),
            context.session_status.platform,
            context.session_status.shell
        ));

        if context.session_status.available_shells.is_empty() {
            output.push_str("available_shells: (none)\n");
        } else {
            output.push_str(&format!(
                "available_shells: {}\n",
                context.session_status.available_shells.join(", ")
            ));
        }

        if context.session_status.workspace_entries.is_empty() {
            output.push_str("workspace_entries: (none)\n");
        } else {
            output.push_str("workspace_entries:\n");
            for entry in &context.session_status.workspace_entries {
                output.push_str(&format!("- {entry}\n"));
            }
        }
    }

    output.push_str("\n[open_files]\n");
    if context.open_files.is_empty() {
        output.push_str("(none)\n");
    } else {
        output.push_str(
            "These watcher-backed snapshots are the authoritative current file contents for this turn. Reuse them instead of re-reading the same files via shell commands unless you need a different external view.\n\n",
        );
        for snapshot in context.open_files_in_prompt_order() {
            output.push_str(&render_file_snapshot_for_prompt(snapshot));
            output.push('\n');
        }
    }

    output.push_str("[notes]\n");
    if context.notes.is_empty() {
        output.push_str("(none)\n");
    } else {
        for (id, note) in &context.notes {
            output.push_str(&format!("{id}: {}\n", note.content));
        }
    }

    output.push_str("\n[memory_index]\n");
    if let Some(memory_index) = &context.memory_index {
        if memory_index.is_empty() {
            output.push_str("(none)\n");
        } else {
            output.push_str(&memory_index.render_for_prompt());
            output.push('\n');
        }
    } else {
        output.push_str("(none)\n");
    }

    output.push_str("\n[runtime_status]\n");
    if context.runtime_status.is_empty() {
        output.push_str("(none)\n");
    } else {
        if context.runtime_status.locked_files.is_empty() {
            output.push_str("locked_files: (none)\n");
        } else {
            output.push_str("locked_files:\n");
            for path in &context.runtime_status.locked_files {
                output.push_str(&format!("- {}\n", path.display()));
            }
        }

        if let Some(pressure) = &context.runtime_status.context_pressure {
            output.push_str(&format!(
                "context_pressure: {}% - {}\n",
                pressure.used_percent, pressure.message
            ));
        }
    }

    output.push_str("\n[hints]\n");
    if context.hints.is_empty() {
        output.push_str("(none)\n");
    } else {
        for hint in &context.hints {
            output.push_str(&format!("- {}\n", hint.content));
        }
    }

    output
}

fn request_message_from_chat_turn(turn: &ChatTurn) -> RequestMessage {
    let content = message_content_from_blocks(&turn.content);
    match turn.role {
        crate::context::Role::User => RequestMessage::user(content),
        crate::context::Role::Assistant => RequestMessage::assistant_text(content),
        crate::context::Role::System | crate::context::Role::Tool => unreachable!(),
    }
}

fn message_content_from_blocks(blocks: &[ContentBlock]) -> MessageContent {
    MessageContent::from_parts(
        blocks
            .iter()
            .map(|block| match block {
                ContentBlock::Text { text } => MessageContentPart::Text(text.clone()),
                ContentBlock::Image {
                    media_type,
                    data_base64,
                    name,
                    ..
                } => MessageContentPart::Image {
                    media_type: media_type.clone(),
                    data_base64: data_base64.clone(),
                    name: name.clone(),
                },
            })
            .collect::<Vec<_>>(),
    )
}

fn translate_tool_definition(tool: &ToolDefinition) -> FunctionToolDefinition {
    FunctionToolDefinition {
        name: tool.name.to_string(),
        description: render_tool_description(tool),
        parameters: build_parameters_schema(&tool.parameters),
    }
}

fn build_parameters_schema(parameters: &[ToolParameter]) -> Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for parameter in parameters {
        properties.insert(parameter.name.to_string(), parameter_schema(parameter));

        if parameter.required {
            required.push(parameter.name.to_string());
        }
    }

    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn parameter_schema(parameter: &ToolParameter) -> Value {
    match parameter.kind {
        "string_array" => json!({
            "type": "array",
            "items": { "type": "string" },
            "description": parameter.description,
        }),
        _ => json!({
            "type": json_type_for_parameter(parameter),
            "description": parameter.description,
        }),
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

pub fn validate_messages(messages: &[RequestMessage]) -> Result<()> {
    for message in messages {
        if message.role == "tool" && message.tool_call_id.is_none() {
            anyhow::bail!("tool message missing tool_call_id");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::build_parameters_schema;
    use crate::tools::ToolParameter;

    #[test]
    fn string_array_parameters_render_as_json_array_schema() {
        let schema = build_parameters_schema(&[ToolParameter {
            name: "tags",
            kind: "string_array",
            required: true,
            description: "Keyword list.",
        }]);

        assert_eq!(schema["properties"]["tags"]["type"], json!("array"));
        assert_eq!(
            schema["properties"]["tags"]["items"]["type"],
            json!("string")
        );
    }
}
