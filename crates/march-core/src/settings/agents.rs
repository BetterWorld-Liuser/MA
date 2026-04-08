use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use super::{
    AgentProfileRecord, SettingsStorage, normalize_agent_description, normalize_agent_name,
    normalize_avatar_color, system_time_from_unix, unix_timestamp,
};

impl SettingsStorage {
    pub fn list_agent_profiles(&self) -> Result<Vec<AgentProfileRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, display_name, system_prompt, avatar_color,
                        description, model_config_id, created_at, updated_at
                 FROM agent_profiles
                 ORDER BY created_at ASC, id ASC",
            )
            .context("failed to prepare agent profile list query")?;

        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            })
            .context("failed to query agent profiles")?;

        let mut profiles = Vec::new();
        for row in rows {
            let row = row.context("failed to decode agent profile row")?;
            let description = normalize_agent_description(row.5.clone(), &row.2, &row.3);
            let binding = self.resolve_agent_binding(row.6)?;
            profiles.push(AgentProfileRecord {
                id: row.0,
                name: row.1,
                display_name: row.2,
                system_prompt: row.3,
                avatar_color: row.4,
                description,
                model_config_id: row.6,
                provider_id: binding.as_ref().map(|model| model.provider_id),
                model_id: binding.as_ref().map(|model| model.model_id.clone()),
                created_at: system_time_from_unix(row.7)?,
                updated_at: system_time_from_unix(row.8)?,
            });
        }
        Ok(profiles)
    }

    pub fn load_agent_profile_by_name(&self, name: &str) -> Result<Option<AgentProfileRecord>> {
        let normalized_name = name.trim().to_ascii_lowercase();
        if normalized_name.is_empty() {
            return Ok(None);
        }

        self.connection
            .query_row(
                "SELECT id, name, display_name, system_prompt, avatar_color,
                        description, model_config_id, created_at, updated_at
                 FROM agent_profiles
                 WHERE lower(name) = lower(?1)",
                params![normalized_name],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                        row.get::<_, i64>(7)?,
                        row.get::<_, i64>(8)?,
                    ))
                },
            )
            .optional()
            .context("failed to load agent profile")
            .and_then(|row| {
                row.map(|row| {
                    let description = normalize_agent_description(row.5.clone(), &row.2, &row.3);
                    let binding = self.resolve_agent_binding(row.6)?;
                    Ok(AgentProfileRecord {
                        id: row.0,
                        name: row.1,
                        display_name: row.2,
                        system_prompt: row.3,
                        avatar_color: row.4,
                        description,
                        model_config_id: row.6,
                        provider_id: binding.as_ref().map(|model| model.provider_id),
                        model_id: binding.as_ref().map(|model| model.model_id.clone()),
                        created_at: system_time_from_unix(row.7)?,
                        updated_at: system_time_from_unix(row.8)?,
                    })
                })
                .transpose()
            })
    }

    pub fn upsert_agent_profile(
        &self,
        name: impl AsRef<str>,
        display_name: impl AsRef<str>,
        description: impl AsRef<str>,
        system_prompt: impl AsRef<str>,
        avatar_color: impl AsRef<str>,
        provider_id: Option<i64>,
        model_id: Option<String>,
    ) -> Result<AgentProfileRecord> {
        let name = normalize_agent_name(name.as_ref());
        if name.is_empty() {
            bail!("agent name cannot be empty");
        }
        let display_name = display_name.as_ref().trim();
        if display_name.is_empty() {
            bail!("agent display_name cannot be empty");
        }
        let description = normalize_agent_description(
            description.as_ref().trim().to_string(),
            display_name,
            system_prompt.as_ref(),
        );
        let system_prompt = system_prompt.as_ref().trim();
        if system_prompt.is_empty() {
            bail!("agent system_prompt cannot be empty");
        }
        let model_config_id = self.resolve_model_config_id(provider_id, model_id)?;
        let avatar_color = normalize_avatar_color(avatar_color.as_ref());
        let now = SystemTime::now();
        let now_ts = unix_timestamp(now)?;

        if let Some(existing) = self.load_agent_profile_by_name(&name)? {
            self.connection
                .execute(
                    "UPDATE agent_profiles
                     SET display_name = ?2,
                         description = ?3,
                         system_prompt = ?4,
                         avatar_color = ?5,
                         model_config_id = ?6,
                         updated_at = ?7
                     WHERE id = ?1",
                    params![
                        existing.id,
                        display_name,
                        description,
                        system_prompt,
                        avatar_color,
                        model_config_id,
                        now_ts,
                    ],
                )
                .context("failed to update agent profile")?;
        } else {
            self.connection
                .execute(
                    "INSERT INTO agent_profiles (
                        name, display_name, description, system_prompt, avatar_color,
                        model_config_id, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        name,
                        display_name,
                        description,
                        system_prompt,
                        avatar_color,
                        model_config_id,
                        now_ts,
                        now_ts,
                    ],
                )
                .context("failed to insert agent profile")?;
        }

        self.load_agent_profile_by_name(&name)?
            .ok_or_else(|| anyhow::anyhow!("agent {} was not persisted", name))
    }

    pub fn delete_agent_profile(&self, name: &str) -> Result<()> {
        let normalized_name = normalize_agent_name(name);
        if normalized_name.is_empty() {
            bail!("agent name cannot be empty");
        }

        let affected = self
            .connection
            .execute(
                "DELETE FROM agent_profiles WHERE lower(name) = lower(?1)",
                params![normalized_name],
            )
            .context("failed to delete agent profile")?;
        if affected == 0 {
            bail!("agent {} not found", normalized_name);
        }
        Ok(())
    }
}
