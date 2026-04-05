use std::path::Path;

use crate::agent::AvailableShell;

/// ToolRuntime 描述“当前这轮会话里，模型实际可用的工具长什么样”。
/// 这层信息既可以渲染成纯文本提示，也可以在后续翻译成 provider 的 tool schema。
#[derive(Debug, Clone)]
pub struct ToolRuntime {
    pub tools: Vec<ToolDefinition>,
}

impl ToolRuntime {
    pub fn for_session(available_shells: &[AvailableShell], working_directory: &Path) -> Self {
        Self {
            tools: vec![
                run_command_tool(available_shells, working_directory),
                open_file_tool(),
                close_file_tool(),
                write_file_tool(),
                replace_lines_tool(),
                insert_lines_tool(),
                delete_lines_tool(),
                write_note_tool(),
                remove_note_tool(),
                create_agent_tool(),
                update_agent_tool(),
                delete_agent_tool(),
            ],
        }
    }

    pub fn render_prompt_section(&self) -> String {
        let mut output = String::new();
        output.push_str("The following tools are available in this session.\n");
        output.push_str("Choose the narrowest tool that directly matches the task.\n\n");

        for tool in &self.tools {
            output.push_str(&format!("## {}\n", tool.name));
            output.push_str(&format!("{}\n", tool.description));

            if !tool.parameters.is_empty() {
                output.push_str("Parameters:\n");
                for parameter in &tool.parameters {
                    output.push_str(&format!(
                        "- {} ({}){}: {}\n",
                        parameter.name,
                        parameter.kind,
                        if parameter.required { ", required" } else { "" },
                        parameter.description
                    ));
                }
            }

            if !tool.notes.is_empty() {
                output.push_str("Usage notes:\n");
                for note in &tool.notes {
                    output.push_str(&format!("- {}\n", note));
                }
            }

            output.push('\n');
        }

        output.trim_end().to_string()
    }
}

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Vec<ToolParameter>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ToolParameter {
    pub name: &'static str,
    pub kind: &'static str,
    pub required: bool,
    pub description: &'static str,
}

fn run_command_tool(
    available_shells: &[AvailableShell],
    working_directory: &Path,
) -> ToolDefinition {
    ToolDefinition {
        name: "run_command",
        description: "Run a shell command in the workspace for compilation, tests, git, search, scripts, or other environment-backed operations.",
        parameters: vec![
            ToolParameter {
                name: "shell",
                kind: "enum",
                required: true,
                description: "The shell interpreter to use for this command.",
            },
            ToolParameter {
                name: "command",
                kind: "string",
                required: true,
                description: "The exact command text to execute in that shell.",
            },
        ],
        notes: vec![
            format!(
                "Available shells in this session: {}.",
                format_available_shells(available_shells)
            ),
            format!(
                "Current working directory for every run_command call: {}.",
                working_directory.display()
            ),
            "Only choose a shell from the available-shell list above.".to_string(),
            "Use run_command when you need external environment capabilities such as git, compilers, test runners, shell pipelines, or existing CLI tools.".to_string(),
            "Do not use run_command just to read a file that is already present in the open-files context layer; use that watcher-backed snapshot directly.".to_string(),
        ],
    }
}

fn open_file_tool() -> ToolDefinition {
    ToolDefinition {
        name: "open_file",
        description: "Start tracking a file so its latest watcher-backed snapshot appears in the open-files context layer.",
        parameters: vec![ToolParameter {
            name: "path",
            kind: "path",
            required: true,
            description: "The file to add into Ma's open-file set.",
        }],
        notes: vec![
            "There is no read_file tool in Ma's core design: opening a file makes its real on-disk content available in context.".to_string(),
            "Prefer open_file over run_command when the goal is to inspect a workspace file's content.".to_string(),
            "Once a file is open, reuse the open-files context instead of re-reading the same path through shell commands.".to_string(),
            "Use open_file before line-based edits when the file is not already present in the open-files layer.".to_string(),
        ],
    }
}

fn close_file_tool() -> ToolDefinition {
    ToolDefinition {
        name: "close_file",
        description: "Remove a tracked file from the open-files context layer and stop watching it.",
        parameters: vec![ToolParameter {
            name: "path",
            kind: "path",
            required: true,
            description: "The file to remove from context.",
        }],
        notes: vec![
            "Use close_file to shrink context when a file is no longer relevant.".to_string(),
        ],
    }
}

fn write_file_tool() -> ToolDefinition {
    ToolDefinition {
        name: "write_file",
        description: "Write the full content of a file in one operation and keep that file in Ma's watcher-backed open-files set.",
        parameters: vec![
            ToolParameter {
                name: "path",
                kind: "path",
                required: true,
                description: "The file to create or overwrite.",
            },
            ToolParameter {
                name: "content",
                kind: "string",
                required: true,
                description: "The exact file content to write.",
            },
        ],
        notes: vec![
            "Prefer write_file for creating a new file or replacing a file wholesale.".to_string(),
            "After write_file, the written file remains tracked so subsequent context reflects the real on-disk state.".to_string(),
            "Use line-based edit tools instead of write_file when only a small region needs to change.".to_string(),
        ],
    }
}

fn replace_lines_tool() -> ToolDefinition {
    ToolDefinition {
        name: "replace_lines",
        description: "Replace an inclusive line range with new content.",
        parameters: vec![
            ToolParameter {
                name: "path",
                kind: "path",
                required: true,
                description: "The file to edit.",
            },
            ToolParameter {
                name: "start_line",
                kind: "integer",
                required: true,
                description: "The first line to replace.",
            },
            ToolParameter {
                name: "end_line",
                kind: "integer",
                required: true,
                description: "The last line to replace.",
            },
            ToolParameter {
                name: "new_content",
                kind: "string",
                required: true,
                description: "The replacement content for the selected line range.",
            },
        ],
        notes: vec![
            "Use replace_lines when you know the precise line span to update.".to_string(),
            "If the file changed after it was opened, Ma should refresh the snapshot before applying the edit.".to_string(),
        ],
    }
}

fn insert_lines_tool() -> ToolDefinition {
    ToolDefinition {
        name: "insert_lines",
        description: "Insert new content after a specific line.",
        parameters: vec![
            ToolParameter {
                name: "path",
                kind: "path",
                required: true,
                description: "The file to edit.",
            },
            ToolParameter {
                name: "after_line",
                kind: "integer",
                required: true,
                description: "Insert new content after this line number.",
            },
            ToolParameter {
                name: "new_content",
                kind: "string",
                required: true,
                description: "The content to insert.",
            },
        ],
        notes: vec![
            "Use insert_lines for additive edits that do not replace an existing region."
                .to_string(),
        ],
    }
}

fn delete_lines_tool() -> ToolDefinition {
    ToolDefinition {
        name: "delete_lines",
        description: "Delete an inclusive line range from a file.",
        parameters: vec![
            ToolParameter {
                name: "path",
                kind: "path",
                required: true,
                description: "The file to edit.",
            },
            ToolParameter {
                name: "start_line",
                kind: "integer",
                required: true,
                description: "The first line to delete.",
            },
            ToolParameter {
                name: "end_line",
                kind: "integer",
                required: true,
                description: "The last line to delete.",
            },
        ],
        notes: vec![
            "Use delete_lines when the change is a pure removal and the line span is known."
                .to_string(),
        ],
    }
}

fn write_note_tool() -> ToolDefinition {
    ToolDefinition {
        name: "write_note",
        description: "Create or overwrite a persistent note in the AI's cross-turn working memory. Reusing an existing id replaces that note's content instead of creating a second similar entry.",
        parameters: vec![
            ToolParameter {
                name: "id",
                kind: "string",
                required: true,
                description: "The stable identifier for this note. If this id already exists, write_note overwrites the existing note in place.",
            },
            ToolParameter {
                name: "content",
                kind: "string",
                required: true,
                description: "The full note body to store as the current content for this id.",
            },
        ],
        notes: vec![
            "Use write_note to preserve important state across turns, such as the current task target or a useful build error summary.".to_string(),
            "Treat note ids as stable memory slots: reuse the same id when updating the same fact, plan, identity, or status.".to_string(),
            "If a note with that id already exists, write_note replaces it; it does not append a second note.".to_string(),
            "Prefer overwriting an existing semantically matching id over inventing near-duplicate ids like target_v2, latest_target, or user_identity_new.".to_string(),
            "Create a new id only when the new information truly needs to coexist with the old note in future turns.".to_string(),
        ],
    }
}

fn remove_note_tool() -> ToolDefinition {
    ToolDefinition {
        name: "remove_note",
        description: "Delete a note that is no longer useful in cross-turn context.",
        parameters: vec![ToolParameter {
            name: "id",
            kind: "string",
            required: true,
            description: "The note identifier to remove.",
        }],
        notes: vec![
            "Use remove_note once the stored state becomes stale, solved, or irrelevant."
                .to_string(),
        ],
    }
}

fn create_agent_tool() -> ToolDefinition {
    ToolDefinition {
        name: "create_agent",
        description: "Create a reusable agent profile that can later be invoked with @agent_name in chat.",
        parameters: vec![
            ToolParameter {
                name: "name",
                kind: "string",
                required: true,
                description: "Stable agent name used in @mentions, usually lowercase and concise.",
            },
            ToolParameter {
                name: "display_name",
                kind: "string",
                required: true,
                description: "Human-friendly label shown in the UI.",
            },
            ToolParameter {
                name: "system_prompt",
                kind: "string",
                required: true,
                description: "The role instruction that defines how this agent behaves.",
            },
            ToolParameter {
                name: "avatar_color",
                kind: "string",
                required: false,
                description: "Optional hex color used to distinguish this agent in the UI.",
            },
            ToolParameter {
                name: "provider_id",
                kind: "integer",
                required: false,
                description: "Optional provider binding. Omit to follow the task default.",
            },
            ToolParameter {
                name: "model",
                kind: "string",
                required: false,
                description: "Optional model binding. Omit to follow the task default.",
            },
        ],
        notes: vec![
            "Use create_agent when the user asks you to create a reusable reviewer, architect, planner, or similar role.".to_string(),
            "Agent names are normalized to lowercase mention names such as reviewer or architect.".to_string(),
        ],
    }
}

fn update_agent_tool() -> ToolDefinition {
    ToolDefinition {
        name: "update_agent",
        description: "Update an existing agent profile, including its prompt, display name, color, or model binding.",
        parameters: vec![
            ToolParameter {
                name: "name",
                kind: "string",
                required: true,
                description: "The existing agent name to update.",
            },
            ToolParameter {
                name: "display_name",
                kind: "string",
                required: false,
                description: "Optional new UI label.",
            },
            ToolParameter {
                name: "system_prompt",
                kind: "string",
                required: false,
                description: "Optional replacement role instruction.",
            },
            ToolParameter {
                name: "avatar_color",
                kind: "string",
                required: false,
                description: "Optional replacement avatar color.",
            },
            ToolParameter {
                name: "provider_id",
                kind: "integer",
                required: false,
                description: "Optional provider binding. Use null with clear_model_binding to follow the task default.",
            },
            ToolParameter {
                name: "model",
                kind: "string",
                required: false,
                description: "Optional replacement model binding.",
            },
            ToolParameter {
                name: "clear_model_binding",
                kind: "boolean",
                required: false,
                description: "When true, clears provider/model binding so the agent follows the task default.",
            },
        ],
        notes: vec![
            "Use update_agent when refining an existing role after the user asks for prompt or model changes.".to_string(),
            "If the target is march, only the system prompt is persisted as March customization; name itself cannot be renamed.".to_string(),
        ],
    }
}

fn delete_agent_tool() -> ToolDefinition {
    ToolDefinition {
        name: "delete_agent",
        description: "Delete a reusable agent profile.",
        parameters: vec![ToolParameter {
            name: "name",
            kind: "string",
            required: true,
            description: "The agent name to delete.",
        }],
        notes: vec![
            "Do not delete the built-in march agent.".to_string(),
            "Deleting the currently active non-March agent is rejected to avoid invalidating the running turn.".to_string(),
        ],
    }
}

fn format_available_shells(available_shells: &[AvailableShell]) -> String {
    available_shells
        .iter()
        .map(|shell| {
            if shell.program == shell.kind.label() {
                shell.kind.label().to_string()
            } else {
                format!("{} ({})", shell.kind.label(), shell.program)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::CommandShell;

    #[test]
    fn tooling_prompt_mentions_design_level_tools() {
        let runtime = ToolRuntime::for_session(
            &[
                AvailableShell {
                    kind: CommandShell::PowerShell,
                    program: "pwsh".to_string(),
                },
                AvailableShell {
                    kind: CommandShell::Cmd,
                    program: "cmd".to_string(),
                },
            ],
            Path::new("D:/playground/MA"),
        );

        let prompt = runtime.render_prompt_section();

        assert!(prompt.contains("## run_command"));

        assert!(prompt.contains("Available shells in this session: powershell (pwsh), cmd."));
        assert!(prompt.contains("## open_file"));
        assert!(prompt.contains("Prefer open_file over run_command"));
        assert!(prompt.contains("Do not use run_command just to read a file"));
        assert!(prompt.contains("## close_file"));
        assert!(prompt.contains("## replace_lines"));
        assert!(prompt.contains("## write_note"));
        assert!(prompt.contains("## remove_note"));
    }
}
