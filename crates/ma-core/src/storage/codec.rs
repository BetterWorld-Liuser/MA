use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::context::{Role, ToolSummary};

use super::TaskTitleSource;

pub fn unix_timestamp(time: SystemTime) -> Result<i64> {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .context("system time is before unix epoch")?;
    i64::try_from(duration.as_secs()).context("unix timestamp overflow")
}

pub fn optional_unix_timestamp(time: Option<SystemTime>) -> Result<Option<i64>> {
    time.map(unix_timestamp).transpose()
}

pub fn system_time_from_unix(timestamp: i64) -> Result<SystemTime> {
    let seconds = u64::try_from(timestamp).context("negative unix timestamp in database")?;
    Ok(UNIX_EPOCH + Duration::from_secs(seconds))
}

pub fn optional_system_time(timestamp: Option<i64>) -> Result<Option<SystemTime>> {
    timestamp.map(system_time_from_unix).transpose()
}

pub fn normalize_working_directory(path: &Path) -> Result<PathBuf> {
    std::fs::canonicalize(path)
        .with_context(|| format!("failed to resolve working directory {}", path.display()))
}

pub fn decode_working_directory(raw: Option<String>, workspace_root: &Path) -> Result<PathBuf> {
    let candidate = raw
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root.to_path_buf());
    normalize_working_directory(&candidate)
}

impl TaskTitleSource {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Auto => "auto",
            Self::Manual => "manual",
        }
    }

    pub fn from_db_value(value: &str) -> Result<Self> {
        match value {
            "default" => Ok(Self::Default),
            "auto" => Ok(Self::Auto),
            "manual" => Ok(Self::Manual),
            other => bail!("unknown task title source in database: {}", other),
        }
    }
}

pub fn role_to_db(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

pub fn role_from_db(role: &str) -> Result<Role> {
    match role {
        "system" => Ok(Role::System),
        "user" => Ok(Role::User),
        "assistant" => Ok(Role::Assistant),
        "tool" => Ok(Role::Tool),
        other => bail!("unknown role in database: {}", other),
    }
}

pub fn encode_tool_summaries(tool_summaries: &[ToolSummary]) -> Result<Option<String>> {
    if tool_summaries.is_empty() {
        return Ok(None);
    }

    serde_json::to_string(tool_summaries)
        .map(Some)
        .context("failed to encode tool summaries as json")
}

pub fn decode_tool_summaries(raw: Option<&str>) -> Result<Vec<ToolSummary>> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    serde_json::from_str(raw).context("failed to decode tool summaries from json")
}
