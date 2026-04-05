use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::Deserialize;
use serde_json::Value;

use crate::context::ToolSummary;
use crate::provider::ProviderToolCall;
use crate::settings::SettingsStorage;

use super::editing::{delete_line_range, edit_lines, insert_line_block, replace_line_range};
use super::prompting::format_tool_output;
use super::shells::{AvailableShell, parse_shell};
use super::{AgentSession, CommandRequest, CommandShell, ToolOutcome};

impl AgentSession {
    pub(super) fn execute_tool_call(
        &mut self,
        tool_call: &ProviderToolCall,
    ) -> Result<ToolOutcome> {
        let args: Value = serde_json::from_str(&tool_call.arguments_json).with_context(|| {
            format!(
                "failed to decode arguments for tool {}: {}",
                tool_call.name, tool_call.arguments_json
            )
        })?;

        match tool_call.name.as_str() {
            "run_command" => {
                let args: RunCommandArgs =
                    serde_json::from_value(args).context("invalid run_command args")?;
                let execution = self.run_command(CommandRequest {
                    command: args.command,
                    shell: parse_shell(&args.shell)?,
                })?;
                Ok(ToolOutcome {
                    result_text: format_tool_output(&execution),
                    summary: Some(ToolSummary {
                        name: "run_command".to_string(),
                        summary: format!(
                            "{} (exit code {})",
                            execution.command, execution.exit_code
                        ),
                    }),
                })
            }
            "open_file" => {
                let args: PathArgs =
                    serde_json::from_value(args).context("invalid open_file args")?;
                let path = self.resolve_path(args.path);
                self.open_file(path.clone())?;
                Ok(simple_tool(
                    format!("opened {}", path.display()),
                    "open_file",
                    format!("开始追踪 {}", path.display()),
                ))
            }
            "close_file" => {
                let args: PathArgs =
                    serde_json::from_value(args).context("invalid close_file args")?;
                let path = self.resolve_path(args.path);
                self.close_file(path.clone())?;
                Ok(simple_tool(
                    format!("closed {}", path.display()),
                    "close_file",
                    format!("停止追踪 {}", path.display()),
                ))
            }
            "write_file" => {
                let args: WriteFileArgs =
                    serde_json::from_value(args).context("invalid write_file args")?;
                let path = self.resolve_path(args.path);
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
                let _guard = if path.exists() {
                    Some(self.watcher.store().begin_agent_write([path.clone()])?)
                } else {
                    None
                };
                fs::write(&path, args.content)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                self.track_written_file(&path)?;
                Ok(simple_tool(
                    format!("wrote {}", path.display()),
                    "write_file",
                    format!("写入了 {}", path.display()),
                ))
            }
            "replace_lines" => {
                let args: ReplaceLinesArgs =
                    serde_json::from_value(args).context("invalid replace_lines args")?;
                let path = self.resolve_path(args.path);
                edit_lines(&path, |lines| {
                    replace_line_range(lines, args.start_line, args.end_line, &args.new_content)
                })?;
                self.refresh_if_watched(&path)?;
                Ok(simple_tool(
                    format!(
                        "replaced lines {}-{} in {}",
                        args.start_line,
                        args.end_line,
                        path.display()
                    ),
                    "replace_lines",
                    format!(
                        "修改了 {} 第 {}-{} 行",
                        path.display(),
                        args.start_line,
                        args.end_line
                    ),
                ))
            }
            "insert_lines" => {
                let args: InsertLinesArgs =
                    serde_json::from_value(args).context("invalid insert_lines args")?;
                let path = self.resolve_path(args.path);
                edit_lines(&path, |lines| {
                    insert_line_block(lines, args.after_line, &args.new_content)
                })?;
                self.refresh_if_watched(&path)?;
                Ok(simple_tool(
                    format!("inserted after {} in {}", args.after_line, path.display()),
                    "insert_lines",
                    format!("在 {} 第 {} 行后插入内容", path.display(), args.after_line),
                ))
            }
            "delete_lines" => {
                let args: DeleteLinesArgs =
                    serde_json::from_value(args).context("invalid delete_lines args")?;
                let path = self.resolve_path(args.path);
                edit_lines(&path, |lines| {
                    delete_line_range(lines, args.start_line, args.end_line)
                })?;
                self.refresh_if_watched(&path)?;
                Ok(simple_tool(
                    format!(
                        "deleted lines {}-{} in {}",
                        args.start_line,
                        args.end_line,
                        path.display()
                    ),
                    "delete_lines",
                    format!(
                        "删除了 {} 第 {}-{} 行",
                        path.display(),
                        args.start_line,
                        args.end_line
                    ),
                ))
            }
            "write_note" => {
                let args: WriteNoteArgs =
                    serde_json::from_value(args).context("invalid write_note args")?;
                self.write_note(args.id.clone(), args.content);
                Ok(simple_tool(
                    format!("stored note {}", args.id),
                    "write_note",
                    format!("更新了 note {}", args.id),
                ))
            }
            "remove_note" => {
                let args: RemoveNoteArgs =
                    serde_json::from_value(args).context("invalid remove_note args")?;
                self.remove_note(&args.id);
                Ok(simple_tool(
                    format!("removed note {}", args.id),
                    "remove_note",
                    format!("移除了 note {}", args.id),
                ))
            }
            "create_agent" => {
                let args: CreateAgentArgs =
                    serde_json::from_value(args).context("invalid create_agent args")?;
                if args
                    .name
                    .trim()
                    .eq_ignore_ascii_case(crate::agents::MARCH_AGENT_NAME)
                {
                    bail!("cannot create march via create_agent");
                }
                let settings = SettingsStorage::open()?;
                let profile = settings.upsert_agent_profile(
                    args.name,
                    args.display_name,
                    args.description,
                    args.system_prompt,
                    args.avatar_color.unwrap_or_default(),
                    args.provider_id,
                    args.model,
                )?;
                self.refresh_agent_profiles()?;
                Ok(simple_tool(
                    format!("created agent {}", profile.name),
                    "create_agent",
                    format!("创建了角色 {}", profile.display_name),
                ))
            }
            "update_agent" => {
                let args: UpdateAgentArgs =
                    serde_json::from_value(args).context("invalid update_agent args")?;
                let settings = SettingsStorage::open()?;
                let name = args.name.trim().to_ascii_lowercase();
                if name.is_empty() {
                    bail!("agent name cannot be empty");
                }

                if name == crate::agents::MARCH_AGENT_NAME {
                    let current_prompt = settings
                        .snapshot()?
                        .custom_system_core
                        .unwrap_or_else(|| {
                            crate::agent::default_march_prompt().to_string()
                        });
                    let next_prompt = args.system_prompt.unwrap_or(current_prompt);
                    settings.set_custom_system_core(Some(next_prompt), true)?;
                    self.refresh_agent_profiles()?;
                    return Ok(simple_tool(
                        "updated march system prompt".to_string(),
                        "update_agent",
                        "更新了 March 角色提示词".to_string(),
                    ));
                }

                let existing = settings
                    .load_agent_profile_by_name(&name)?
                    .ok_or_else(|| anyhow!("agent {} not found", name))?;
                let clear_model_binding = args.clear_model_binding.unwrap_or(false);
                let profile = settings.upsert_agent_profile(
                    existing.name.clone(),
                    args.display_name.unwrap_or(existing.display_name),
                    args.description.unwrap_or(existing.description),
                    args.system_prompt.unwrap_or(existing.system_prompt),
                    args.avatar_color.unwrap_or(existing.avatar_color),
                    if clear_model_binding {
                        None
                    } else {
                        args.provider_id.or(existing.provider_id)
                    },
                    if clear_model_binding {
                        None
                    } else {
                        args.model.or(existing.model_id)
                    },
                )?;
                self.refresh_agent_profiles()?;
                Ok(simple_tool(
                    format!("updated agent {}", profile.name),
                    "update_agent",
                    format!("更新了角色 {}", profile.display_name),
                ))
            }
            "delete_agent" => {
                let args: DeleteAgentArgs =
                    serde_json::from_value(args).context("invalid delete_agent args")?;
                let name = args.name.trim().to_ascii_lowercase();
                if name == crate::agents::MARCH_AGENT_NAME {
                    bail!("cannot delete march");
                }
                if self.active_agent_name() == name {
                    bail!("cannot delete the currently active agent {}", name);
                }
                let settings = SettingsStorage::open()?;
                settings.delete_agent_profile(&name)?;
                self.refresh_agent_profiles()?;
                Ok(simple_tool(
                    format!("deleted agent {}", name),
                    "delete_agent",
                    format!("删除了角色 {}", name),
                ))
            }
            other => bail!("unknown tool call: {}", other),
        }
    }

    pub(super) fn resolve_shell(&self, shell: CommandShell) -> Result<AvailableShell> {
        self.available_shells
            .iter()
            .find(|candidate| candidate.kind == shell)
            .cloned()
            .with_context(|| format!("requested shell {} is not available", shell.label()))
    }

    pub(super) fn resolve_path(&self, path: PathBuf) -> PathBuf {
        if path.is_absolute() {
            path
        } else {
            self.working_directory.join(path)
        }
    }

    pub(super) fn refresh_if_watched(&self, path: &Path) -> Result<()> {
        if self.open_file_snapshots().contains_key(path) {
            self.watcher
                .store()
                .refresh_file(path, crate::context::ModifiedBy::Agent)?;
        }
        Ok(())
    }

    pub(super) fn track_written_file(&mut self, path: &Path) -> Result<()> {
        let scope = self.active_agent_name().to_string();
        if !self
            .open_files
            .iter()
            .any(|entry| entry.scope == scope && entry.path == path)
        {
            self.open_files.push(crate::storage::PersistedOpenFile {
                scope,
                path: path.to_path_buf(),
                locked: false,
            });
        }
        if self.open_file_snapshots().contains_key(path) {
            self.watcher
                .store()
                .refresh_file(path, crate::context::ModifiedBy::Agent)?;
        } else {
            self.watcher.watch_file(path.to_path_buf())?;
            self.watcher
                .store()
                .refresh_file(path, crate::context::ModifiedBy::Agent)?;
        }
        Ok(())
    }
}

pub(super) fn summarize_tool_call(name: &str, arguments_json: &str) -> String {
    let args = serde_json::from_str::<Value>(arguments_json).unwrap_or(Value::Null);
    match name {
        "run_command" => {
            let shell = args.get("shell").and_then(Value::as_str).unwrap_or("shell");
            let command = args.get("command").and_then(Value::as_str).unwrap_or("");
            if command.is_empty() {
                "run_command".to_string()
            } else {
                format!("run_command {} {}", shell, command)
            }
        }
        "open_file" | "close_file" | "write_file" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            if path.is_empty() {
                name.to_string()
            } else {
                format!("{name} {path}")
            }
        }
        "replace_lines" | "delete_lines" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            let start_line = args.get("start_line").and_then(Value::as_u64).unwrap_or(0);
            let end_line = args.get("end_line").and_then(Value::as_u64).unwrap_or(0);
            if path.is_empty() || start_line == 0 || end_line == 0 {
                name.to_string()
            } else {
                format!("{name} {path}:{start_line}-{end_line}")
            }
        }
        "insert_lines" => {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("");
            let after_line = args.get("after_line").and_then(Value::as_u64).unwrap_or(0);
            if path.is_empty() || after_line == 0 {
                name.to_string()
            } else {
                format!("{name} {path}:{after_line}")
            }
        }
        "write_note" | "remove_note" => {
            let id = args.get("id").and_then(Value::as_str).unwrap_or("");
            if id.is_empty() {
                name.to_string()
            } else {
                format!("{name} {id}")
            }
        }
        "create_agent" | "update_agent" | "delete_agent" => {
            let agent_name = args.get("name").and_then(Value::as_str).unwrap_or("");
            if agent_name.is_empty() {
                name.to_string()
            } else {
                format!("{name} {agent_name}")
            }
        }
        _ => name.to_string(),
    }
}

pub(super) fn preview_tool_result(result_text: &str) -> Option<String> {
    let preview = result_text
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("");
    if preview.is_empty() {
        return None;
    }

    if preview.chars().count() > 120 {
        Some(format!(
            "{}…",
            preview.chars().take(120).collect::<String>()
        ))
    } else {
        Some(preview.to_string())
    }
}

pub(super) fn format_tool_error(tool_name: &str, error: &anyhow::Error) -> String {
    format!("Tool `{tool_name}` failed.\nError: {error:#}")
}

fn simple_tool(result_text: String, name: &str, summary: String) -> ToolOutcome {
    ToolOutcome {
        result_text,
        summary: Some(ToolSummary {
            name: name.to_string(),
            summary,
        }),
    }
}

#[derive(Debug, Deserialize)]
struct RunCommandArgs {
    shell: String,
    command: String,
}

#[derive(Debug, Deserialize)]
struct PathArgs {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct WriteFileArgs {
    path: PathBuf,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ReplaceLinesArgs {
    path: PathBuf,
    start_line: usize,
    end_line: usize,
    new_content: String,
}

#[derive(Debug, Deserialize)]
struct InsertLinesArgs {
    path: PathBuf,
    after_line: usize,
    new_content: String,
}

#[derive(Debug, Deserialize)]
struct DeleteLinesArgs {
    path: PathBuf,
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Deserialize)]
struct WriteNoteArgs {
    id: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct RemoveNoteArgs {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct CreateAgentArgs {
    name: String,
    display_name: String,
    description: String,
    system_prompt: String,
    avatar_color: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct UpdateAgentArgs {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    system_prompt: Option<String>,
    avatar_color: Option<String>,
    provider_id: Option<i64>,
    model: Option<String>,
    clear_model_binding: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DeleteAgentArgs {
    name: String,
}
