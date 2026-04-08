import { computed, ref, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { TaskModelSelectorView } from '@/data/mock';
import {
  buildCachedTaskModelSelector,
  taskModelSelectorCache,
  type FlatModelItem,
} from './taskModelSelectorShared';

type UseTaskModelCatalogOptions = {
  taskId: Ref<number | null | undefined>;
  selectedModel: Ref<string | undefined>;
  selectedTemperature: Ref<number | undefined>;
  selectedTopP: Ref<number | undefined>;
  selectedPresencePenalty: Ref<number | undefined>;
  selectedFrequencyPenalty: Ref<number | undefined>;
  selectedMaxOutputTokens: Ref<number | undefined>;
};

export function useTaskModelCatalog({
  taskId,
  selectedModel,
  selectedTemperature,
  selectedTopP,
  selectedPresencePenalty,
  selectedFrequencyPenalty,
  selectedMaxOutputTokens,
}: UseTaskModelCatalogOptions) {
  const modelItems = ref<FlatModelItem[]>([]);
  const modelSearchQuery = ref('');
  const modelsLoading = ref(false);
  const modelsRefreshing = ref(false);
  const resolvedCurrentModelConfigId = ref<number | null>(null);
  const resolvedCurrentModel = ref('');
  const resolvedCurrentTemperature = ref<number | null>(null);
  const resolvedCurrentTopP = ref<number | null>(null);
  const resolvedCurrentPresencePenalty = ref<number | null>(null);
  const resolvedCurrentFrequencyPenalty = ref<number | null>(null);
  const resolvedCurrentMaxOutputTokens = ref<number | null>(null);
  const resolvedModelDefaultMaxOutputTokens = ref<number | null>(null);
  const resolvedModelSupportsVision = ref(false);
  let activeModelRequestId = 0;

  const effectiveSelectedModel = computed(() => selectedModel.value?.trim() || resolvedCurrentModel.value.trim());
  const selectedModelDisplayName = computed(() => {
    const active = modelItems.value.find((item) => item.modelConfigId === resolvedCurrentModelConfigId.value);
    return active?.displayName || '';
  });
  const modelButtonLabel = computed(() => selectedModelDisplayName.value || effectiveSelectedModel.value || '选择模型');
  const filteredModelItems = computed(() => {
    const query = modelSearchQuery.value.trim().toLowerCase();
    return modelItems.value.filter((item) => !query
      || item.modelId.toLowerCase().includes(query)
      || item.displayName.toLowerCase().includes(query)
      || item.providerName.toLowerCase().includes(query)
      || item.providerType.toLowerCase().includes(query));
  });
  const supportsVision = computed(() => resolvedModelSupportsVision.value);

  function syncResolvedFromSelectedInputs(modelSettingsOpen: boolean, resetModelSettingsDraft: () => void) {
    resolvedCurrentModel.value = selectedModel.value?.trim() ?? '';
    resolvedCurrentTemperature.value = selectedTemperature.value ?? null;
    resolvedCurrentTopP.value = selectedTopP.value ?? null;
    resolvedCurrentPresencePenalty.value = selectedPresencePenalty.value ?? null;
    resolvedCurrentFrequencyPenalty.value = selectedFrequencyPenalty.value ?? null;
    resolvedCurrentMaxOutputTokens.value = selectedMaxOutputTokens.value ?? null;
    if (!modelSettingsOpen) {
      resetModelSettingsDraft();
    }
  }

  function restoreModelStateFromCache(currentTaskId?: number | null) {
    if (!currentTaskId) {
      resolvedCurrentModelConfigId.value = null;
      resolvedCurrentModel.value = '';
      resolvedModelSupportsVision.value = false;
      modelItems.value = [];
      resolvedCurrentTemperature.value = null;
      resolvedCurrentTopP.value = null;
      resolvedCurrentPresencePenalty.value = null;
      resolvedCurrentFrequencyPenalty.value = null;
      resolvedCurrentMaxOutputTokens.value = null;
      resolvedModelDefaultMaxOutputTokens.value = null;
      return;
    }

    const cached = taskModelSelectorCache.get(currentTaskId);
    if (!cached) {
      return;
    }

    resolvedCurrentModelConfigId.value = cached.currentModelConfigId ?? null;
    resolvedCurrentModel.value = cached.currentModel;
    modelItems.value = cached.models.map((model) => ({ ...model }));
    resolvedCurrentTemperature.value = cached.currentTemperature ?? null;
    resolvedCurrentTopP.value = cached.currentTopP ?? null;
    resolvedCurrentPresencePenalty.value = cached.currentPresencePenalty ?? null;
    resolvedCurrentFrequencyPenalty.value = cached.currentFrequencyPenalty ?? null;
    resolvedCurrentMaxOutputTokens.value = cached.currentMaxOutputTokens ?? null;
    resolvedModelDefaultMaxOutputTokens.value = cached.currentModelDefaultMaxOutputTokens ?? null;
  }

  async function refreshModels(resetModelSettingsDraft: () => void) {
    if (!taskId.value) {
      return;
    }

    const requestId = ++activeModelRequestId;
    const hasWarmData = modelItems.value.length > 0;
    modelsLoading.value = !hasWarmData;
    modelsRefreshing.value = hasWarmData;
    try {
      const response = await invoke<TaskModelSelectorView>('list_provider_models', {
        taskId: taskId.value,
      });
      if (requestId !== activeModelRequestId) {
        return;
      }
      applyModels(response, taskId.value, resetModelSettingsDraft);
    } finally {
      if (requestId === activeModelRequestId) {
        modelsLoading.value = false;
        modelsRefreshing.value = false;
      }
    }
  }

  function applyModels(
    response: TaskModelSelectorView,
    currentTaskId: number,
    resetModelSettingsDraft: () => void,
  ) {
    const cacheEntry = buildCachedTaskModelSelector(response);

    taskModelSelectorCache.set(currentTaskId, cacheEntry);
    resolvedCurrentModelConfigId.value = cacheEntry.currentModelConfigId ?? null;
    resolvedCurrentModel.value = cacheEntry.currentModel;
    resolvedCurrentTemperature.value = cacheEntry.currentTemperature ?? null;
    resolvedCurrentTopP.value = cacheEntry.currentTopP ?? null;
    resolvedCurrentPresencePenalty.value = cacheEntry.currentPresencePenalty ?? null;
    resolvedCurrentFrequencyPenalty.value = cacheEntry.currentFrequencyPenalty ?? null;
    resolvedCurrentMaxOutputTokens.value = cacheEntry.currentMaxOutputTokens ?? null;
    resolvedModelDefaultMaxOutputTokens.value = cacheEntry.currentModelDefaultMaxOutputTokens ?? null;
    resolvedModelSupportsVision.value = response.currentModelCapabilities.supportsVision;
    modelItems.value = cacheEntry.models.map((model) => ({ ...model }));
    resetModelSettingsDraft();
  }

  return {
    modelItems,
    modelSearchQuery,
    modelsLoading,
    modelsRefreshing,
    resolvedCurrentModelConfigId,
    resolvedCurrentModel,
    resolvedCurrentTemperature,
    resolvedCurrentTopP,
    resolvedCurrentPresencePenalty,
    resolvedCurrentFrequencyPenalty,
    resolvedCurrentMaxOutputTokens,
    resolvedModelDefaultMaxOutputTokens,
    effectiveSelectedModel,
    modelButtonLabel,
    filteredModelItems,
    supportsVision,
    syncResolvedFromSelectedInputs,
    restoreModelStateFromCache,
    refreshModels,
  };
}
