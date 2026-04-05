import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { ProviderConnectionTestResult, ProviderModelsView, ProviderSettingsView } from '@/data/mock';

type UseProviderSettingsOptions = {
  runWorkspaceAction: (action: () => Promise<void>) => Promise<void>;
  setErrorMessage: (message: string) => void;
  humanizeError: (error: unknown) => string;
};

export function useProviderSettings({
  runWorkspaceAction,
  setErrorMessage,
  humanizeError,
}: UseProviderSettingsOptions) {
  const settingsOpen = ref(false);
  const providerSettings = ref<ProviderSettingsView | null>(null);
  const providerModels = ref<string[]>([]);
  const providerSuggestedModels = ref<string[]>([]);
  const providerModelsLoading = ref(false);
  const providerProbeModels = ref<string[]>([]);
  const providerProbeSuggestedModels = ref<string[]>([]);
  const providerProbeModelsLoading = ref(false);
  const providerTestMessage = ref('');
  const providerTestSuccess = ref(false);

  async function refreshProviderSettings() {
    try {
      providerSettings.value = await invoke<ProviderSettingsView>('load_provider_settings');
      if (providerSettings.value?.defaultProviderId) {
        await loadProviderModelsForSettings(providerSettings.value.defaultProviderId);
      } else {
        providerModels.value = [];
        providerSuggestedModels.value = [];
      }
    } catch (error) {
      console.warn('Failed to load provider settings.', error);
    }
  }

  async function openSettings() {
    settingsOpen.value = true;
    await refreshProviderSettings();
  }

  function closeSettings() {
    settingsOpen.value = false;
  }

  async function saveProvider(input: { id?: number; providerType: string; name: string; baseUrl: string; apiKey: string }) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('upsert_provider', {
        input,
      });
    });

    if (providerSettings.value?.defaultProviderId) {
      await loadProviderModelsForSettings(providerSettings.value.defaultProviderId);
    }
  }

  async function deleteProvider(providerId: number) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('delete_provider', {
        input: { providerId },
      });
    });

    if (providerSettings.value?.defaultProviderId) {
      await loadProviderModelsForSettings(providerSettings.value.defaultProviderId);
      return;
    }
    providerModels.value = [];
    providerSuggestedModels.value = [];
  }

  async function saveDefaultProvider(input: { providerId: number; model: string }) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('set_default_provider', {
        input: {
          providerId: input.providerId,
          model: input.model,
        },
      });
    });
    await loadProviderModelsForSettings(input.providerId);
  }

  async function loadProviderModelsForSettings(providerId: number) {
    providerModelsLoading.value = true;
    try {
      const response = await invoke<ProviderModelsView>('list_provider_models_for_settings', {
        providerId,
      });
      providerModels.value = response.available_models;
      providerSuggestedModels.value = response.suggested_models;
    } catch (error) {
      providerModels.value = [];
      providerSuggestedModels.value = [];
      setErrorMessage(humanizeError(error));
    } finally {
      providerModelsLoading.value = false;
    }
  }

  async function loadProbeModels(input: {
    id?: number;
    providerType: string;
    baseUrl: string;
    apiKey: string;
    probeModel?: string;
  }) {
    providerProbeModelsLoading.value = true;
    providerProbeModels.value = [];
    providerProbeSuggestedModels.value = [];
    try {
      const response = await invoke<ProviderModelsView>('list_probe_models', {
        input,
      });
      providerProbeModels.value = response.available_models;
      providerProbeSuggestedModels.value = response.suggested_models;
      return response;
    } catch (error) {
      providerProbeModels.value = [];
      providerProbeSuggestedModels.value = [];
      throw error;
    } finally {
      providerProbeModelsLoading.value = false;
    }
  }

  async function testProviderConnection(input: {
    id?: number;
    providerType: string;
    name: string;
    baseUrl: string;
    apiKey: string;
    probeModel?: string;
  }) {
    providerTestMessage.value = '';
    providerTestSuccess.value = false;
    const resultHolder: { value?: ProviderConnectionTestResult } = {};
    let failureMessage = '';
    await runWorkspaceAction(async () => {
      try {
        resultHolder.value = await invoke<ProviderConnectionTestResult>('test_provider_connection', {
          input,
        });
      } catch (error) {
        failureMessage = humanizeError(error);
        throw error;
      }
    });
    const result = resultHolder.value;
    if (result) {
      providerTestSuccess.value = result.success;
      providerTestMessage.value = result.message;
    } else {
      providerTestSuccess.value = false;
      providerTestMessage.value = failureMessage || '测试连通性失败，请查看顶部错误信息。';
    }
    return result;
  }

  async function saveProviderModel(input: {
    id?: number;
    providerId: number;
    modelId: string;
    displayName: string;
    contextWindow: number;
    maxOutputTokens: number;
    supportsToolUse: boolean;
    supportsVision: boolean;
    supportsAudio: boolean;
    supportsPdf: boolean;
  }) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('upsert_provider_model', {
        input,
      });
    });
  }

  async function deleteProviderModel(providerModelId: number) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('delete_provider_model', {
        input: { providerModelId },
      });
    });
  }

  async function saveAgent(input: {
    name: string;
    displayName: string;
    description: string;
    systemPrompt: string;
    avatarColor?: string;
    providerId?: number | null;
    modelId?: string | null;
    useCustomMarchPrompt?: boolean;
  }) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('upsert_agent', {
        input,
      });
    });
  }

  async function deleteAgent(name: string) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('delete_agent', {
        input: { name },
      });
    });
  }

  async function restoreMarchPrompt() {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('restore_march_prompt', {
        input: {},
      });
    });
  }

  return {
    settingsOpen,
    providerSettings,
    providerModels,
    providerSuggestedModels,
    providerModelsLoading,
    providerProbeModels,
    providerProbeSuggestedModels,
    providerProbeModelsLoading,
    providerTestMessage,
    providerTestSuccess,
    refreshProviderSettings,
    openSettings,
    closeSettings,
    saveProvider,
    testProviderConnection,
    deleteProvider,
    saveProviderModel,
    deleteProviderModel,
    saveAgent,
    deleteAgent,
    restoreMarchPrompt,
    saveDefaultProvider,
    loadProviderModelsForSettings,
    loadProbeModels,
  };
}
