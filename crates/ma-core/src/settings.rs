use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

/// ProviderType 对应设置页和运行时可选的 provider 入口。
/// 旧版本只有 OpenAI-compatible，这里保留 compat 作为自定义端点类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderType {
    OpenAiCompat,
    OpenAi,
    Anthropic,
    Gemini,
    Fireworks,
    Together,
    Groq,
    Mimo,
    Nebius,
    Xai,
    DeepSeek,
    Zai,
    BigModel,
    Cohere,
    Ollama,
}

impl ProviderType {
    pub fn as_db_value(self) -> &'static str {
        match self {
            ProviderType::OpenAiCompat => "openai_compat",
            ProviderType::OpenAi => "openai",
            ProviderType::Anthropic => "anthropic",
            ProviderType::Gemini => "gemini",
            ProviderType::Fireworks => "fireworks",
            ProviderType::Together => "together",
            ProviderType::Groq => "groq",
            ProviderType::Mimo => "mimo",
            ProviderType::Nebius => "nebius",
            ProviderType::Xai => "xai",
            ProviderType::DeepSeek => "deepseek",
            ProviderType::Zai => "zai",
            ProviderType::BigModel => "bigmodel",
            ProviderType::Cohere => "cohere",
            ProviderType::Ollama => "ollama",
        }
    }

    pub fn from_db_value(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "openai_compat" => Some(Self::OpenAiCompat),
            "openai" => Some(Self::OpenAi),
            "anthropic" => Some(Self::Anthropic),
            "gemini" => Some(Self::Gemini),
            "fireworks" => Some(Self::Fireworks),
            "together" => Some(Self::Together),
            "groq" => Some(Self::Groq),
            "mimo" => Some(Self::Mimo),
            "nebius" => Some(Self::Nebius),
            "xai" => Some(Self::Xai),
            "deepseek" => Some(Self::DeepSeek),
            "zai" => Some(Self::Zai),
            "bigmodel" => Some(Self::BigModel),
            "cohere" => Some(Self::Cohere),
            "ollama" => Some(Self::Ollama),
            _ => None,
        }
    }

    pub fn requires_api_key(self) -> bool {
        !matches!(self, Self::Ollama)
    }

    pub fn base_url_required(self) -> bool {
        matches!(self, Self::OpenAiCompat)
    }
}

#[derive(Debug, Clone)]
pub struct ProviderRecord {
    pub id: i64,
    pub name: String,
    pub provider_type: ProviderType,
    pub api_key: String,
    pub base_url: Option<String>,
    pub created_at: SystemTime,
}

#[derive(Debug, Clone)]
pub struct ProviderSettingsSnapshot {
    pub providers: Vec<ProviderRecord>,
    pub default_provider_id: Option<i64>,
    pub default_model: Option<String>,
}

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
            default_provider_id: self.get_setting_i64("default_provider_id")?,
            default_model: self.get_setting("default_model")?,
        })
    }

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
        let base_url = normalize_optional_string(base_url.as_ref().to_string());

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

        let default_provider_id: Option<i64> =
            query_setting_i64(&self.connection, "default_provider_id")?;
        if default_provider_id == Some(provider_id) {
            delete_setting(&self.connection, "default_provider_id")?;
            delete_setting(&self.connection, "default_model")?;
        }

        Ok(())
    }

    pub fn set_default_provider(
        &self,
        provider_id: Option<i64>,
        model: Option<String>,
    ) -> Result<()> {
        match provider_id {
            Some(id) => {
                self.load_provider(id)?;
                set_setting(&self.connection, "default_provider_id", &id.to_string())?;
            }
            None => delete_setting(&self.connection, "default_provider_id")?,
        }

        match model.and_then(normalize_optional_string) {
            Some(model) => set_setting(&self.connection, "default_model", &model)?,
            None => delete_setting(&self.connection, "default_model")?,
        }

        Ok(())
    }

    pub fn default_provider(&self) -> Result<Option<ProviderRecord>> {
        let Some(provider_id) = self.get_setting_i64("default_provider_id")? else {
            return Ok(None);
        };

        self.load_provider(provider_id).map(Some)
    }

    pub fn default_model(&self) -> Result<Option<String>> {
        self.get_setting("default_model")
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

    fn initialize_schema(&mut self) -> Result<()> {
        self.connection
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS providers (
                    id            INTEGER PRIMARY KEY,
                    name          TEXT    NOT NULL,
                    provider_type TEXT    NOT NULL DEFAULT 'openai_compat',
                    api_key       TEXT    NOT NULL,
                    base_url      TEXT    NOT NULL DEFAULT '',
                    created_at    INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS settings (
                    key   TEXT PRIMARY KEY,
                    value TEXT NOT NULL
                );
                ",
            )
            .context("failed to initialize settings schema")?;

        let has_provider_type = self
            .connection
            .prepare("PRAGMA table_info(providers)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .any(|name| name == "provider_type");

        if !has_provider_type {
            // 旧版本只有 OpenAI-compatible provider，迁移时统一标记成 compat，保证旧设置继续可用。
            self.connection
                .execute(
                    "ALTER TABLE providers ADD COLUMN provider_type TEXT NOT NULL DEFAULT 'openai_compat'",
                    [],
                )
                .context("failed to add provider_type column")?;
        }

        Ok(())
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

fn query_setting_i64(connection: &Connection, key: &str) -> Result<Option<i64>> {
    Ok(connection
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .context("failed to read setting")?
        .and_then(|raw| raw.trim().parse::<i64>().ok()))
}

fn normalize_optional_string(raw: String) -> Option<String> {
    let trimmed = raw.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
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
