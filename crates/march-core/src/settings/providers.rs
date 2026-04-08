use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use super::{
    ProviderRecord, ProviderType, SettingsStorage, delete_setting, normalize_optional_string,
    normalize_provider_base_url, system_time_from_unix, unix_timestamp,
};

impl SettingsStorage {
    pub fn list_providers(&self) -> Result<Vec<ProviderRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, provider_type, api_key, base_url, created_at
                 FROM providers
                 ORDER BY created_at ASC, id ASC",
            )
            .context("failed to prepare provider list query")?;

        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .context("failed to query providers")?;

        let mut providers = Vec::new();
        for row in rows {
            let (id, name, provider_type_raw, api_key, base_url_raw, created_at) =
                row.context("failed to decode provider row")?;
            let provider_type = ProviderType::from_db_value(&provider_type_raw)
                .ok_or_else(|| anyhow::anyhow!("unsupported provider type {provider_type_raw}"))?;
            providers.push(ProviderRecord {
                id,
                name,
                provider_type,
                api_key,
                base_url: normalize_optional_string(base_url_raw),
                created_at: system_time_from_unix(created_at)?,
            });
        }
        Ok(providers)
    }

    pub fn upsert_provider(
        &self,
        provider_id: Option<i64>,
        provider_type: ProviderType,
        name: impl AsRef<str>,
        api_key: impl AsRef<str>,
        base_url: impl AsRef<str>,
    ) -> Result<ProviderRecord> {
        let name = name.as_ref().trim();
        let api_key = api_key.as_ref().trim().to_string();
        let base_url = normalize_provider_base_url(provider_type, base_url.as_ref());

        if name.is_empty() {
            bail!("provider name cannot be empty");
        }
        if provider_type.base_url_required() && base_url.is_none() {
            bail!("provider base url cannot be empty");
        }

        let now = SystemTime::now();
        let created_at = unix_timestamp(now)?;

        match provider_id {
            Some(id) => {
                let existing = self.load_provider(id)?;
                let api_key = if api_key.is_empty() {
                    existing.api_key
                } else {
                    api_key
                };
                if provider_type.requires_api_key() && api_key.trim().is_empty() {
                    bail!("provider api key cannot be empty");
                }
                let affected = self
                    .connection
                    .execute(
                        "UPDATE providers
                         SET name = ?2, provider_type = ?3, api_key = ?4, base_url = ?5
                         WHERE id = ?1",
                        params![
                            id,
                            name,
                            provider_type.as_db_value(),
                            api_key,
                            base_url.as_deref().unwrap_or_default(),
                        ],
                    )
                    .context("failed to update provider")?;

                if affected == 0 {
                    bail!("provider {} not found", id);
                }

                self.load_provider(id)
            }
            None => {
                if provider_type.requires_api_key() && api_key.is_empty() {
                    bail!("provider api key cannot be empty");
                }
                self.connection
                    .execute(
                        "INSERT INTO providers (name, provider_type, api_key, base_url, created_at)
                         VALUES (?1, ?2, ?3, ?4, ?5)",
                        params![
                            name,
                            provider_type.as_db_value(),
                            api_key,
                            base_url.as_deref().unwrap_or_default(),
                            created_at,
                        ],
                    )
                    .context("failed to insert provider")?;
                let id = self.connection.last_insert_rowid();
                self.load_provider(id)
            }
        }
    }

    pub fn delete_provider(&self, provider_id: i64) -> Result<()> {
        let affected = self
            .connection
            .execute("DELETE FROM providers WHERE id = ?1", params![provider_id])
            .context("failed to delete provider")?;

        if affected == 0 {
            bail!("provider {} not found", provider_id);
        }

        let default_model_config_id = self.get_setting_i64("default_model_config_id")?;
        if let Some(default_model_config_id) = default_model_config_id {
            let default_model_config = self.load_model_config(default_model_config_id)?;
            if default_model_config.provider_id == provider_id {
                delete_setting(&self.connection, "default_model_config_id")?;
            }
        }

        Ok(())
    }

    pub fn load_provider(&self, provider_id: i64) -> Result<ProviderRecord> {
        let row = self
            .connection
            .query_row(
                "SELECT id, name, provider_type, api_key, base_url, created_at
                 FROM providers
                 WHERE id = ?1",
                params![provider_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                },
            )
            .optional()
            .context("failed to load provider")?
            .ok_or_else(|| anyhow::anyhow!("provider {} not found", provider_id))?;

        let provider_type = ProviderType::from_db_value(&row.2)
            .ok_or_else(|| anyhow::anyhow!("unsupported provider type {}", row.2))?;

        Ok(ProviderRecord {
            id: row.0,
            name: row.1,
            provider_type,
            api_key: row.3,
            base_url: normalize_optional_string(row.4),
            created_at: system_time_from_unix(row.5)?,
        })
    }
}
