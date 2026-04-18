use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::context::ToolSummary;
use crate::memory::{MemorizeRequest, UpdateMemoryRequest};
use crate::provider::ProviderToolCall;
use crate::settings::SettingsStorage;

use super::editing::{delete_line_range, edit_lines, insert_line_block, replace_line_range};
use super::file_diffs::format_file_diff;
use super::prompting::format_tool_output;
use super::shells::{AvailableShell, parse_shell};
use super::{
    AgentSession, CommandOutputStreamUpdate, CommandRequest, CommandShell, ToolOutcome,
    TurnCancellation, session::DEFAULT_RUN_COMMAND_TIMEOUT,
};

impl AgentSession {
    pub(super) async fn execute_tool_call(
        &mut self,
        tool_call: &ProviderToolCall,
        cancellation: &TurnCancellation,
    ) -> Result<ToolOutcome> {
        self.execute_tool_call_with_output(tool_call, cancellation, |_| Ok(()))
            .await
    }

    pub(super) async fn execute_tool_call_with_output<F>(
        &mut self,
        tool_call: &ProviderToolCall,
        cancellation: &TurnCancellation,
        mut on_output: F,
    ) -> Result<ToolOutcome>
    where
        F: FnMut(CommandOutputStreamUpdate) -> Result<()>,
    {
        let args: Value = serde_json::from_str(&tool_call.arguments_json).with_context(|| {
            format!(
                "failed to decode arguments for tool {}: {}",
                tool_call.name, tool_call.arguments_json
            )
        })?;

        match tool_call.name.as_str() {
            "run_command" => {
                let args = parse_run_command_args(args, &tool_call.arguments_json)?;
                let timeout_secs = args
                    .timeout_secs
                    .unwrap_or(DEFAULT_RUN_COMMAND_TIMEOUT.as_secs());
                if timeout_secs == 0 {
                    bail!("invalid run_command args: timeout_secs must be at least 1");
                }
                let execution = self
                    .run_command_with_output(
                        CommandRequest {
                            command: args.command,
                            shell: parse_shell(&args.shell)?,
                            timeout: Duration::from_secs(timeout_secs),
                        },
                        cancellation,
                        &mut on_output,
                    )
                    .await?;
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
                    format!(
                        "opened {}\n\nThe file's full contents are now rendered in the [open_files] section of your context. Read them there to complete the task — do not call read tools on this path again unless you need a view outside the watcher snapshot.",
                        path.display()
                    ),
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
                let before = if path.exists() {
                    Some(
                        fs::read_to_string(&path)
                            .with_context(|| format!("failed to read {}", path.display()))?,
                    )
                } else {
                    None
                };
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .with_context(|| format!("failed to create {}", parent.display()))?;
                }
                let _guard = if path.exists() {
                    Some(self.watcher.store().begin_agent_write([path.clone()])?)
                } else {
                    None
                };
                fs::write(&path, &args.content)
                    .with_context(|| format!("failed to write {}", path.display()))?;
                self.track_written_file(&path)?;
                let diff = format_file_diff(&path, before.as_deref().unwrap_or(""), &args.content);
                Ok(simple_tool(
                    diff.rendered,
                    "write_file",
                    format!("写入了 {}", path.display()),
                ))
            }
            "replace_lines" => {
                let args: ReplaceLinesArgs =
                    serde_json::from_value(args).context("invalid replace_lines args")?;
                let path = self.resolve_path(args.path);
                let edit_result = edit_lines(&path, |lines| {
                    replace_line_range(lines, args.start_line, args.end_line, &args.new_content)
                })?;
                self.refresh_if_watched(&path)?;
                let diff = format_file_diff(&path, &edit_result.before, &edit_result.after);
                Ok(simple_tool(
                    diff.rendered,
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
                let edit_result = edit_lines(&path, |lines| {
                    insert_line_block(lines, args.after_line, &args.new_content)
                })?;
                self.refresh_if_watched(&path)?;
                let diff = format_file_diff(&path, &edit_result.before, &edit_result.after);
                Ok(simple_tool(
                    diff.rendered,
                    "insert_lines",
                    format!("在 {} 第 {} 行后插入内容", path.display(), args.after_line),
                ))
            }
            "delete_lines" => {
                let args: DeleteLinesArgs =
                    serde_json::from_value(args).context("invalid delete_lines args")?;
                let path = self.resolve_path(args.path);
                let edit_result = edit_lines(&path, |lines| {
                    delete_line_range(lines, args.start_line, args.end_line)
                })?;
                self.refresh_if_watched(&path)?;
                let diff = format_file_diff(&path, &edit_result.before, &edit_result.after);
                Ok(simple_tool(
                    diff.rendered,
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
            "memorize" => {
                let args: MemorizeArgs =
                    serde_json::from_value(args).context("invalid memorize args")?;
                let active_agent = self.active_agent_name().to_string();
                let memory = self.memory_manager.memorize(
                    MemorizeRequest {
                        id: args.id,
                        memory_type: args.memory_type,
                        topic: args.topic,
                        title: args.title,
                        content: args.content,
                        tags: args.tags,
                        scope: args.scope,
                        level: args.level,
                    },
                    &active_agent,
                )?;
                Ok(simple_tool(
                    format!("stored memory {}", memory.prefixed_id()),
                    "memorize",
                    format!("写入了记忆 {}", memory.prefixed_id()),
                ))
            }
            "recall_memory" => {
                let args: MemoryIdArgs =
                    serde_json::from_value(args).context("invalid recall_memory args")?;
                let active_agent = self.active_agent_name().to_string();
                let memory = self.memory_manager.recall(&args.id, &active_agent)?;
                Ok(ToolOutcome {
                    result_text: format!(
                        "id: {}\nlevel: {:?}\ntype: {}\ntopic: {}\ntitle: {}\ncontent:\n{}",
                        memory.prefixed_id(),
                        memory.level,
                        memory.memory_type,
                        memory.topic,
                        memory.title,
                        memory.content
                    ),
                    summary: Some(ToolSummary {
                        name: "recall_memory".to_string(),
                        summary: format!("查看了记忆 {}", memory.prefixed_id()),
                    }),
                })
            }
            "update_memory" => {
                let args: UpdateMemoryArgs =
                    serde_json::from_value(args).context("invalid update_memory args")?;
                let memory = self.memory_manager.update_memory(UpdateMemoryRequest {
                    id: args.id,
                    title: args.title,
                    content: args.content,
                    tags: args.tags,
                    topic: args.topic,
                    memory_type: args.memory_type,
                })?;
                Ok(simple_tool(
                    format!("updated memory {}", memory.prefixed_id()),
                    "update_memory",
                    format!("更新了记忆 {}", memory.prefixed_id()),
                ))
            }
            "forget_memory" => {
                let args: MemoryIdArgs =
                    serde_json::from_value(args).context("invalid forget_memory args")?;
                self.memory_manager.forget(&args.id)?;
                Ok(simple_tool(
                    format!("forgot memory {}", args.id),
                    "forget_memory",
                    format!("删除了记忆 {}", args.id),
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
                        .unwrap_or_else(|| crate::agent::default_march_prompt().to_string());
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
                let reassigned_count = self.memory_manager.reassign_scope_from_agent(&name)?;
                let settings = SettingsStorage::open()?;
                settings.delete_agent_profile(&name)?;
                self.refresh_agent_profiles()?;
                Ok(simple_tool(
                    format!("deleted agent {}", name),
                    "delete_agent",
                    format!(
                        "删除了角色 {}，并把 {} 条私有记忆转成共享",
                        name, reassigned_count
                    ),
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
            let timeout_secs = args.get("timeout_secs").and_then(Value::as_u64);
            if command.is_empty() {
                "run_command".to_string()
            } else if let Some(timeout_secs) = timeout_secs {
                format!(
                    "run_command {} {} (timeout {}s)",
                    shell, command, timeout_secs
                )
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
        "memorize" | "recall_memory" | "update_memory" | "forget_memory" => {
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

fn parse_run_command_args(args: Value, original_arguments_json: &str) -> Result<RunCommandArgs> {
    match args {
        Value::Object(_) => serde_json::from_value(args).context(
            "invalid run_command args: expected exactly one object like {\"shell\":\"powershell\",\"command\":\"dir\"}",
        ),
        Value::String(encoded) => {
            let trimmed = encoded.trim();
            if looks_like_multiple_json_objects(trimmed) {
                bail!(
                    "invalid run_command args: received multiple concatenated JSON objects; each run_command tool call may execute exactly one command, so split them into separate tool calls. Raw args: {}",
                    original_arguments_json
                );
            } else if trimmed.starts_with('{') && trimmed.ends_with('}') {
                let nested: Value = serde_json::from_str(trimmed).context(
                    "invalid run_command args: received a stringified JSON object; emit the object directly instead of wrapping it in a string",
                )?;
                serde_json::from_value(nested).context(
                    "invalid run_command args: expected exactly one object like {\"shell\":\"powershell\",\"command\":\"dir\"}",
                )
            } else {
                bail!(
                    "invalid run_command args: expected a JSON object with fields shell and command, but received a plain string. Raw args: {}",
                    original_arguments_json
                );
            }
        }
        _ => bail!(
            "invalid run_command args: expected a JSON object with fields shell and command. Raw args: {}",
            original_arguments_json
        ),
    }
}

fn looks_like_multiple_json_objects(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.contains("}{") || trimmed.contains("}\n{") || trimmed.contains("}\r\n{")
}

#[derive(Debug, Deserialize)]
struct RunCommandArgs {
    shell: String,
    command: String,
    timeout_secs: Option<u64>,
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
struct MemorizeArgs {
    id: String,
    memory_type: String,
    topic: String,
    title: String,
    content: String,
    #[serde(deserialize_with = "deserialize_tags")]
    tags: Vec<String>,
    scope: Option<String>,
    level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MemoryIdArgs {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct UpdateMemoryArgs {
    id: String,
    title: Option<String>,
    content: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_tags")]
    tags: Option<Vec<String>>,
    topic: Option<String>,
    memory_type: Option<String>,
}

fn deserialize_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_tags_impl(deserializer).map(|tags| tags.unwrap_or_default())
}

fn deserialize_optional_tags<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    deserialize_tags_impl(deserializer)
}

fn deserialize_tags_impl<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum TagsArg {
        List(Vec<String>),
        Single(String),
    }

    let raw = Option::<TagsArg>::deserialize(deserializer)?;
    Ok(raw.map(|value| match value {
        TagsArg::List(list) => list,
        TagsArg::Single(text) => text
            .split([',', '，'])
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .map(ToString::to_string)
            .collect(),
    }))
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        MemorizeArgs, UpdateMemoryArgs, looks_like_multiple_json_objects, parse_run_command_args,
    };

    #[test]
    fn parse_run_command_args_accepts_direct_object() {
        let args = parse_run_command_args(
            json!({
                "shell": "powershell",
                "command": "Get-ChildItem",
            }),
            r#"{"shell":"powershell","command":"Get-ChildItem"}"#,
        )
        .expect("direct object should parse");

        assert_eq!(args.shell, "powershell");
        assert_eq!(args.command, "Get-ChildItem");
        assert_eq!(args.timeout_secs, None);
    }

    #[test]
    fn parse_run_command_args_accepts_stringified_single_object() {
        let args = parse_run_command_args(
            json!(r#"{"shell":"powershell","command":"Get-ChildItem"}"#),
            r#""{\"shell\":\"powershell\",\"command\":\"Get-ChildItem\"}""#,
        )
        .expect("stringified single object should parse");

        assert_eq!(args.shell, "powershell");
        assert_eq!(args.command, "Get-ChildItem");
        assert_eq!(args.timeout_secs, None);
    }

    #[test]
    fn parse_run_command_args_accepts_optional_timeout() {
        let args = parse_run_command_args(
            json!({
                "shell": "powershell",
                "command": "Get-ChildItem",
                "timeout_secs": 42,
            }),
            r#"{"shell":"powershell","command":"Get-ChildItem","timeout_secs":42}"#,
        )
        .expect("timeout should parse");

        assert_eq!(args.shell, "powershell");
        assert_eq!(args.command, "Get-ChildItem");
        assert_eq!(args.timeout_secs, Some(42));
    }

    #[test]
    fn parse_run_command_args_reports_concatenated_objects_clearly() {
        let error = parse_run_command_args(
            json!(r#"{"shell":"powershell","command":"Get-Content a"}{"shell":"powershell","command":"Get-Content b"}"#),
            r#""{\"shell\":\"powershell\",\"command\":\"Get-Content a\"}{\"shell\":\"powershell\",\"command\":\"Get-Content b\"}""#,
        )
        .expect_err("concatenated objects should fail");

        let message = error.to_string();
        assert!(message.contains("multiple concatenated JSON objects"));
        assert!(message.contains("split them into separate tool calls"));
    }

    #[test]
    fn multiple_json_object_detector_catches_common_shapes() {
        assert!(looks_like_multiple_json_objects(r#"{"a":1}{"b":2}"#));
        assert!(looks_like_multiple_json_objects("{\"a\":1}\n{\"b\":2}"));
        assert!(!looks_like_multiple_json_objects(r#"{"a":1}"#));
    }

    #[test]
    fn memorize_args_accept_comma_separated_tag_string() {
        let args: MemorizeArgs = serde_json::from_value(json!({
            "id": "user-preference-title",
            "memory_type": "preference",
            "topic": "style",
            "title": "Preferred title",
            "content": "Call the user 老大.",
            "tags": "称呼,偏好,老大"
        }))
        .expect("comma-separated tag string should parse");

        assert_eq!(args.tags, vec!["称呼", "偏好", "老大"]);
    }

    #[test]
    fn update_memory_args_accept_tag_array() {
        let args: UpdateMemoryArgs = serde_json::from_value(json!({
            "id": "g:user-preference-title",
            "tags": ["称呼", "偏好", "老大"]
        }))
        .expect("tag array should parse");

        assert_eq!(
            args.tags,
            Some(vec!["称呼".into(), "偏好".into(), "老大".into()])
        );
    }
}
