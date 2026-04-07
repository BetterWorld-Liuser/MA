use super::*;
use crate::memory::MemoryManager;

impl UiAppBackend {
    pub fn provider_settings(&self) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        let mut view = UiProviderSettingsView::from_snapshot(
            settings.database_path().to_path_buf(),
            settings.snapshot()?,
        );
        view.agents = load_agent_profiles(&self.workspace_path)?
            .iter()
            .map(UiAgentProfileView::from)
            .collect();
        Ok(view)
    }

    pub fn handle_upsert_agent(
        &self,
        request: UiUpsertAgentRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        let normalized_name = request.name.trim().to_ascii_lowercase();
        if normalized_name == MARCH_AGENT_NAME {
            settings.set_custom_system_core(
                Some(request.system_prompt),
                request.use_custom_march_prompt.unwrap_or(true),
            )?;
        } else {
            settings.upsert_agent_profile(
                normalized_name,
                request.display_name,
                request.description,
                request.system_prompt,
                request.avatar_color.unwrap_or_default(),
                request.provider_id,
                request.model_id,
            )?;
        }
        self.provider_settings()
    }

    pub fn handle_delete_agent(
        &self,
        request: UiDeleteAgentRequest,
    ) -> Result<UiProviderSettingsView> {
        let name = request.name.trim().to_ascii_lowercase();
        if name == MARCH_AGENT_NAME {
            bail!("cannot delete March");
        }
        let mut memories = MemoryManager::load(&self.workspace_path)?;
        memories.reassign_scope_from_agent(&name)?;
        let settings = SettingsStorage::open()?;
        settings.delete_agent_profile(&name)?;
        self.provider_settings()
    }

    pub fn handle_restore_march_prompt(
        &self,
        _request: UiRestoreMarchPromptRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        settings.set_custom_system_core(None, false)?;
        self.provider_settings()
    }

    pub fn handle_upsert_provider(
        &self,
        request: UiUpsertProviderRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        let provider_type =
            ProviderType::from_db_value(&request.provider_type).ok_or_else(|| {
                anyhow::anyhow!("unsupported provider type {}", request.provider_type)
            })?;
        settings.upsert_provider(
            request.id,
            provider_type,
            request.name,
            request.api_key,
            request.base_url,
        )?;
        self.provider_settings()
    }

    pub fn handle_delete_provider(
        &self,
        request: UiDeleteProviderRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        settings.delete_provider(request.provider_id)?;
        self.provider_settings()
    }

    pub fn handle_upsert_provider_model(
        &self,
        request: UiUpsertProviderModelRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        let server_tools = request
            .server_tools
            .into_iter()
            .map(|tool| {
                let capability =
                    ServerToolCapability::from_db_value(&tool.capability).ok_or_else(|| {
                        anyhow::anyhow!("unsupported server tool capability {}", tool.capability)
                    })?;
                let format = ServerToolFormat::from_db_value(&tool.format).ok_or_else(|| {
                    anyhow::anyhow!("unsupported server tool format {}", tool.format)
                })?;
                Ok(ServerToolConfig { capability, format })
            })
            .collect::<Result<Vec<_>>>()?;
        settings.upsert_provider_model(
            request.id,
            request.provider_id,
            request.model_id,
            request.display_name,
            request.context_window,
            request.max_output_tokens,
            request.supports_tool_use,
            request.supports_vision,
            request.supports_audio,
            request.supports_pdf,
            server_tools,
        )?;
        self.provider_settings()
    }

    pub fn handle_delete_provider_model(
        &self,
        request: UiDeleteProviderModelRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        settings.delete_provider_model(request.provider_model_id)?;
        self.provider_settings()
    }

    pub fn handle_set_default_model(
        &mut self,
        request: UiSetDefaultModelRequest,
    ) -> Result<UiProviderSettingsView> {
        let settings = SettingsStorage::open()?;
        let previous = settings.snapshot()?;
        self.storage.backfill_missing_task_defaults(
            previous.default_model_config_id,
            settings.default_model()?,
        )?;
        settings.set_default_model_config(request.model_config_id)?;
        self.provider_settings()
    }
}
