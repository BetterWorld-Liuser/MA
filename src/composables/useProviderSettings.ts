import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  ProviderConnectionTestResult,
  ProviderSettingsView,
  ProviderModelsView,
} from '@/data/mock';
import { normalizeProviderBaseUrl } from '@/lib/providerBaseUrl';

type UseProviderSettingsOptions = {
  runWorkspaceAction: (action: () => Promise<void>) => Promise<boolean>;
  humanizeError: (error: unknown) => string;
};

export function useProviderSettings({
  runWorkspaceAction,
  humanizeError,
}: UseProviderSettingsOptions) {
  const probeModelCache = new Map<string, ProviderModelsView>();
  const settingsOpen = ref(false);
  const providerSettings = ref<ProviderSettingsView | null>(null);
  const providerProbeModels = ref<string[]>([]);
  const providerProbeSuggestedModels = ref<string[]>([]);
  const providerProbeModelsLoading = ref(false);
  const providerTestLoading = ref(false);
  const providerTestMessage = ref('');
  const providerTestSuccess = ref(false);

  async function refreshProviderSettings() {
    try {
      providerSettings.value = await invoke<ProviderSettingsView>('load_provider_settings');
    } catch (error) {
      console.warn('Failed to load provider settings.', error);
    }
  }

  async function openSettings() {
    settingsOpen.value = true;
    // Open the shell first so a slow or stalled backend refresh does not make
    // the settings entry look unclickable.
    await refreshProviderSettings();
  }

  function closeSettings() {
    settingsOpen.value = false;
  }

  function buildProbeModelsCacheKey(input: {
    id?: number;
    providerType: string;
    baseUrl: string;
    apiKey: string;
  }) {
    return JSON.stringify({
      id: input.id ?? null,
      providerType: input.providerType.trim(),
      baseUrl: normalizeProviderBaseUrl(input.providerType, input.baseUrl),
      apiKey: input.apiKey.trim(),
    });
  }

  function normalizeProviderInput<T extends { providerType: string; baseUrl: string }>(input: T): T {
    return {
      ...input,
      baseUrl: normalizeProviderBaseUrl(input.providerType, input.baseUrl),
    };
  }

  function applyProbeModelResponse(response: ProviderModelsView) {
    providerProbeModels.value = response.available_models;
    providerProbeSuggestedModels.value = response.suggested_models;
  }

  function invalidateProbeModelsCache() {
    probeModelCache.clear();
  }

  async function saveProvider(input: { id?: number; providerType: string; name: string; baseUrl: string; apiKey: string }) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('upsert_provider', {
        input: normalizeProviderInput(input),
      });
    });
    invalidateProbeModelsCache();
  }

  async function deleteProvider(providerId: number) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('delete_provider', {
        input: { providerId },
      });
    });
    invalidateProbeModelsCache();
  }

  async function saveDefaultModel(input: { modelConfigId?: number | null }) {
    await runWorkspaceAction(async () => {
      providerSettings.value = await invoke<ProviderSettingsView>('set_default_model', {
        input: {
          modelConfigId: input.modelConfigId ?? null,
        },
      });
    });
  }

  async function loadProbeModels(input: {
    id?: number;
    providerType: string;
    baseUrl: string;
    apiKey: string;
    probeModel?: string;
  }, options?: {
    forceRefresh?: boolean;
  }) {
    const normalizedInput = normalizeProviderInput(input);
    const cacheKey = buildProbeModelsCacheKey(normalizedInput);
    if (!options?.forceRefresh) {
      const cachedResponse = probeModelCache.get(cacheKey);
      if (cachedResponse) {
        applyProbeModelResponse(cachedResponse);
        return cachedResponse;
      }
    }

    providerProbeModelsLoading.value = true;
    try {
      const response = await invoke<ProviderModelsView>('list_probe_models', {
        input: normalizedInput,
      });
      probeModelCache.set(cacheKey, response);
      applyProbeModelResponse(response);
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
    providerTestLoading.value = true;
    providerTestMessage.value = '';
    providerTestSuccess.value = false;
    const normalizedInput = normalizeProviderInput(input);
    const resultHolder: { value?: ProviderConnectionTestResult } = {};
    let failureMessage = '';
    try {
      await runWorkspaceAction(async () => {
        try {
          resultHolder.value = await invoke<ProviderConnectionTestResult>('test_provider_connection', {
            input: normalizedInput,
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
    } finally {
      providerTestLoading.value = false;
    }
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
    serverTools: Array<{
      capability: string;
      format: string;
    }>;
  }) {
    await runWorkspaceAction(async () => {
      // Use the command response directly so "save" is a single write+refresh hop.
      // A second load call makes this path uniquely fragile compared with other settings saves.
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
    providerProbeModels,
    providerProbeSuggestedModels,
    providerProbeModelsLoading,
    providerTestLoading,
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
    saveDefaultModel,
    loadProbeModels,
    invalidateProbeModelsCache,
  };
}
