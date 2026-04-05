use anyhow::{Context, Result, bail};
use genai::chat::{
    ChatMessage, ChatRequest, ContentPart, MessageContent, Tool as GenAiTool,
    ToolCall as GenAiToolCall, ToolResponse as GenAiToolResponse,
};
use serde::Serialize;
use serde_json::Value;

use crate::context::{
    AgentContext, render_chat_turn_for_prompt, render_file_snapshot_for_prompt,
};
use crate::tools::{ToolDefinition, ToolParameter};

/// RequestMessage 保持显式结构，方便 tool loop 在同一轮里累积 assistant/tool 消息。
#[derive(Debug, Clone, Serialize)]
pub struct RequestMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    tool_calls: Vec<ApiToolCallRequest>,
}

impl RequestMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant_text(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: Some(content.into()),
            tool_call_id: None,
            tool_calls: Vec::new(),
        }
    }

    pub fn assistant_tool_calls_with_text(
        content: Option<String>,
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
            content: Some(content.into()),
            tool_call_id: Some(tool_call_id.into()),
            tool_calls: Vec::new(),
        }
    }
}

pub fn build_messages(context: &AgentContext) -> Vec<RequestMessage> {
    let mut messages = vec![RequestMessage::system(context.system_core.clone())];

    if !context.injections.is_empty() {
        messages.push(RequestMessage::system(render_injections(context)));
    }

    messages.push(RequestMessage::user(render_context_body(context)));
    messages
}

pub fn build_chat_request(
    context: &AgentContext,
    conversation: &[RequestMessage],
) -> Result<ChatRequest> {
    let mut request = ChatRequest::default();

    for message in conversation {
        let content = message.content.clone().unwrap_or_default();
        match message.role.as_str() {
            "system" => {
                if request.system.is_none() {
                    request = request.with_system(content);
                } else {
                    request = request.append_message(ChatMessage::system(content));
                }
            }
            "user" => request = request.append_message(ChatMessage::user(content)),
            "assistant" => {
                let assistant_message = build_assistant_message(&content, &message.tool_calls)?;
                request = request.append_message(assistant_message);
            }
            "tool" => {
                let tool_call_id = message
                    .tool_call_id
                    .clone()
                    .context("tool message missing tool_call_id")?;
                request = request.append_message(ChatMessage::from(GenAiToolResponse::new(
                    tool_call_id,
                    content,
                )));
            }
            other => bail!("unsupported request role {other}"),
        }
    }

    if !context.tools.is_empty() {
        request = request.with_tools(context.tools.iter().map(translate_tool_definition));
    }

    Ok(request)
}

#[derive(Debug, Serialize, Clone)]
pub struct ApiToolCallRequest {
    pub id: String,
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ApiToolFunctionCallRequest,
}

#[derive(Debug, Serialize, Clone)]
pub struct ApiToolFunctionCallRequest {
    pub name: String,
    pub arguments: String,
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
    output.push_str("# Session Status\n");
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

    output.push_str("\n# Open Files\n");
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

    output.push_str("# Notes\n");
    if context.notes.is_empty() {
        output.push_str("(none)\n");
    } else {
        for (id, note) in &context.notes {
            output.push_str(&format!("{id}: {}\n", note.content));
        }
    }

    output.push_str("\n# Runtime Status\n");
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

    output.push_str("\n# Hints\n");
    if context.hints.is_empty() {
        output.push_str("(none)\n");
    } else {
        for hint in &context.hints {
            output.push_str(&format!("- {}\n", hint.content));
        }
    }

    output.push_str("\n# Recent Chat\n");
    for turn in &context.recent_chat {
        output.push_str(&format!("{}\n", render_chat_turn_for_prompt(turn)));
    }

    output
}

fn build_assistant_message(
    content: &str,
    tool_calls: &[ApiToolCallRequest],
) -> Result<ChatMessage> {
    if tool_calls.is_empty() {
        return Ok(ChatMessage::assistant(content.to_string()));
    }

    let mut parts = Vec::new();
    if !content.trim().is_empty() {
        parts.push(ContentPart::Text(content.to_string()));
    }
    for tool_call in tool_calls {
        parts.push(ContentPart::ToolCall(GenAiToolCall {
            call_id: tool_call.id.clone(),
            fn_name: tool_call.function.name.clone(),
            fn_arguments: parse_tool_arguments(&tool_call.function.arguments),
            thought_signatures: None,
        }));
    }

    Ok(ChatMessage::assistant(MessageContent::from_parts(parts)))
}

fn parse_tool_arguments(arguments_json: &str) -> Value {
    serde_json::from_str(arguments_json)
        .unwrap_or_else(|_| Value::String(arguments_json.to_string()))
}

fn translate_tool_definition(tool: &ToolDefinition) -> GenAiTool {
    GenAiTool::new(tool.name.to_string())
        .with_description(render_tool_description(tool))
        .with_schema(build_parameters_schema(&tool.parameters))
}

fn render_tool_description(tool: &ToolDefinition) -> String {
    if tool.notes.is_empty() {
        return tool.description.to_string();
    }

    format!(
        "{}\n\nUsage notes:\n- {}",
        tool.description,
        tool.notes.join("\n- ")
    )
}

fn build_parameters_schema(parameters: &[ToolParameter]) -> Value {
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

    serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
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
