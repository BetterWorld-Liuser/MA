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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerToolCapability {
    WebSearch,
    CodeExecution,
    FileSearch,
}

impl ServerToolCapability {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::WebSearch => "web_search",
            Self::CodeExecution => "code_execution",
            Self::FileSearch => "file_search",
        }
    }

    pub fn from_db_value(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "web_search" => Some(Self::WebSearch),
            "code_execution" => Some(Self::CodeExecution),
            "file_search" => Some(Self::FileSearch),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerToolFormat {
    Anthropic,
    OpenAi,
    Gemini,
}

impl ServerToolFormat {
    pub fn as_db_value(self) -> &'static str {
        match self {
            Self::Anthropic => "anthropic",
            Self::OpenAi => "openai",
            Self::Gemini => "gemini",
        }
    }

    pub fn from_db_value(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "anthropic" => Some(Self::Anthropic),
            "openai" => Some(Self::OpenAi),
            "gemini" => Some(Self::Gemini),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerToolConfig {
    pub capability: ServerToolCapability,
    pub format: ServerToolFormat,
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
pub struct ProviderModelRecord {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub context_window: usize,
    pub max_output_tokens: usize,
    pub supports_tool_use: bool,
    pub supports_vision: bool,
    pub supports_audio: bool,
    pub supports_pdf: bool,
    pub server_tools: Vec<ServerToolConfig>,
}

#[derive(Debug, Clone)]
pub struct ProviderSettingsSnapshot {
    pub providers: Vec<ProviderRecord>,
    pub provider_models: Vec<ProviderModelRecord>,
    pub agent_profiles: Vec<AgentProfileRecord>,
    pub default_provider_id: Option<i64>,
    pub default_model: Option<String>,
    pub custom_system_core: Option<String>,
    pub use_custom_system_core: bool,
}

#[derive(Debug, Clone)]
pub struct AgentProfileRecord {
    pub id: i64,
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub system_prompt: String,
    pub avatar_color: String,
    pub provider_id: Option<i64>,
    pub model_id: Option<String>,
    pub created_at: SystemTime,
    pub updated_at: SystemTime,
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
            provider_models: self.list_provider_models()?,
            agent_profiles: self.list_agent_profiles()?,
            default_provider_id: self.get_setting_i64("default_provider_id")?,
            default_model: self.get_setting("default_model")?,
            custom_system_core: self.get_setting("custom_system_core")?,
            use_custom_system_core: self
                .get_setting("use_custom_system_core")?
                .is_some_and(|value| value == "1"),
        })
    }

    pub fn list_agent_profiles(&self) -> Result<Vec<AgentProfileRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, name, display_name, system_prompt, avatar_color,
                        description, provider_id, model_id, created_at, updated_at
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
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                ))
            })
            .context("failed to query agent profiles")?;

        let mut profiles = Vec::new();
        for row in rows {
            let row = row.context("failed to decode agent profile row")?;
            let description = normalize_agent_description(row.5.clone(), &row.2, &row.3);
            profiles.push(AgentProfileRecord {
                id: row.0,
                name: row.1,
                display_name: row.2,
                system_prompt: row.3,
                avatar_color: row.4,
                description,
                provider_id: row.6,
                model_id: normalize_optional_string(row.7),
                created_at: system_time_from_unix(row.8)?,
                updated_at: system_time_from_unix(row.9)?,
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
                        description, provider_id, model_id, created_at, updated_at
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
                        row.get::<_, String>(7)?,
                        row.get::<_, i64>(8)?,
                        row.get::<_, i64>(9)?,
                    ))
                },
            )
            .optional()
            .context("failed to load agent profile")
            .and_then(|row| {
                row.map(|row| {
                    let description = normalize_agent_description(row.5.clone(), &row.2, &row.3);
                    Ok(AgentProfileRecord {
                        id: row.0,
                        name: row.1,
                        display_name: row.2,
                        system_prompt: row.3,
                        avatar_color: row.4,
                        description,
                        provider_id: row.6,
                        model_id: normalize_optional_string(row.7),
                        created_at: system_time_from_unix(row.8)?,
                        updated_at: system_time_from_unix(row.9)?,
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
        if let Some(provider_id) = provider_id {
            self.load_provider(provider_id)?;
        }
        let model_id = model_id.and_then(normalize_optional_string);
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
                         provider_id = ?6,
                         model_id = ?7,
                         updated_at = ?8
                     WHERE id = ?1",
                    params![
                        existing.id,
                        display_name,
                        description,
                        system_prompt,
                        avatar_color,
                        provider_id,
                        model_id.as_deref().unwrap_or_default(),
                        now_ts,
                    ],
                )
                .context("failed to update agent profile")?;
        } else {
            self.connection
                .execute(
                    "INSERT INTO agent_profiles (
                        name, display_name, description, system_prompt, avatar_color,
                        provider_id, model_id, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        name,
                        display_name,
                        description,
                        system_prompt,
                        avatar_color,
                        provider_id,
                        model_id.as_deref().unwrap_or_default(),
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

    pub fn list_provider_models(&self) -> Result<Vec<ProviderModelRecord>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT id, provider_id, model_id, display_name,
                        context_window, max_output, supports_tool_use,
                        supports_vision, supports_audio, supports_pdf
                 FROM provider_models
                 ORDER BY provider_id ASC, model_id COLLATE NOCASE ASC, id ASC",
            )
            .context("failed to prepare provider model list query")?;

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
                ))
            })
            .context("failed to query provider models")?;

        let mut provider_models = Vec::new();
        for row in rows {
            let row = row.context("failed to decode provider model row")?;
            provider_models.push(ProviderModelRecord {
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
                server_tools: self.load_server_tools_for_model(row.1, &row.2)?,
            });
        }
        Ok(provider_models)
    }

    pub fn list_provider_models_for_provider(
        &self,
        provider_id: i64,
    ) -> Result<Vec<ProviderModelRecord>> {
        self.load_provider(provider_id)?;
        Ok(self
            .list_provider_models()?
            .into_iter()
            .filter(|model| model.provider_id == provider_id)
            .collect())
    }

    pub fn load_provider_model_by_model_id(
        &self,
        provider_id: i64,
        model_id: &str,
    ) -> Result<Option<ProviderModelRecord>> {
        let normalized_model = model_id.trim();
        if normalized_model.is_empty() {
            return Ok(None);
        }

        self.connection
            .query_row(
                "SELECT id, provider_id, model_id, display_name,
                        context_window, max_output, supports_tool_use,
                        supports_vision, supports_audio, supports_pdf
                 FROM provider_models
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
                    ))
                },
            )
            .optional()
            .context("failed to load provider model")
            .and_then(|row| {
                row.map(|row| {
                    Ok(ProviderModelRecord {
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
                        server_tools: self.load_server_tools_for_model(row.1, &row.2)?,
                    })
                })
                .transpose()
            })
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
    ) -> Result<ProviderModelRecord> {
        self.load_provider(provider_id)?;
        let model_id = model_id.as_ref().trim();
        if model_id.is_empty() {
            bail!("provider model_id cannot be empty");
        }
        if context_window == 0 {
            bail!("provider model context_window must be greater than 0");
        }
        if max_output_tokens == 0 {
            bail!("provider model max_output_tokens must be greater than 0");
        }
        let display_name = normalize_optional_string(display_name.as_ref().to_string());

        match provider_model_id {
            Some(id) => {
                let existing = self.load_provider_model(id)?;
                let affected = self
                    .connection
                    .execute(
                        "UPDATE provider_models
                         SET provider_id = ?2,
                             model_id = ?3,
                             display_name = ?4,
                             context_window = ?5,
                             max_output = ?6,
                             supports_tool_use = ?7,
                             supports_vision = ?8,
                             supports_audio = ?9,
                             supports_pdf = ?10
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
                        ],
                    )
                    .context("failed to update provider model")?;
                if affected == 0 {
                    bail!("provider model {} not found", id);
                }
                if existing.provider_id != provider_id || existing.model_id != model_id {
                    self.delete_server_tools_for_model(existing.provider_id, &existing.model_id)?;
                }
                self.sync_server_tools_for_model(provider_id, model_id, &server_tools)?;
                self.load_provider_model(id)
            }
            None => {
                self.connection
                    .execute(
                        "INSERT INTO provider_models (
                            provider_id, model_id, display_name, context_window,
                            max_output, supports_tool_use, supports_vision,
                            supports_audio, supports_pdf
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
                        ],
                    )
                    .context("failed to insert provider model")?;
                let id = self.connection.last_insert_rowid();
                self.sync_server_tools_for_model(provider_id, model_id, &server_tools)?;
                self.load_provider_model(id)
            }
        }
    }

    pub fn delete_provider_model(&self, provider_model_id: i64) -> Result<()> {
        let existing = self.load_provider_model(provider_model_id)?;
        self.delete_server_tools_for_model(existing.provider_id, &existing.model_id)?;
        let affected = self
            .connection
            .execute(
                "DELETE FROM provider_models WHERE id = ?1",
                params![provider_model_id],
            )
            .context("failed to delete provider model")?;
        if affected == 0 {
            bail!("provider model {} not found", provider_model_id);
        }
        Ok(())
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

    pub fn load_provider_model(&self, provider_model_id: i64) -> Result<ProviderModelRecord> {
        let row = self
            .connection
            .query_row(
                "SELECT id, provider_id, model_id, display_name,
                        context_window, max_output, supports_tool_use,
                        supports_vision, supports_audio, supports_pdf
                 FROM provider_models
                 WHERE id = ?1",
                params![provider_model_id],
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
                    ))
                },
            )
            .optional()
            .context("failed to load provider model")?
            .ok_or_else(|| anyhow::anyhow!("provider model {} not found", provider_model_id))?;

        Ok(ProviderModelRecord {
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
            server_tools: self.load_server_tools_for_model(row.1, &row.2)?,
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

                CREATE TABLE IF NOT EXISTS provider_models (
                    id               INTEGER PRIMARY KEY,
                    provider_id      INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
                    model_id         TEXT    NOT NULL,
                    display_name     TEXT    NOT NULL DEFAULT '',
                    context_window   INTEGER NOT NULL DEFAULT 131072,
                    max_output       INTEGER NOT NULL DEFAULT 4096,
                    supports_tool_use INTEGER NOT NULL DEFAULT 0,
                    supports_vision   INTEGER NOT NULL DEFAULT 0,
                    supports_audio    INTEGER NOT NULL DEFAULT 0,
                    supports_pdf      INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS model_server_tools (
                    id            INTEGER PRIMARY KEY,
                    provider_id   INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
                    model_id      TEXT    NOT NULL,
                    capability    TEXT    NOT NULL,
                    format        TEXT    NOT NULL,
                    UNIQUE(provider_id, model_id, capability)
                );

                CREATE TABLE IF NOT EXISTS agent_profiles (
                    id            INTEGER PRIMARY KEY,
                    name          TEXT    NOT NULL UNIQUE,
                    display_name  TEXT    NOT NULL,
                    description   TEXT    NOT NULL DEFAULT '',
                    system_prompt TEXT    NOT NULL,
                    avatar_color  TEXT    NOT NULL DEFAULT '#64748B',
                    provider_id   INTEGER REFERENCES providers(id) ON DELETE SET NULL,
                    model_id      TEXT    NOT NULL DEFAULT '',
                    created_at    INTEGER NOT NULL,
                    updated_at    INTEGER NOT NULL
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

        let has_agent_description = self
            .connection
            .prepare("PRAGMA table_info(agent_profiles)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .any(|name| name == "description");

        if !has_agent_description {
            self.connection
                .execute(
                    "ALTER TABLE agent_profiles ADD COLUMN description TEXT NOT NULL DEFAULT ''",
                    [],
                )
                .context("failed to add agent_profiles.description column")?;
        }

        Ok(())
    }

    fn load_server_tools_for_model(
        &self,
        provider_id: i64,
        model_id: &str,
    ) -> Result<Vec<ServerToolConfig>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT capability, format
                 FROM model_server_tools
                 WHERE provider_id = ?1 AND lower(model_id) = lower(?2)
                 ORDER BY id ASC",
            )
            .context("failed to prepare model server tools query")?;

        let rows = statement
            .query_map(params![provider_id, model_id], |row| {
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

    fn delete_server_tools_for_model(&self, provider_id: i64, model_id: &str) -> Result<()> {
        self.connection
            .execute(
                "DELETE FROM model_server_tools
                 WHERE provider_id = ?1 AND lower(model_id) = lower(?2)",
                params![provider_id, model_id],
            )
            .context("failed to delete model server tools")?;
        Ok(())
    }

    fn sync_server_tools_for_model(
        &self,
        provider_id: i64,
        model_id: &str,
        server_tools: &[ServerToolConfig],
    ) -> Result<()> {
        self.delete_server_tools_for_model(provider_id, model_id)?;
        for tool in server_tools {
            self.connection
                .execute(
                    "INSERT INTO model_server_tools (provider_id, model_id, capability, format)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![
                        provider_id,
                        model_id,
                        tool.capability.as_db_value(),
                        tool.format.as_db_value(),
                    ],
                )
                .context("failed to insert model server tool")?;
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

fn normalize_agent_name(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace(' ', "-")
}

fn normalize_avatar_color(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "#64748B".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_agent_description(raw: String, display_name: &str, system_prompt: &str) -> String {
    let trimmed = raw.trim().to_string();
    if !trimmed.is_empty() {
        return trimmed;
    }

    system_prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.chars().take(60).collect::<String>())
        .filter(|line| !line.trim().is_empty())
        .unwrap_or_else(|| format!("负责 {} 相关工作。", display_name))
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
