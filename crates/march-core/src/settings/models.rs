use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use super::{
    ModelConfigRecord, ServerToolCapability, ServerToolConfig, ServerToolFormat, SettingsStorage,
    normalize_optional_string, sync_server_tools_for_model, unix_timestamp,
};

impl SettingsStorage {
    pub fn list_model_configs(&self) -> Result<Vec<ModelConfigRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, provider_id, model_id, display_name,
                        context_window, max_output, supports_tool_use,
                        supports_vision, supports_audio, supports_pdf, probed_at
                 FROM model_configs
                 ORDER BY provider_id ASC, model_id COLLATE NOCASE ASC, id ASC",
            )
            .context("failed to prepare model config list query")?;

        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    normalize_optional_string(row.get::<_, String>(3)?),
                    row.get::<_, i64>(4)? as usize,
                    row.get::<_, i64>(5)? as usize,
                    row.get::<_, i64>(6)? != 0,
                    row.get::<_, i64>(7)? != 0,
                    row.get::<_, i64>(8)? != 0,
                    row.get::<_, i64>(9)? != 0,
                    row.get::<_, Option<i64>>(10)?,
                ))
            })
            .context("failed to query model configs")?;

        let mut model_configs = Vec::new();
        for row in rows {
            let row = row.context("failed to decode model config row")?;
            model_configs.push(ModelConfigRecord {
                id: row.0,
                provider_id: row.1,
                model_id: row.2.clone(),
                display_name: row.3,
                context_window: row.4,
                max_output_tokens: row.5,
                supports_tool_use: row.6,
                supports_vision: row.7,
                supports_audio: row.8,
                supports_pdf: row.9,
                probed_at: row.10,
                server_tools: self.load_server_tools_for_model(row.0)?,
            });
        }
        Ok(model_configs)
    }

    pub fn list_model_configs_for_provider(
        &self,
        provider_id: i64,
    ) -> Result<Vec<ModelConfigRecord>> {
        self.load_provider(provider_id)?;
        Ok(self
            .list_model_configs()?
            .into_iter()
            .filter(|model| model.provider_id == provider_id)
            .collect())
    }

    pub fn load_model_config_by_model_id(
        &self,
        provider_id: i64,
        model_id: &str,
    ) -> Result<Option<ModelConfigRecord>> {
        let normalized_model = model_id.trim();
        if normalized_model.is_empty() {
            return Ok(None);
        }

        self.connection
            .query_row(
                "SELECT id, provider_id, model_id, display_name,
                        context_window, max_output, supports_tool_use,
                        supports_vision, supports_audio, supports_pdf, probed_at
                 FROM model_configs
                 WHERE provider_id = ?1 AND lower(model_id) = lower(?2)",
                params![provider_id, normalized_model],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        normalize_optional_string(row.get::<_, String>(3)?),
                        row.get::<_, i64>(4)? as usize,
                        row.get::<_, i64>(5)? as usize,
                        row.get::<_, i64>(6)? != 0,
                        row.get::<_, i64>(7)? != 0,
                        row.get::<_, i64>(8)? != 0,
                        row.get::<_, i64>(9)? != 0,
                        row.get::<_, Option<i64>>(10)?,
                    ))
                },
            )
            .optional()
            .context("failed to load model config")
            .and_then(|row| {
                row.map(|row| {
                    Ok(ModelConfigRecord {
                        id: row.0,
                        provider_id: row.1,
                        model_id: row.2.clone(),
                        display_name: row.3,
                        context_window: row.4,
                        max_output_tokens: row.5,
                        supports_tool_use: row.6,
                        supports_vision: row.7,
                        supports_audio: row.8,
                        supports_pdf: row.9,
                        probed_at: row.10,
                        server_tools: self.load_server_tools_for_model(row.0)?,
                    })
                })
                .transpose()
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_model_config(
        &self,
        model_config_id: Option<i64>,
        provider_id: i64,
        model_id: impl AsRef<str>,
        display_name: impl AsRef<str>,
        context_window: usize,
        max_output_tokens: usize,
        supports_tool_use: bool,
        supports_vision: bool,
        supports_audio: bool,
        supports_pdf: bool,
        probed_at: Option<i64>,
        server_tools: Vec<ServerToolConfig>,
    ) -> Result<ModelConfigRecord> {
        self.load_provider(provider_id)?;
        let model_id = model_id.as_ref().trim();
        if model_id.is_empty() {
            bail!("model config model_id cannot be empty");
        }
        if context_window == 0 {
            bail!("model config context_window must be greater than 0");
        }
        if max_output_tokens == 0 {
            bail!("model config max_output_tokens must be greater than 0");
        }
        let display_name = normalize_optional_string(display_name.as_ref().to_string());
        let transaction = self
            .connection
            .unchecked_transaction()
            .context("failed to start model config transaction")?;

        let persisted_id = match model_config_id {
            Some(id) => {
                let affected = transaction
                    .execute(
                        "UPDATE model_configs
                         SET provider_id = ?2,
                             model_id = ?3,
                             display_name = ?4,
                             context_window = ?5,
                             max_output = ?6,
                             supports_tool_use = ?7,
                             supports_vision = ?8,
                             supports_audio = ?9,
                             supports_pdf = ?10,
                             probed_at = ?11
                         WHERE id = ?1",
                        params![
                            id,
                            provider_id,
                            model_id,
                            display_name.as_deref().unwrap_or_default(),
                            context_window as i64,
                            max_output_tokens as i64,
                            if supports_tool_use { 1 } else { 0 },
                            if supports_vision { 1 } else { 0 },
                            if supports_audio { 1 } else { 0 },
                            if supports_pdf { 1 } else { 0 },
                            probed_at,
                        ],
                    )
                    .context("failed to update model config")?;
                if affected == 0 {
                    bail!("model config {} not found", id);
                }
                id
            }
            None => {
                transaction
                    .execute(
                        "INSERT INTO model_configs (
                            provider_id, model_id, display_name, context_window,
                            max_output, supports_tool_use, supports_vision,
                            supports_audio, supports_pdf, probed_at
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        params![
                            provider_id,
                            model_id,
                            display_name.as_deref().unwrap_or_default(),
                            context_window as i64,
                            max_output_tokens as i64,
                            if supports_tool_use { 1 } else { 0 },
                            if supports_vision { 1 } else { 0 },
                            if supports_audio { 1 } else { 0 },
                            if supports_pdf { 1 } else { 0 },
                            probed_at,
                        ],
                    )
                    .context("failed to insert model config")?;
                transaction.last_insert_rowid()
            }
        };

        sync_server_tools_for_model(&transaction, persisted_id, &server_tools)?;
        transaction
            .commit()
            .context("failed to commit model config transaction")?;
        self.load_model_config(persisted_id)
    }

    pub fn delete_model_config(&self, model_config_id: i64) -> Result<()> {
        let affected = self
            .connection
            .execute(
                "DELETE FROM model_configs WHERE id = ?1",
                params![model_config_id],
            )
            .context("failed to delete model config")?;
        if affected == 0 {
            bail!("model config {} not found", model_config_id);
        }
        Ok(())
    }

    pub fn load_model_config(&self, model_config_id: i64) -> Result<ModelConfigRecord> {
        let row = self
            .connection
            .query_row(
                "SELECT id, provider_id, model_id, display_name,
                        context_window, max_output, supports_tool_use,
                        supports_vision, supports_audio, supports_pdf, probed_at
                 FROM model_configs
                 WHERE id = ?1",
                params![model_config_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        normalize_optional_string(row.get::<_, String>(3)?),
                        row.get::<_, i64>(4)? as usize,
                        row.get::<_, i64>(5)? as usize,
                        row.get::<_, i64>(6)? != 0,
                        row.get::<_, i64>(7)? != 0,
                        row.get::<_, i64>(8)? != 0,
                        row.get::<_, i64>(9)? != 0,
                        row.get::<_, Option<i64>>(10)?,
                    ))
                },
            )
            .optional()
            .context("failed to load model config")?
            .ok_or_else(|| anyhow::anyhow!("model config {} not found", model_config_id))?;
        Ok(ModelConfigRecord {
            id: row.0,
            provider_id: row.1,
            model_id: row.2.clone(),
            display_name: row.3,
            context_window: row.4,
            max_output_tokens: row.5,
            supports_tool_use: row.6,
            supports_vision: row.7,
            supports_audio: row.8,
            supports_pdf: row.9,
            probed_at: row.10,
            server_tools: self.load_server_tools_for_model(row.0)?,
        })
    }

    pub fn list_provider_models(&self) -> Result<Vec<ModelConfigRecord>> {
        self.list_model_configs()
    }

    pub fn list_provider_models_for_provider(
        &self,
        provider_id: i64,
    ) -> Result<Vec<ModelConfigRecord>> {
        self.list_model_configs_for_provider(provider_id)
    }

    pub fn load_provider_model_by_model_id(
        &self,
        provider_id: i64,
        model_id: &str,
    ) -> Result<Option<ModelConfigRecord>> {
        self.load_model_config_by_model_id(provider_id, model_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn upsert_provider_model(
        &self,
        provider_model_id: Option<i64>,
        provider_id: i64,
        model_id: impl AsRef<str>,
        display_name: impl AsRef<str>,
        context_window: usize,
        max_output_tokens: usize,
        supports_tool_use: bool,
        supports_vision: bool,
        supports_audio: bool,
        supports_pdf: bool,
        server_tools: Vec<ServerToolConfig>,
    ) -> Result<ModelConfigRecord> {
        self.load_provider(provider_id)?;
        self.upsert_model_config(
            provider_model_id,
            provider_id,
            model_id,
            display_name,
            context_window,
            max_output_tokens,
            supports_tool_use,
            supports_vision,
            supports_audio,
            supports_pdf,
            Some(unix_timestamp(SystemTime::now())?),
            server_tools,
        )
    }

    pub fn delete_provider_model(&self, provider_model_id: i64) -> Result<()> {
        self.delete_model_config(provider_model_id)
    }

    fn load_server_tools_for_model(&self, model_config_id: i64) -> Result<Vec<ServerToolConfig>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT capability, format
                 FROM model_server_tools
                 WHERE model_config_id = ?1
                 ORDER BY id ASC",
            )
            .context("failed to prepare model server tools query")?;

        let rows = statement
            .query_map(params![model_config_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .context("failed to query model server tools")?;

        let mut tools = Vec::new();
        for row in rows {
            let (capability_raw, format_raw) = row.context("failed to decode model server tool")?;
            let capability =
                ServerToolCapability::from_db_value(&capability_raw).ok_or_else(|| {
                    anyhow::anyhow!("unsupported server tool capability {capability_raw}")
                })?;
            let format = ServerToolFormat::from_db_value(&format_raw)
                .ok_or_else(|| anyhow::anyhow!("unsupported server tool format {format_raw}"))?;
            tools.push(ServerToolConfig { capability, format });
        }
        Ok(tools)
    }
}
