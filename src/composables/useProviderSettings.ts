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

  async function testProviderConnection(input: {
    id?: number;
    providerType: string;
    name: string;
    baseUrl: string;
    apiKey: string;
  }) {
    providerTestMessage.value = '';
    providerTestSuccess.value = false;
    let result: ProviderConnectionTestResult | null = null;
    await runWorkspaceAction(async () => {
      result = await invoke<ProviderConnectionTestResult>('test_provider_connection', {
        input,
      });
    });
    if (result) {
      providerTestSuccess.value = result.success;
      providerTestMessage.value = result.message;
    } else {
      providerTestSuccess.value = false;
      providerTestMessage.value = '测试连通性失败';
    }
    return result;
  }

  return {
    settingsOpen,
    providerSettings,
    providerModels,
    providerSuggestedModels,
    providerModelsLoading,
    providerTestMessage,
    providerTestSuccess,
    refreshProviderSettings,
    openSettings,
    closeSettings,
    saveProvider,
    testProviderConnection,
    deleteProvider,
    saveDefaultProvider,
    loadProviderModelsForSettings,
  };
}
