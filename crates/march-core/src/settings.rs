use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

mod agents;
mod normalize;
mod migrations;
mod models;
mod providers;
mod types;

pub use types::{
    AgentProfileRecord, ModelConfigRecord, ProviderModelRecord, ProviderRecord,
    ProviderSettingsSnapshot, ProviderType, ServerToolCapability, ServerToolConfig,
    ServerToolFormat,
};
pub(crate) use normalize::normalize_provider_base_url;

use normalize::{
    normalize_agent_description, normalize_agent_name, normalize_avatar_color,
    normalize_optional_string,
};

pub struct SettingsStorage {
    db_path: PathBuf,
    connection: Connection,
}

impl SettingsStorage {
    pub fn open() -> Result<Self> {
        let settings_dir = march_settings_dir()?;
        fs::create_dir_all(&settings_dir)
            .with_context(|| format!("failed to create {}", settings_dir.display()))?;

        let db_path = settings_dir.join("settings.db");
        let connection = Connection::open(&db_path)
            .with_context(|| format!("failed to open {}", db_path.display()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .context("failed to enable sqlite foreign_keys")?;

        let mut storage = Self {
            db_path,
            connection,
        };
        storage.initialize_schema()?;
        Ok(storage)
    }

    pub fn database_path(&self) -> &Path {
        &self.db_path
    }

    pub fn snapshot(&self) -> Result<ProviderSettingsSnapshot> {
        Ok(ProviderSettingsSnapshot {
            providers: self.list_providers()?,
            model_configs: self.list_model_configs()?,
            agent_profiles: self.list_agent_profiles()?,
            default_model_config_id: self.get_setting_i64("default_model_config_id")?,
            custom_system_core: self.get_setting("custom_system_core")?,
            use_custom_system_core: self
                .get_setting("use_custom_system_core")?
                .is_some_and(|value| value == "1"),
        })
    }

    pub fn set_custom_system_core(
        &self,
        system_prompt: Option<String>,
        enabled: bool,
    ) -> Result<()> {
        match system_prompt.and_then(normalize_optional_string) {
            Some(value) => set_setting(&self.connection, "custom_system_core", &value)?,
            None => delete_setting(&self.connection, "custom_system_core")?,
        }
        set_setting(
            &self.connection,
            "use_custom_system_core",
            if enabled { "1" } else { "0" },
        )?;
        Ok(())
    }

    pub fn set_default_model_config(&self, model_config_id: Option<i64>) -> Result<()> {
        match model_config_id {
            Some(id) => {
                self.load_model_config(id)?;
                set_setting(&self.connection, "default_model_config_id", &id.to_string())?;
            }
            None => delete_setting(&self.connection, "default_model_config_id")?,
        }

        Ok(())
    }

    pub fn default_model_config(&self) -> Result<Option<ModelConfigRecord>> {
        let Some(model_config_id) = self.get_setting_i64("default_model_config_id")? else {
            return Ok(None);
        };

        self.load_model_config(model_config_id).map(Some)
    }

    pub fn default_provider(&self) -> Result<Option<ProviderRecord>> {
        let Some(model_config) = self.default_model_config()? else {
            return Ok(None);
        };
        self.load_provider(model_config.provider_id).map(Some)
    }

    pub fn default_model(&self) -> Result<Option<String>> {
        Ok(self.default_model_config()?.map(|model| model.model_id))
    }

    fn get_setting(&self, key: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |row| row.get(0),
            )
            .optional()
            .context("failed to load setting")
    }

    fn get_setting_i64(&self, key: &str) -> Result<Option<i64>> {
        Ok(self
            .get_setting(key)?
            .and_then(|raw| raw.trim().parse::<i64>().ok()))
    }

    fn resolve_model_config_id(
        &self,
        provider_id: Option<i64>,
        model_id: Option<String>,
    ) -> Result<Option<i64>> {
        match (provider_id, model_id.and_then(normalize_optional_string)) {
            (Some(provider_id), Some(model_id)) => Ok(self
                .load_model_config_by_model_id(provider_id, &model_id)?
                .map(|model| model.id)),
            _ => Ok(None),
        }
    }

    fn resolve_agent_binding(
        &self,
        model_config_id: Option<i64>,
    ) -> Result<Option<ModelConfigRecord>> {
        model_config_id
            .map(|id| self.load_model_config(id))
            .transpose()
    }
}

pub fn march_settings_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("MA_SETTINGS_DIR").map(PathBuf::from) {
        return Ok(path);
    }

    Ok(user_home_dir()?.join(".march"))
}

pub fn user_home_dir() -> Result<PathBuf> {
    if cfg!(target_os = "windows") {
        if let Some(user_profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
            return Ok(user_profile);
        }

        let home_drive = std::env::var_os("HOMEDRIVE");
        let home_path = std::env::var_os("HOMEPATH");
        if let (Some(home_drive), Some(home_path)) = (home_drive, home_path) {
            return Ok(PathBuf::from(home_drive).join(home_path));
        }
    }

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        return Ok(home);
    }

    bail!("failed to resolve user home directory")
}

fn set_setting(connection: &Connection, key: &str, value: &str) -> Result<()> {
    connection
        .execute(
            "INSERT INTO settings (key, value)
             VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .with_context(|| format!("failed to write setting {}", key))?;
    Ok(())
}

fn delete_setting(connection: &Connection, key: &str) -> Result<()> {
    connection
        .execute("DELETE FROM settings WHERE key = ?1", params![key])
        .with_context(|| format!("failed to delete setting {}", key))?;
    Ok(())
}

fn unix_timestamp(time: SystemTime) -> Result<i64> {
    Ok(time
        .duration_since(UNIX_EPOCH)
        .context("time was before unix epoch")?
        .as_secs()
        .try_into()
        .context("unix timestamp overflowed i64")?)
}

fn system_time_from_unix(value: i64) -> Result<SystemTime> {
    let seconds = u64::try_from(value).context("negative unix timestamp in settings db")?;
    Ok(UNIX_EPOCH + std::time::Duration::from_secs(seconds))
}

fn delete_server_tools_for_model(connection: &Connection, model_config_id: i64) -> Result<()> {
    connection
        .execute(
            "DELETE FROM model_server_tools
             WHERE model_config_id = ?1",
            params![model_config_id],
        )
        .context("failed to delete model server tools")?;
    Ok(())
}

fn sync_server_tools_for_model(
    connection: &Connection,
    model_config_id: i64,
    server_tools: &[ServerToolConfig],
) -> Result<()> {
    delete_server_tools_for_model(connection, model_config_id)?;
    for tool in server_tools {
        connection
            .execute(
                "INSERT INTO model_server_tools (model_config_id, capability, format)
                 VALUES (?1, ?2, ?3)",
                params![
                    model_config_id,
                    tool.capability.as_db_value(),
                    tool.format.as_db_value(),
                ],
            )
            .context("failed to insert model server tool")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_temp_settings_storage(name: &str) -> SettingsStorage {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("ma-settings-{name}-{unique}"));
        fs::create_dir_all(&root).expect("failed to create temp settings directory");
        let db_path = root.join("settings.db");
        let connection = Connection::open(&db_path).expect("failed to open temp settings db");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("failed to enable foreign keys");
        let mut storage = SettingsStorage {
            db_path,
            connection,
        };
        storage
            .initialize_schema()
            .expect("failed to initialize schema");
        storage
    }

    #[test]
    fn upsert_model_config_rolls_back_when_server_tool_insert_fails() {
        let storage = open_temp_settings_storage("rollback");
        let provider = storage
            .upsert_provider(
                None,
                ProviderType::OpenAiCompat,
                "Compat",
                "test-key",
                "https://example.com/v1",
            )
            .expect("failed to create provider");

        storage
            .connection
            .execute_batch(
                "
                PRAGMA foreign_keys = OFF;
                BEGIN IMMEDIATE;
                ALTER TABLE model_server_tools RENAME TO model_server_tools_new;
                CREATE TABLE model_server_tools (
                    id               INTEGER PRIMARY KEY,
                    provider_id      INTEGER NOT NULL,
                    model_id         TEXT    NOT NULL,
                    capability       TEXT    NOT NULL,
                    format           TEXT    NOT NULL,
                    model_config_id  INTEGER
                );
                DROP TABLE model_server_tools_new;
                COMMIT;
                PRAGMA foreign_keys = ON;
                ",
            )
            .expect("failed to force legacy model_server_tools schema");

        let result = storage.upsert_model_config(
            None,
            provider.id,
            "gpt-5.4-mini",
            "",
            256000,
            128000,
            true,
            true,
            false,
            false,
            None,
            vec![ServerToolConfig {
                capability: ServerToolCapability::WebSearch,
                format: ServerToolFormat::OpenAiChatCompletions,
            }],
        );

        assert!(result.is_err(), "legacy schema should reject partial save");
        assert!(
            storage
                .load_model_config_by_model_id(provider.id, "gpt-5.4-mini")
                .expect("failed to reload model")
                .is_none(),
            "failed insert should not leave a half-persisted model config behind",
        );
    }

    #[test]
    fn initialize_schema_rebuilds_legacy_server_tool_table() {
        let storage = open_temp_settings_storage("rebuild");
        storage
            .upsert_provider(
                None,
                ProviderType::OpenAiCompat,
                "Compat",
                "test-key",
                "https://example.com/v1",
            )
            .expect("failed to create provider");
        let provider = storage.load_provider(1).expect("failed to load provider");

        storage
            .connection
            .execute(
                "INSERT INTO model_configs (
                    id, provider_id, model_id, display_name, context_window,
                    max_output, supports_tool_use, supports_vision, supports_audio, supports_pdf, probed_at
                 ) VALUES (?1, ?2, ?3, '', 128000, 4096, 1, 0, 0, 0, NULL)",
                params![1_i64, provider.id, "gpt-legacy"],
            )
            .expect("failed to seed model config");
        storage
            .connection
            .execute_batch(
                "
                PRAGMA foreign_keys = OFF;
                BEGIN IMMEDIATE;
                ALTER TABLE model_server_tools RENAME TO model_server_tools_new;
                CREATE TABLE model_server_tools (
                    id               INTEGER PRIMARY KEY,
                    provider_id      INTEGER NOT NULL,
                    model_id         TEXT    NOT NULL,
                    capability       TEXT    NOT NULL,
                    format           TEXT    NOT NULL,
                    model_config_id  INTEGER
                );
                INSERT INTO model_server_tools (id, provider_id, model_id, capability, format, model_config_id)
                VALUES (1, 1, 'gpt-legacy', 'web_search', 'openai_chat_completions', NULL);
                DROP TABLE model_server_tools_new;
                COMMIT;
                PRAGMA foreign_keys = ON;
                ",
            )
            .expect("failed to seed legacy server tool table");

        let mut reopened = SettingsStorage {
            db_path: storage.db_path.clone(),
            connection: Connection::open(&storage.db_path).expect("failed to reopen settings db"),
        };
        reopened
            .connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("failed to enable foreign keys on reopen");
        reopened
            .initialize_schema()
            .expect("failed to rebuild legacy schema");

        assert!(
            !reopened
                .table_has_column("model_server_tools", "provider_id")
                .expect("failed to inspect rebuilt schema"),
            "legacy provider_id column should be removed after rebuild",
        );

        let reloaded = reopened
            .load_model_config_by_model_id(provider.id, "gpt-legacy")
            .expect("failed to reload rebuilt model")
            .expect("rebuilt model should exist");
        assert_eq!(reloaded.server_tools.len(), 1);
        assert_eq!(
            reloaded.server_tools[0],
            ServerToolConfig {
                capability: ServerToolCapability::WebSearch,
                format: ServerToolFormat::OpenAiChatCompletions,
            }
        );
    }
}
