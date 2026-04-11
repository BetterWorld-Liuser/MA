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
                memorize_tool(),
                recall_memory_tool(),
                update_memory_tool(),
                forget_memory_tool(),
                create_agent_tool(),
                update_agent_tool(),
                delete_agent_tool(),
            ],
        }
    }

    pub fn render_prompt_section(&self) -> String {
        let mut output = String::new();
        output.push_str("The following tools are available in this session.\n\n");
        output.push_str("Tool selection principles:\n");
        output.push_str("- Choose the narrowest tool that directly matches the task.\n");
        output.push_str("- For inspecting or editing workspace files, prefer the file tools (open_file, write_file, line-based edits) over run_command. File tools are deterministic, cross-platform, and integrated with the watcher-backed open-files context layer.\n");
        output.push_str("- Reserve run_command for capabilities that require the external environment: compilation, tests, git, grep, shell pipelines, or CLI tools.\n");
        output.push_str("- All file content in the open-files context is shown with line numbers. Line-based edit tools (replace_lines, insert_lines, delete_lines) use these line numbers as absolute positions — no text matching is needed.\n\n");

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
        description: "Run a shell command in the workspace. Use this for capabilities that require the external environment: compilation, tests, git, grep/search, shell pipelines, build scripts, or any installed CLI tool. The command runs in a specified shell interpreter with the workspace root as the working directory.",
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
            ToolParameter {
                name: "timeout_secs",
                kind: "integer",
                required: false,
                description: "Optional command timeout in seconds. If omitted, March defaults to 10 seconds.",
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
            "Default timeout for run_command is 10 seconds.".to_string(),
            "Set timeout_secs when the command is expected to take longer or should fail faster.".to_string(),
            "Only choose a shell from the available-shell list above.".to_string(),
            "Each run_command tool call may execute exactly one command. The arguments must be a single JSON object with required fields shell and command, plus optional timeout_secs.".to_string(),
            "If you need to run two commands, emit two separate run_command tool calls instead of concatenating commands or JSON objects.".to_string(),
            "Use run_command when you need external environment capabilities such as git, compilers, test runners, shell pipelines, or existing CLI tools.".to_string(),
            "Do not use run_command just to read a file that is already present in the open-files context layer; use that watcher-backed snapshot directly.".to_string(),
        ],
    }
}

fn open_file_tool() -> ToolDefinition {
    ToolDefinition {
        name: "open_file",
        description: "Start tracking a file so its latest watcher-backed snapshot appears in the open-files context layer. The snapshot is kept in sync with disk: any external change is automatically reflected, and the content shown always includes line numbers for precise editing.",
        parameters: vec![ToolParameter {
            name: "path",
            kind: "path",
            required: true,
            description: "The file to add into Ma's open-file set.",
        }],
        notes: vec![
            "There is no read_file tool: opening a file IS reading it. The real on-disk content appears in context immediately and stays up to date via the file watcher.".to_string(),
            "Prefer open_file over run_command when the goal is to inspect a workspace file's content.".to_string(),
            "Once a file is open, reuse the open-files context instead of re-reading the same path through shell commands.".to_string(),
            "Use open_file before line-based edits when the file is not already present in the open-files layer.".to_string(),
            "Content limits: binary files (null bytes in the first 8 KB) are rejected. Text files are truncated at 2,000 lines, each line is capped at 1,000 characters, and overall rendered content is capped at 100 KB. If a file is truncated, a header note explains the limits — use run_command with grep/head/tail to access the rest.".to_string(),
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
        description: "Create a new file or overwrite an existing file with the given content in one operation. The file is automatically added to the watcher-backed open-files set so its snapshot stays current.",
        parameters: vec![
            ToolParameter {
                name: "path",
                kind: "path",
                required: true,
                description: "The file to create or overwrite. Parent directories are created if they do not exist.",
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
            "After write_file, the file is tracked and its snapshot in the open-files context reflects the written content.".to_string(),
            "On success, write_file returns a unified diff immediately so you can verify the change within the same turn without reopening the file.".to_string(),
            "Use line-based edit tools (replace_lines, insert_lines, delete_lines) instead when only a small region needs to change — they save tokens by only outputting the diff.".to_string(),
        ],
    }
}

fn replace_lines_tool() -> ToolDefinition {
    ToolDefinition {
        name: "replace_lines",
        description: "Replace an inclusive line range [start_line, end_line] with new content. Line numbers come from the open-files snapshot — they are absolute positions, not text patterns.",
        parameters: vec![
            ToolParameter {
                name: "path",
                kind: "path",
                required: true,
                description: "The file to edit (must be in the open-files set).",
            },
            ToolParameter {
                name: "start_line",
                kind: "integer",
                required: true,
                description: "The first line to replace (1-based, inclusive).",
            },
            ToolParameter {
                name: "end_line",
                kind: "integer",
                required: true,
                description: "The last line to replace (1-based, inclusive).",
            },
            ToolParameter {
                name: "new_content",
                kind: "string",
                required: true,
                description: "The replacement text. Can be fewer or more lines than the replaced range.",
            },
        ],
        notes: vec![
            "Use replace_lines for in-place edits where you only need to output the changed lines, saving tokens compared to write_file.".to_string(),
            "The new_content may differ in line count from the replaced range — subsequent line numbers shift accordingly.".to_string(),
            "If the file was modified externally after you last saw it, the watcher will detect the change and warn before the edit is applied.".to_string(),
            "On success, replace_lines returns a unified diff immediately so you can verify the exact hunk without reopening the file.".to_string(),
        ],
    }
}

fn insert_lines_tool() -> ToolDefinition {
    ToolDefinition {
        name: "insert_lines",
        description: "Insert new lines after a specific line number without replacing any existing content.",
        parameters: vec![
            ToolParameter {
                name: "path",
                kind: "path",
                required: true,
                description: "The file to edit (must be in the open-files set).",
            },
            ToolParameter {
                name: "after_line",
                kind: "integer",
                required: true,
                description: "Insert new content after this line number (1-based). Use 0 to prepend before the first line.",
            },
            ToolParameter {
                name: "new_content",
                kind: "string",
                required: true,
                description: "The content to insert. Can be one or more lines.",
            },
        ],
        notes: vec![
            "Use insert_lines for purely additive edits — adding new functions, imports, or blocks without touching existing lines.".to_string(),
            "Subsequent line numbers shift down by the number of inserted lines.".to_string(),
            "On success, insert_lines returns a unified diff immediately so you can inspect the inserted hunk without reopening the file.".to_string(),
        ],
    }
}

fn delete_lines_tool() -> ToolDefinition {
    ToolDefinition {
        name: "delete_lines",
        description: "Delete an inclusive line range [start_line, end_line] from a file without replacing it with new content.",
        parameters: vec![
            ToolParameter {
                name: "path",
                kind: "path",
                required: true,
                description: "The file to edit (must be in the open-files set).",
            },
            ToolParameter {
                name: "start_line",
                kind: "integer",
                required: true,
                description: "The first line to delete (1-based, inclusive).",
            },
            ToolParameter {
                name: "end_line",
                kind: "integer",
                required: true,
                description: "The last line to delete (1-based, inclusive).",
            },
        ],
        notes: vec![
            "Use delete_lines for pure removal — dropping imports, dead code, or empty blocks."
                .to_string(),
            "Subsequent line numbers shift up by the number of deleted lines.".to_string(),
            "On success, delete_lines returns a unified diff immediately so you can confirm the removed hunk without reopening the file.".to_string(),
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
            "Use write_note to preserve important state across turns, such as the current task target, a build error summary, or a working plan.".to_string(),
            "Treat note ids as stable memory slots: reuse the same id when updating the same fact, plan, identity, or status. Recommended ids: target, plan, build_output, user_identity.".to_string(),
            "If a note with that id already exists, write_note replaces it (upsert semantics); it does not append a second note.".to_string(),
            "Do not invent near-duplicate ids like target_v2 or latest_target — overwrite the original id instead.".to_string(),
            "Create a new id only when the new information truly needs to coexist with existing notes in future turns.".to_string(),
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
                name: "description",
                kind: "string",
                required: true,
                description: "One-sentence summary of what this agent is for.",
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

fn memorize_tool() -> ToolDefinition {
    ToolDefinition {
        name: "memorize",
        description: "Create or overwrite a long-term memory that survives across tasks and sessions. Project memories are stored in `.march/memories/*.md`; global memories are stored in the user settings database.",
        parameters: vec![
            ToolParameter {
                name: "id",
                kind: "string",
                required: true,
                description: "Stable memory id. For project memories this becomes the markdown filename. Reuse the same id to refresh an existing memory instead of creating duplicates.",
            },
            ToolParameter {
                name: "memory_type",
                kind: "string",
                required: true,
                description: "Free-form memory type such as fact, decision, pattern, preference, or caveat.",
            },
            ToolParameter {
                name: "topic",
                kind: "string",
                required: true,
                description: "Topic bucket used for grouping related memories, such as auth, testing, style, or deployment.",
            },
            ToolParameter {
                name: "title",
                kind: "string",
                required: true,
                description: "One-line summary shown in the memory index.",
            },
            ToolParameter {
                name: "content",
                kind: "string",
                required: true,
                description: "The full memory detail body.",
            },
            ToolParameter {
                name: "tags",
                kind: "string_array",
                required: true,
                description: "Keyword list for retrieval. Provide it as an array of strings.",
            },
            ToolParameter {
                name: "scope",
                kind: "string",
                required: false,
                description: "Optional scope: shared or a specific agent name.",
            },
            ToolParameter {
                name: "level",
                kind: "string",
                required: false,
                description: "Optional level: project or global. When omitted, preference defaults to global and everything else defaults to project.",
            },
        ],
        notes: vec![
            "Before calling memorize, check whether the same memory already exists in the Memory Index. If it does, reuse that exact id.".to_string(),
            "Prefer update_memory for changing an existing durable fact or preference. Use memorize only when you are creating a genuinely new memory slot.".to_string(),
            "Do not create near-duplicate memories with fresh ids when an indexed memory already represents the same fact, preference, or decision.".to_string(),
            "Use memories for durable project knowledge, decisions, workflows, and user preferences — not for short-lived task state.".to_string(),
        ],
    }
}

fn recall_memory_tool() -> ToolDefinition {
    ToolDefinition {
        name: "recall_memory",
        description: "Load the full content for one indexed memory by id.",
        parameters: vec![ToolParameter {
            name: "id",
            kind: "string",
            required: true,
            description: "Memory id from the memory index, usually prefixed like p:auth-policy or g:user-style.",
        }],
        notes: vec![
            "recall_memory increments the memory's access count and resets its skip count."
                .to_string(),
        ],
    }
}

fn update_memory_tool() -> ToolDefinition {
    ToolDefinition {
        name: "update_memory",
        description: "Update selected fields on an existing long-term memory.",
        parameters: vec![
            ToolParameter {
                name: "id",
                kind: "string",
                required: true,
                description: "Existing memory id, prefixed or raw.",
            },
            ToolParameter {
                name: "title",
                kind: "string",
                required: false,
                description: "Optional replacement title.",
            },
            ToolParameter {
                name: "content",
                kind: "string",
                required: false,
                description: "Optional replacement content.",
            },
            ToolParameter {
                name: "tags",
                kind: "string_array",
                required: false,
                description: "Optional replacement tag array.",
            },
            ToolParameter {
                name: "topic",
                kind: "string",
                required: false,
                description: "Optional replacement topic.",
            },
            ToolParameter {
                name: "memory_type",
                kind: "string",
                required: false,
                description: "Optional replacement memory type.",
            },
        ],
        notes: vec![
            "When the Memory Index already shows the target memory, treat its displayed id as the source of truth for edits.".to_string(),
            "Use update_memory when the same durable fact still exists but needs a cleaner title, fresher content, or new retrieval tags.".to_string(),
            "Prefer update_memory over memorize whenever the new information replaces or refines an existing memory instead of introducing a separate one.".to_string(),
        ],
    }
}

fn forget_memory_tool() -> ToolDefinition {
    ToolDefinition {
        name: "forget_memory",
        description: "Delete an outdated or redundant long-term memory.",
        parameters: vec![ToolParameter {
            name: "id",
            kind: "string",
            required: true,
            description: "Existing memory id, prefixed or raw.",
        }],
        notes: vec![
            "Use forget_memory when the stored information is no longer true or a new consolidated memory has replaced several older ones.".to_string(),
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
                name: "description",
                kind: "string",
                required: false,
                description: "Optional new one-sentence role summary.",
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
        assert!(prompt.contains("Default timeout for run_command is 10 seconds."));
        assert!(prompt.contains("timeout_secs"));
        assert!(prompt.contains("Each run_command tool call may execute exactly one command."));
        assert!(prompt.contains("emit two separate run_command tool calls"));
        assert!(prompt.contains("## open_file"));
        assert!(prompt.contains("Prefer open_file over run_command"));
        assert!(prompt.contains("Do not use run_command just to read a file"));
        assert!(prompt.contains("Prefer update_memory over memorize"));
        assert!(prompt.contains("Do not create near-duplicate memories with fresh ids"));
        assert!(prompt.contains("## close_file"));
        assert!(prompt.contains("## replace_lines"));
        assert!(prompt.contains("## write_note"));
        assert!(prompt.contains("## remove_note"));
    }
}
