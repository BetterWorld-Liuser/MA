use anyhow::{Context, Result};

use super::{SettingsStorage, set_setting};

impl SettingsStorage {
    pub(super) fn initialize_schema(&mut self) -> Result<()> {
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

                CREATE TABLE IF NOT EXISTS model_configs (
                    id               INTEGER PRIMARY KEY,
                    provider_id      INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
                    model_id         TEXT    NOT NULL,
                    display_name     TEXT    NOT NULL DEFAULT '',
                    context_window   INTEGER NOT NULL DEFAULT 131072,
                    max_output       INTEGER NOT NULL DEFAULT 4096,
                    supports_tool_use INTEGER NOT NULL DEFAULT 0,
                    supports_vision   INTEGER NOT NULL DEFAULT 0,
                    supports_audio    INTEGER NOT NULL DEFAULT 0,
                    supports_pdf      INTEGER NOT NULL DEFAULT 0,
                    probed_at         INTEGER,
                    UNIQUE(provider_id, model_id)
                );

                CREATE TABLE IF NOT EXISTS model_server_tools (
                    id               INTEGER PRIMARY KEY,
                    model_config_id  INTEGER NOT NULL REFERENCES model_configs(id) ON DELETE CASCADE,
                    capability       TEXT    NOT NULL,
                    format           TEXT    NOT NULL,
                    UNIQUE(model_config_id, capability)
                );

                CREATE TABLE IF NOT EXISTS agent_profiles (
                    id            INTEGER PRIMARY KEY,
                    name          TEXT    NOT NULL UNIQUE,
                    display_name  TEXT    NOT NULL,
                    description   TEXT    NOT NULL DEFAULT '',
                    system_prompt TEXT    NOT NULL,
                    avatar_color  TEXT    NOT NULL DEFAULT '#64748B',
                    model_config_id INTEGER REFERENCES model_configs(id) ON DELETE SET NULL,
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

        self.migrate_legacy_model_storage()?;
        self.migrate_legacy_server_tool_formats()?;
        self.shrink_model_configs_schema()?;
        self.migrate_legacy_default_model_setting()?;
        self.migrate_legacy_agent_model_binding()?;

        Ok(())
    }

    fn migrate_legacy_model_storage(&self) -> Result<()> {
        if !self.table_has_column("model_server_tools", "model_config_id")? {
            self.connection
                .execute(
                    "ALTER TABLE model_server_tools ADD COLUMN model_config_id INTEGER REFERENCES model_configs(id) ON DELETE CASCADE",
                    [],
                )
                .context("failed to add model_server_tools.model_config_id column")?;
        }

        let has_provider_models = self.table_has_column("provider_models", "id")?;
        if has_provider_models {
            self.connection.execute_batch(
                "
                INSERT OR IGNORE INTO model_configs (
                    id, provider_id, model_id, display_name, context_window,
                    max_output, supports_tool_use, supports_vision, supports_audio, supports_pdf, probed_at
                )
                SELECT
                    pm.id,
                    pm.provider_id,
                    pm.model_id,
                    pm.display_name,
                    pm.context_window,
                    pm.max_output,
                    pm.supports_tool_use,
                    pm.supports_vision,
                    pm.supports_audio,
                    pm.supports_pdf,
                    NULL
                FROM provider_models pm
                JOIN providers p ON p.id = pm.provider_id;
                ",
            )?;
        }

        let legacy_server_tools = self.table_has_column("model_server_tools", "provider_id")?
            && self.table_has_column("model_server_tools", "model_id")?;
        if legacy_server_tools {
            self.connection.execute_batch(
                "
                INSERT OR IGNORE INTO model_server_tools (model_config_id, capability, format)
                SELECT
                    mc.id,
                    mst.capability,
                    mst.format
                FROM (
                    SELECT rowid, provider_id, model_id, capability, format
                    FROM model_server_tools
                    WHERE provider_id IS NOT NULL
                ) mst
                JOIN model_configs mc
                  ON mc.provider_id = mst.provider_id
                 AND lower(mc.model_id) = lower(mst.model_id);
                ",
            )?;
        }

        Ok(())
    }

    fn migrate_legacy_server_tool_formats(&self) -> Result<()> {
        self.connection
            .execute_batch(
                "
                UPDATE model_server_tools
                   SET format = CASE
                       WHEN EXISTS (
                           SELECT 1
                             FROM model_configs mc
                             JOIN providers p ON p.id = mc.provider_id
                            WHERE mc.id = model_server_tools.model_config_id
                              AND p.provider_type = 'openai'
                       )
                       THEN 'openai_responses'
                       ELSE 'openai_chat_completions'
                   END
                 WHERE lower(format) = 'openai';
                ",
            )
            .context("failed to migrate legacy openai server tool formats")?;
        Ok(())
    }

    fn migrate_legacy_default_model_setting(&self) -> Result<()> {
        if self.get_setting_i64("default_model_config_id")?.is_some() {
            return Ok(());
        }

        let provider_id = self.get_setting_i64("default_provider_id")?;
        let model_id = self.get_setting("default_model")?;
        let Some(provider_id) = provider_id else {
            return Ok(());
        };
        let Some(model_id) = model_id else {
            return Ok(());
        };
        let Some(model_config) = self.load_model_config_by_model_id(provider_id, &model_id)? else {
            return Ok(());
        };

        set_setting(
            &self.connection,
            "default_model_config_id",
            &model_config.id.to_string(),
        )?;
        Ok(())
    }

    fn shrink_model_configs_schema(&self) -> Result<()> {
        let needs_rebuild = self.table_has_column("model_configs", "wire_format")?
            || self.table_has_column("model_server_tools", "provider_id")?
            || self.table_has_column("model_server_tools", "model_id")?
            || self.table_has_column("agent_profiles", "provider_id")?
            || self.table_has_column("agent_profiles", "model_id")?;
        if !needs_rebuild {
            return Ok(());
        }

        match self
            .connection
            .execute("ALTER TABLE model_configs DROP COLUMN wire_format", [])
        {
            Ok(_) => Ok(()),
            Err(_) => self.rebuild_model_configs_without_wire_format(),
        }
    }

    fn rebuild_model_configs_without_wire_format(&self) -> Result<()> {
        self.connection
            .execute_batch(
                "
                PRAGMA foreign_keys = OFF;
                BEGIN IMMEDIATE;

                ALTER TABLE model_server_tools RENAME TO model_server_tools_legacy;
                ALTER TABLE agent_profiles RENAME TO agent_profiles_legacy;
                ALTER TABLE model_configs RENAME TO model_configs_legacy;

                CREATE TABLE model_configs (
                    id               INTEGER PRIMARY KEY,
                    provider_id      INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
                    model_id         TEXT    NOT NULL,
                    display_name     TEXT    NOT NULL DEFAULT '',
                    context_window   INTEGER NOT NULL DEFAULT 131072,
                    max_output       INTEGER NOT NULL DEFAULT 4096,
                    supports_tool_use INTEGER NOT NULL DEFAULT 0,
                    supports_vision   INTEGER NOT NULL DEFAULT 0,
                    supports_audio    INTEGER NOT NULL DEFAULT 0,
                    supports_pdf      INTEGER NOT NULL DEFAULT 0,
                    probed_at         INTEGER,
                    UNIQUE(provider_id, model_id)
                );

                INSERT INTO model_configs (
                    id, provider_id, model_id, display_name, context_window,
                    max_output, supports_tool_use, supports_vision,
                    supports_audio, supports_pdf, probed_at
                )
                SELECT
                    id, provider_id, model_id, display_name, context_window,
                    max_output, supports_tool_use, supports_vision,
                    supports_audio, supports_pdf, probed_at
                FROM model_configs_legacy;

                  CREATE TABLE model_server_tools (
                      id               INTEGER PRIMARY KEY,
                      model_config_id  INTEGER NOT NULL REFERENCES model_configs(id) ON DELETE CASCADE,
                      capability       TEXT    NOT NULL,
                      format           TEXT    NOT NULL,
                      UNIQUE(model_config_id, capability)
                  );

                  INSERT OR IGNORE INTO model_server_tools (id, model_config_id, capability, format)
                  SELECT
                      mst.id,
                      COALESCE(
                          mst.model_config_id,
                          (
                              SELECT mc.id
                              FROM model_configs mc
                              WHERE mc.provider_id = mst.provider_id
                                AND lower(mc.model_id) = lower(mst.model_id)
                              LIMIT 1
                          )
                      ),
                      mst.capability,
                      mst.format
                  FROM model_server_tools_legacy mst
                  WHERE COALESCE(
                      mst.model_config_id,
                      (
                          SELECT mc.id
                          FROM model_configs mc
                          WHERE mc.provider_id = mst.provider_id
                            AND lower(mc.model_id) = lower(mst.model_id)
                          LIMIT 1
                      )
                  ) IS NOT NULL;

                CREATE TABLE agent_profiles (
                    id            INTEGER PRIMARY KEY,
                    name          TEXT    NOT NULL UNIQUE,
                    display_name  TEXT    NOT NULL,
                    description   TEXT    NOT NULL DEFAULT '',
                    system_prompt TEXT    NOT NULL,
                    avatar_color  TEXT    NOT NULL DEFAULT '#64748B',
                    model_config_id INTEGER REFERENCES model_configs(id) ON DELETE SET NULL,
                    created_at    INTEGER NOT NULL,
                    updated_at    INTEGER NOT NULL
                );

                INSERT INTO agent_profiles (
                    id, name, display_name, description, system_prompt, avatar_color,
                    model_config_id, created_at, updated_at
                )
                SELECT
                    id, name, display_name, description, system_prompt, avatar_color,
                    model_config_id, created_at, updated_at
                FROM agent_profiles_legacy;

                DROP TABLE model_server_tools_legacy;
                DROP TABLE agent_profiles_legacy;
                DROP TABLE model_configs_legacy;

                COMMIT;
                PRAGMA foreign_keys = ON;
                ",
            )
            .context("failed to rebuild model_configs without wire_format column")
    }

    fn migrate_legacy_agent_model_binding(&self) -> Result<()> {
        let has_legacy_provider_id = self.table_has_column("agent_profiles", "provider_id")?;
        let has_legacy_model_id = self.table_has_column("agent_profiles", "model_id")?;
        let has_model_config_id = self.table_has_column("agent_profiles", "model_config_id")?;

        if !has_model_config_id {
            self.connection
                .execute(
                    "ALTER TABLE agent_profiles ADD COLUMN model_config_id INTEGER REFERENCES model_configs(id) ON DELETE SET NULL",
                    [],
                )
                .context("failed to add agent_profiles.model_config_id column")?;
        }

        if has_legacy_provider_id && has_legacy_model_id {
            self.connection.execute_batch(
                "
                UPDATE agent_profiles
                   SET model_config_id = (
                       SELECT mc.id
                         FROM model_configs mc
                        WHERE mc.provider_id = agent_profiles.provider_id
                          AND lower(mc.model_id) = lower(agent_profiles.model_id)
                        LIMIT 1
                   )
                 WHERE model_config_id IS NULL
                   AND provider_id IS NOT NULL
                   AND trim(model_id) <> '';
                ",
            )?;
        }

        Ok(())
    }

    pub(super) fn table_has_column(&self, table: &str, column: &str) -> Result<bool> {
        Ok(self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .any(|name| name == column))
    }
}
