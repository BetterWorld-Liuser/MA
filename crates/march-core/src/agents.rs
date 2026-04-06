use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::paths::{clean_path, resolve_project_root};
use crate::settings::{SettingsStorage, march_settings_dir};

pub const MARCH_AGENT_NAME: &str = "march";
pub const SHARED_SCOPE: &str = "shared";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentProfile {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub system_prompt: String,
    pub avatar_color: String,
    pub provider_id: Option<i64>,
    pub model_id: Option<String>,
    pub source: AgentProfileSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProfileSource {
    BuiltIn,
    User,
    Project,
}

impl AgentProfile {
    pub fn built_in_march() -> Self {
        Self {
            name: MARCH_AGENT_NAME.to_string(),
            display_name: "March".to_string(),
            description: "默认通用搭档，负责通用 coding、查证和推进。".to_string(),
            system_prompt: crate::agent::default_march_prompt().to_string(),
            avatar_color: "#64748B".to_string(),
            provider_id: None,
            model_id: None,
            source: AgentProfileSource::BuiltIn,
        }
    }
}

pub fn load_agent_profiles(working_directory: &Path) -> Result<Vec<AgentProfile>> {
    let project_root = resolve_project_root(working_directory);
    let mut profiles = HashMap::new();
    let mut march = AgentProfile::built_in_march();
    if let Ok(settings) = SettingsStorage::open() {
        let snapshot = settings.snapshot()?;
        if snapshot.use_custom_system_core {
            if let Some(custom_system_core) = snapshot.custom_system_core {
                march.system_prompt = custom_system_core;
            }
        }
        for record in snapshot.agent_profiles {
            profiles.insert(
                record.name.clone(),
                AgentProfile {
                    name: record.name,
                    display_name: record.display_name,
                    description: record.description,
                    system_prompt: record.system_prompt,
                    avatar_color: record.avatar_color,
                    provider_id: record.provider_id,
                    model_id: record.model_id,
                    source: AgentProfileSource::User,
                },
            );
        }
    }
    profiles.insert(march.name.clone(), march);

    let user_dir = march_settings_dir()?.join("agents");
    for profile in load_profiles_from_dir(&user_dir)? {
        profiles.insert(profile.name.clone(), profile);
    }

    let project_dir = project_root.join(".march").join("agents");
    for profile in load_profiles_from_dir(&project_dir)? {
        profiles.insert(profile.name.clone(), profile);
    }

    let mut ordered = profiles.into_values().collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.name.cmp(&right.name));
    if let Some(index) = ordered
        .iter()
        .position(|profile| profile.name == MARCH_AGENT_NAME)
    {
        let march = ordered.remove(index);
        ordered.insert(0, march);
    }
    Ok(ordered)
}

fn load_profiles_from_dir(dir: &Path) -> Result<Vec<AgentProfile>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut entries = fs::read_dir(dir)
        .with_context(|| format!("failed to read agents directory {}", dir.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to enumerate agents directory {}", dir.display()))?;
    entries.sort_by_key(|entry| entry.path());

    let mut profiles = Vec::new();
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read agent profile {}", path.display()))?;
        if let Some(profile) = parse_agent_profile(&path, &content)? {
            profiles.push(profile);
        }
    }
    Ok(profiles)
}

fn parse_agent_profile(path: &Path, content: &str) -> Result<Option<AgentProfile>> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let (frontmatter, body) = split_frontmatter(content);
    let frontmatter = frontmatter.unwrap_or_default();
    let mut name = None;
    let mut display_name = None;
    let mut description = None;
    let mut avatar_color = None;
    let mut model = None;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once(':') else {
            continue;
        };
        let key = key.trim();
        let value = trim_wrapped_quotes(value.trim());
        match key {
            "name" => name = Some(value.to_string()),
            "display_name" => display_name = Some(value.to_string()),
            "description" => description = Some(value.to_string()),
            "avatar_color" => avatar_color = Some(value.to_string()),
            "model" => model = Some(value.to_string()),
            _ => {}
        }
    }

    let file_stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("agent")
        .trim()
        .to_string();
    let name = normalize_agent_name(name.unwrap_or(file_stem));
    if name.is_empty() {
        return Ok(None);
    }

    let model_id = parse_model_binding(model.as_deref());
    let display_name = display_name.unwrap_or_else(|| name.clone());
    let system_prompt = body.trim().to_string();
    let description = normalize_agent_description(description, &display_name, &system_prompt);
    Ok(Some(AgentProfile {
        name,
        display_name,
        description,
        system_prompt,
        avatar_color: avatar_color.unwrap_or_else(|| "#64748B".to_string()),
        provider_id: None,
        model_id,
        source: if path.starts_with(march_settings_dir()?.join("agents")) {
            AgentProfileSource::User
        } else {
            AgentProfileSource::Project
        },
    }))
}

fn split_frontmatter(content: &str) -> (Option<&str>, &str) {
    let normalized = content.trim_start_matches('\u{feff}');
    if !normalized.starts_with("---\n") && !normalized.starts_with("---\r\n") {
        return (None, normalized);
    }

    let separator = if normalized.starts_with("---\r\n") {
        "\r\n---"
    } else {
        "\n---"
    };
    let body_start = if normalized.starts_with("---\r\n") {
        5
    } else {
        4
    };
    if let Some(end_index) = normalized[body_start..].find(separator) {
        let frontmatter_end = body_start + end_index;
        let body = &normalized[(frontmatter_end + separator.len())..];
        let body = body
            .strip_prefix("\r\n")
            .or_else(|| body.strip_prefix('\n'))
            .unwrap_or(body);
        return (Some(&normalized[body_start..frontmatter_end]), body);
    }

    (None, normalized)
}

fn normalize_agent_name(raw: String) -> String {
    raw.trim().to_ascii_lowercase().replace(' ', "-")
}

fn trim_wrapped_quotes(raw: &str) -> &str {
    raw.trim_matches('"').trim_matches('\'')
}

fn parse_model_binding(raw: Option<&str>) -> Option<String> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return None;
    };
    Some(raw.to_string())
}

fn normalize_agent_description(
    explicit: Option<String>,
    display_name: &str,
    system_prompt: &str,
) -> String {
    explicit
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| summarize_agent_description(display_name, system_prompt))
}

fn summarize_agent_description(display_name: &str, system_prompt: &str) -> String {
    let first_sentence = system_prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| {
            line.trim_matches(|ch: char| {
                ch == '"' || ch == '\'' || ch == '。' || ch == '.' || ch == '：' || ch == ':'
            })
        })
        .filter(|line| !line.is_empty())
        .map(|line| line.chars().take(60).collect::<String>());

    first_sentence.unwrap_or_else(|| format!("负责 {} 相关工作。", display_name))
}

pub fn resolve_agent_file_path(working_directory: &Path, name: &str) -> PathBuf {
    clean_path(
        working_directory
            .join(".march")
            .join("agents")
            .join(format!("{name}.md")),
    )
}
