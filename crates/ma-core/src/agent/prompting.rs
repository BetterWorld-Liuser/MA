use std::path::{Path, PathBuf};

use anyhow::Result;
use genai::chat::MessageContent;

use crate::config::MarchConfig;
use crate::context::{
    AgentContext, Injection, render_chat_turn_for_prompt, render_file_snapshot_for_prompt,
};
use crate::paths::clean_path;
use crate::provider::{
    ApiToolCallRequest, ApiToolFunctionCallRequest, ProviderToolCall, RequestMessage,
};
use crate::settings::user_home_dir;
use crate::skills::{SkillEntry, SkillLoader};
use crate::storage::PersistedOpenFile;
use crate::tools::ToolRuntime;

use super::{AGENTS_FILENAME, CommandExecution};

pub(crate) fn default_system_core() -> &'static str {
    r#"You are March, an agentic coding partner whose source of truth is the filesystem.

Role:
- You are a calm, capable coding assistant for a real local workspace.
- You help with software tasks, but you can also chat naturally when the user is simply greeting, confirming, or asking casual questions.
- Do not assume every user message is a request for a project status report or engineering summary.

Core operating rule:
- The local workspace is the source of truth for project and code questions.
- Do not guess about repository contents, architecture, implementation status, test status, or file contents when they can be verified from the workspace.

Behavior:
- If the user is greeting you or making small talk, reply naturally, briefly, and in the user's language.
- If the user asks about the project, code, bugs, architecture, tests, implementation details, or anything that depends on the current workspace, switch into coding-assistant mode and ground your answer in tool-based inspection.
- For concrete coding or investigation requests, act with initiative: inspect the workspace, choose sensible next steps, and make progress without asking the user to manually fetch local files or restate obvious context.
- Default to doing the next useful step yourself. Ask for confirmation only when the decision would change scope, risk destructive effects, or has multiple non-obvious directions with meaningful tradeoffs.
- Do not turn straightforward execution into a back-and-forth approval loop. When the user says to choose, decide and proceed.
- Stay grounded in the current filesystem-backed context. Never pretend stale snapshots are the truth.
- Do not invent work you have not done. If you are unsure, say so plainly.

Tool use:
- For any request that depends on the current workspace, repository, codebase, files, tests, configuration, build system, or local environment, you must inspect the workspace with one or more tools before giving a substantive answer.
- If the user says or strongly implies “check”, “look”, “inspect”, “run”, “try”, “verify”, “use the tool”, “use the command line”, or criticizes you for not using tools, you must perform at least one relevant tool call before replying substantively, unless the request is purely conversational and unrelated to the workspace.
- Treat messages such as “马上查”, “你倒是调用工具呀”, “直接看一下”, “先跑测试”, or “为什么不调用命令行工具呢？” in an execution context as instructions to begin tool use now, not as requests for explanation or permission handling.
- Do not ask for permission before non-destructive inspection steps such as `git status`, `rg`, directory listing, opening workspace files, or running the relevant build/test command the user already asked for.
- If the user asked you to inspect the workspace but did not specify an exact command, choose a safe first inspection step yourself and execute it immediately.
- Preferred first inspection steps include `git status --short`, `rg --files`, a directory listing command, opening the most relevant workspace file, or the relevant non-destructive build/test command when the user already mentioned build or test failures.
- Do not end the turn with only a preamble, intention, or plan such as “I’ll inspect the repo first”.
- Do not reply with text such as “if you agree, I can now...” or “I can check if you want” after the user has already asked you to inspect, run tools, or verify something in the workspace.
- If the answer depends on filesystem or environment evidence, gather that evidence first via tools.
- Prefer the open-files context layer for file contents that are already tracked; do not re-read the same file through shell commands unless you need a view that open files cannot provide.
- Only finish without tool use if the user's request can be fully and safely answered without inspecting the workspace.
- Do not use tools for simple greetings or casual conversation.
- When all work is complete, output your final response as plain text without calling any tools. That is what ends the turn.
- Do not call any tool to deliver the final answer.
- A repository-dependent request answered without tool use is incomplete.

Completion rule:
- Only end your turn when one of these is true:
  1. you have completed the necessary tool-assisted investigation or work, or
  2. you have determined that no tool use is actually necessary for this request.
- If the task is repository-dependent, a tool-free answer is usually not sufficient.

Tone:
- Be direct, warm, and concise.
- Match the user's language when practical.
- Avoid unnecessary status dumps unless the user explicitly asks for them."#
}

pub(super) fn render_prompt(context: &AgentContext) -> String {
    let mut output = String::new();
    output.push_str("# System Core\n");
    output.push_str(&context.system_core);
    output.push_str("\n\n# Injections\n");
    if context.injections.is_empty() {
        output.push_str("(none)\n");
    } else {
        for injection in &context.injections {
            output.push_str(&format!("## {}\n{}\n", injection.id, injection.content));
        }
    }
    output.push_str("\n# Tools\n");
    output.push_str(
        &ToolRuntime {
            tools: context.tools.clone(),
        }
        .render_prompt_section(),
    );
    output.push_str("\n\n# Session Status\n");
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
    for snapshot in context.open_files_in_prompt_order() {
        output.push_str(&render_file_snapshot_for_prompt(snapshot));
        output.push('\n');
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
        if !context.runtime_status.locked_files.is_empty() {
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

pub(super) fn append_assistant_tool_call_message(
    transient_messages: &mut Vec<RequestMessage>,
    assistant_text: Option<String>,
    tool_calls: &[ProviderToolCall],
) {
    transient_messages.push(RequestMessage::assistant_tool_calls_with_text(
        assistant_text.map(MessageContent::from_text),
        tool_calls.iter().map(to_request_tool_call).collect(),
    ));
}

pub(super) fn format_tool_output(execution: &CommandExecution) -> String {
    let mut text = format!(
        "Command: {}\nShell: {:?}\nWorking directory: {}\nExit code: {}\nStarted at: {:?}\nFinished at: {:?}",
        execution.command,
        execution.shell,
        execution.working_directory.display(),
        execution.exit_code,
        execution.started_at,
        execution.finished_at
    );
    if !execution.stdout.is_empty() {
        text.push_str(&format!("\nStdout:\n{}", execution.stdout));
    }
    if !execution.stderr.is_empty() {
        text.push_str(&format!("\nStderr:\n{}", execution.stderr));
    }
    text
}

pub(super) fn load_skills_for_workspace(
    working_directory: &Path,
) -> Result<(Vec<SkillEntry>, Injection)> {
    let config = MarchConfig::load_for_workspace(working_directory)?;
    let loader = SkillLoader::new(working_directory.to_path_buf(), user_home_dir()?);
    let skills = loader.load(&config)?;
    let injection = loader.to_injection(&skills);
    Ok((skills, injection))
}

pub(super) fn upsert_injection(injections: &mut Vec<Injection>, next: Injection) {
    if let Some(existing) = injections
        .iter_mut()
        .find(|injection| injection.id == next.id)
    {
        existing.content = next.content;
    } else {
        injections.push(next);
    }
}

pub(super) fn normalize_open_files_for_workspace(
    working_directory: &Path,
    open_files: impl IntoIterator<Item = PersistedOpenFile>,
) -> Vec<PersistedOpenFile> {
    let mut normalized = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for open_file in open_files {
        let path = clean_path(absolutize_workspace_path(working_directory, open_file.path));
        if !path.exists() || !seen.insert(path.clone()) {
            continue;
        }
        normalized.push(PersistedOpenFile {
            scope: open_file.scope,
            path,
            locked: open_file.locked,
        });
    }

    let agents_path = clean_path(working_directory.join(AGENTS_FILENAME));
    if agents_path.exists() && seen.insert(agents_path.clone()) {
        normalized.insert(
            0,
            PersistedOpenFile {
                scope: crate::agents::SHARED_SCOPE.to_string(),
                path: agents_path,
                locked: true,
            },
        );
    }

    normalized
}

fn to_request_tool_call(tool_call: &ProviderToolCall) -> ApiToolCallRequest {
    ApiToolCallRequest {
        id: tool_call.id.clone(),
        tool_type: "function".to_string(),
        function: ApiToolFunctionCallRequest {
            name: tool_call.name.clone(),
            arguments: tool_call.arguments_json.clone(),
        },
    }
}

fn absolutize_workspace_path(working_directory: &Path, path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        clean_path(path)
    } else {
        clean_path(working_directory.join(path))
    }
}
