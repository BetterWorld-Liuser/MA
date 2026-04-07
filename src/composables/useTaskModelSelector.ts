import { computed, nextTick, onMounted, onUnmounted, ref, watch, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { open as openPathDialog } from '@tauri-apps/plugin-dialog';
import type { TaskModelSelectorView } from '@/data/mock';

type CachedProviderGroup = {
  providerId?: number | null;
  providerName: string;
  providerType: string;
  providerCacheKey: string;
  availableModels: string[];
};

type CachedTaskModelSelector = {
  currentProviderId?: number | null;
  currentModel: string;
  currentTemperature?: number | null;
  currentTopP?: number | null;
  currentPresencePenalty?: number | null;
  currentFrequencyPenalty?: number | null;
  currentMaxOutputTokens?: number | null;
  currentModelDefaultMaxOutputTokens?: number | null;
  providers: CachedProviderGroup[];
};

type UseTaskModelSelectorOptions = {
  taskId: Ref<number | null | undefined>;
  disabled: Ref<boolean>;
  settingsOpen: Ref<boolean>;
  selectedModel: Ref<string | undefined>;
  selectedTemperature: Ref<number | undefined>;
  selectedTopP: Ref<number | undefined>;
  selectedPresencePenalty: Ref<number | undefined>;
  selectedFrequencyPenalty: Ref<number | undefined>;
  selectedMaxOutputTokens: Ref<number | undefined>;
  workingDirectory: Ref<string | undefined>;
  workspacePath: Ref<string | undefined>;
  plusMenuOpen: Ref<boolean>;
  closeComposerMenus: () => void;
  emitSetModel: (selection: { providerId?: number | null; model: string }) => void;
  emitSetModelSettings: (settings: {
    temperature?: number | null;
    topP?: number | null;
    presencePenalty?: number | null;
    frequencyPenalty?: number | null;
    maxOutputTokens?: number | null;
  }) => void;
  emitSetWorkingDirectory: (path?: string | null) => void;
};

// 模型列表读取仍然可能依赖 provider 网络请求。
// 这里保留一个前端进程内缓存，让菜单可以先秒开最近一次成功结果，再异步刷新。
const taskModelSelectorCache = new Map<number, CachedTaskModelSelector>();

export function useTaskModelSelector({
  taskId,
  disabled,
  settingsOpen,
  selectedModel,
  selectedTemperature,
  selectedTopP,
  selectedPresencePenalty,
  selectedFrequencyPenalty,
  selectedMaxOutputTokens,
  workingDirectory,
  workspacePath,
  plusMenuOpen,
  closeComposerMenus,
  emitSetModel,
  emitSetModelSettings,
  emitSetWorkingDirectory,
}: UseTaskModelSelectorOptions) {
  const modelMenuAnchorRef = ref<HTMLElement | null>(null);
  const modelMenuPanelRef = ref<HTMLElement | null>(null);
  const modelSearchRef = ref<HTMLInputElement | null>(null);
  const modelMenuOpen = ref(false);
  const modelSettingsAnchorRef = ref<HTMLElement | null>(null);
  const modelSettingsPanelRef = ref<HTMLElement | null>(null);
  const modelSettingsOpen = ref(false);
  const providerGroups = ref<CachedProviderGroup[]>([]);
  const modelSearchQuery = ref('');
  const modelsLoading = ref(false);
  const modelsRefreshing = ref(false);
  const resolvedCurrentProviderId = ref<number | null>(null);
  const resolvedCurrentModel = ref('');
  const resolvedCurrentTemperature = ref<number | null>(null);
  const resolvedCurrentTopP = ref<number | null>(null);
  const resolvedCurrentPresencePenalty = ref<number | null>(null);
  const resolvedCurrentFrequencyPenalty = ref<number | null>(null);
  const resolvedCurrentMaxOutputTokens = ref<number | null>(null);
  const resolvedModelDefaultMaxOutputTokens = ref<number | null>(null);
  const resolvedModelSupportsVision = ref(false);
  const modelMenuStyle = ref<Record<string, string>>({});
  const modelSettingsStyle = ref<Record<string, string>>({});
  const temperatureDraft = ref('');
  const topPDraft = ref('');
  const presencePenaltyDraft = ref('');
  const frequencyPenaltyDraft = ref('');
  const maxOutputTokensDraft = ref('');
  const modelSettingsError = ref('');
  let activeModelRequestId = 0;

  const supportsVision = computed(() => resolvedModelSupportsVision.value);
  const effectiveSelectedModel = computed(() => selectedModel.value?.trim() || resolvedCurrentModel.value.trim());
  const modelButtonLabel = computed(() => effectiveSelectedModel.value || '选择模型');
  const maxOutputTokensPlaceholder = computed(() =>
    resolvedModelDefaultMaxOutputTokens.value
      ? `留空则使用默认值 ${resolvedModelDefaultMaxOutputTokens.value}`
      : '留空则使用模型默认值',
  );
  const normalizedWorkspacePath = computed(() => normalizePath(workspacePath.value));
  const normalizedWorkingDirectory = computed(() => normalizePath(workingDirectory.value));
  const isCustomWorkingDirectory = computed(
    () =>
      !!normalizedWorkingDirectory.value
      && !!normalizedWorkspacePath.value
      && normalizedWorkingDirectory.value !== normalizedWorkspacePath.value,
  );
  const workingDirectoryLabel = computed(() => normalizedWorkingDirectory.value || '工作目录');
  const workingDirectoryTooltip = computed(() =>
    normalizedWorkingDirectory.value
      ? `AI 工作目录：${normalizedWorkingDirectory.value}`
      : '设置 AI 工作目录',
  );
  const filteredProviderGroups = computed(() => {
    const query = modelSearchQuery.value.trim().toLowerCase();
    return providerGroups.value
      .map((group) => ({
        ...group,
        filteredModels: !query
          ? group.availableModels
          : group.availableModels.filter((model) => model.toLowerCase().includes(query)),
      }))
      .filter((group) => group.filteredModels.length > 0);
  });

  watch(taskId, (nextTaskId) => {
    closeAllMenus();
    restoreModelStateFromCache(nextTaskId);
    seedModelListFromCurrentSelection();
    resetModelSettingsDraft();
    void refreshModels();
  });

  watch(selectedModel, (model) => {
    resolvedCurrentModel.value = model?.trim() ?? '';
    seedModelListFromCurrentSelection();
  }, { immediate: true });

  watch(selectedTemperature, (value) => {
    resolvedCurrentTemperature.value = value ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  }, { immediate: true });

  watch(selectedTopP, (value) => {
    resolvedCurrentTopP.value = value ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  }, { immediate: true });

  watch(selectedPresencePenalty, (value) => {
    resolvedCurrentPresencePenalty.value = value ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  }, { immediate: true });

  watch(selectedFrequencyPenalty, (value) => {
    resolvedCurrentFrequencyPenalty.value = value ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  }, { immediate: true });

  watch(selectedMaxOutputTokens, (value) => {
    resolvedCurrentMaxOutputTokens.value = value ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  }, { immediate: true });

  watch(settingsOpen, (open) => {
    if (open) {
      closeAllMenus();
    }
  });

  watch([modelMenuOpen, filteredProviderGroups, modelSearchQuery], async ([open]) => {
    if (!open) {
      return;
    }
    await nextTick();
    syncModelMenuPosition();
  });

  watch(modelSettingsOpen, async (open) => {
    if (!open) {
      return;
    }
    await nextTick();
    syncModelSettingsMenuPosition();
  });

  onMounted(() => {
    document.addEventListener('mousedown', handleModelMenuPointerDown);
    window.addEventListener('resize', syncFloatingMenus);
    window.addEventListener('scroll', syncFloatingMenus, true);
    restoreModelStateFromCache(taskId.value);
    seedModelListFromCurrentSelection();
    resetModelSettingsDraft();
    void refreshModels();
  });

  onUnmounted(() => {
    document.removeEventListener('mousedown', handleModelMenuPointerDown);
    window.removeEventListener('resize', syncFloatingMenus);
    window.removeEventListener('scroll', syncFloatingMenus, true);
  });

  async function toggleModelMenu() {
    if (!modelMenuOpen.value) {
      primeModelMenu();
      plusMenuOpen.value = false;
      modelMenuOpen.value = true;
      modelSearchQuery.value = '';
      await nextTick();
      syncModelMenuPosition();
      modelSearchRef.value?.focus();
      return;
    }
    closeModelMenu();
  }

  async function toggleModelSettingsMenu() {
    if (!modelSettingsOpen.value) {
      primeModelMenu();
      closeModelMenu();
      resetModelSettingsDraft();
      modelSettingsOpen.value = true;
      await nextTick();
      syncModelSettingsMenuPosition();
      return;
    }
    closeModelSettingsMenu();
  }

  async function pickWorkingDirectory() {
    const selected = await openPathDialog({
      directory: true,
      multiple: false,
      defaultPath: workingDirectory.value || workspacePath.value,
      title: '选择 AI 工作目录',
    });
    if (!selected || Array.isArray(selected)) {
      return;
    }
    emitSetWorkingDirectory(selected);
  }

  function resetWorkingDirectory() {
    emitSetWorkingDirectory(null);
  }

  function selectModel(providerId: number | null | undefined, model: string) {
    resolvedCurrentProviderId.value = providerId ?? null;
    resolvedCurrentModel.value = model;
    const activeGroup = providerGroups.value.find((group) => group.providerId === (providerId ?? null));
    if (activeGroup && !activeGroup.availableModels.includes(model)) {
      activeGroup.availableModels = [model, ...activeGroup.availableModels];
    }
    emitSetModel({ providerId, model });
    closeModelMenu();
  }

  function isModelActive(providerId: number | null | undefined, model: string) {
    return (providerId ?? null) === resolvedCurrentProviderId.value && model === effectiveSelectedModel.value;
  }

  function applyModelSettings() {
    const parsedTemperature = parseOptionalNumber(temperatureDraft.value);
    const parsedTopP = parseOptionalNumber(topPDraft.value);
    const parsedPresencePenalty = parseOptionalNumber(presencePenaltyDraft.value);
    const parsedFrequencyPenalty = parseOptionalNumber(frequencyPenaltyDraft.value);
    const parsedMaxOutputTokens = parseOptionalInteger(maxOutputTokensDraft.value);

    if (parsedTemperature !== null && (parsedTemperature < 0 || parsedTemperature > 2)) {
      modelSettingsError.value = 'Temperature 需要在 0 到 2 之间。';
      return;
    }

    if (parsedMaxOutputTokens !== null && parsedMaxOutputTokens < 1) {
      modelSettingsError.value = 'Max output tokens 需要大于 0。';
      return;
    }

    if (parsedTopP !== null && (parsedTopP < 0 || parsedTopP > 1)) {
      modelSettingsError.value = 'Top P 需要在 0 到 1 之间。';
      return;
    }

    if (parsedPresencePenalty !== null && (parsedPresencePenalty < -2 || parsedPresencePenalty > 2)) {
      modelSettingsError.value = 'Presence penalty 需要在 -2 到 2 之间。';
      return;
    }

    if (parsedFrequencyPenalty !== null && (parsedFrequencyPenalty < -2 || parsedFrequencyPenalty > 2)) {
      modelSettingsError.value = 'Frequency penalty 需要在 -2 到 2 之间。';
      return;
    }

    modelSettingsError.value = '';
    emitSetModelSettings({
      temperature: parsedTemperature,
      topP: parsedTopP,
      presencePenalty: parsedPresencePenalty,
      frequencyPenalty: parsedFrequencyPenalty,
      maxOutputTokens: parsedMaxOutputTokens,
    });
    closeModelSettingsMenu();
  }

  function closeModelMenu() {
    modelSearchQuery.value = '';
    modelMenuOpen.value = false;
  }

  function closeModelSettingsMenu() {
    modelSettingsError.value = '';
    modelSettingsOpen.value = false;
  }

  function closeAllMenus() {
    closeComposerMenus();
    closeModelMenu();
    closeModelSettingsMenu();
  }

  function resetModelSettingsDraft() {
    temperatureDraft.value = resolvedCurrentTemperature.value == null ? '' : String(resolvedCurrentTemperature.value);
    topPDraft.value = resolvedCurrentTopP.value == null ? '' : String(resolvedCurrentTopP.value);
    presencePenaltyDraft.value = resolvedCurrentPresencePenalty.value == null ? '' : String(resolvedCurrentPresencePenalty.value);
    frequencyPenaltyDraft.value = resolvedCurrentFrequencyPenalty.value == null ? '' : String(resolvedCurrentFrequencyPenalty.value);
    maxOutputTokensDraft.value = resolvedCurrentMaxOutputTokens.value == null ? '' : String(resolvedCurrentMaxOutputTokens.value);
    modelSettingsError.value = '';
  }

  function providerTypeLabel(providerType: string) {
    const labels: Record<string, string> = {
      anthropic: 'Anthropic',
      openai: 'OpenAI',
      gemini: 'Gemini',
      openai_compat: 'OpenAI 兼容',
      ollama: 'Ollama',
      env: '环境',
    };
    return labels[providerType] ?? providerType;
  }

  function handleModelMenuPointerDown(event: MouseEvent) {
    if (!modelMenuOpen.value && !modelSettingsOpen.value) {
      return;
    }

    const target = event.target as Node | null;
    if (!target) {
      return;
    }

    const clickedAnchor = modelMenuAnchorRef.value?.contains(target);
    const clickedPanel = modelMenuPanelRef.value?.contains(target);
    const clickedSettingsAnchor = modelSettingsAnchorRef.value?.contains(target);
    const clickedSettingsPanel = modelSettingsPanelRef.value?.contains(target);
    if (!clickedAnchor && !clickedPanel) {
      closeModelMenu();
    }
    if (!clickedSettingsAnchor && !clickedSettingsPanel) {
      closeModelSettingsMenu();
    }
  }

  return {
    disabled,
    modelMenuAnchorRef,
    modelMenuPanelRef,
    modelSearchRef,
    modelMenuOpen,
    modelSettingsAnchorRef,
    modelSettingsPanelRef,
    modelSettingsOpen,
    providerGroups,
    modelSearchQuery,
    modelsLoading,
    modelsRefreshing,
    modelMenuStyle,
    modelSettingsStyle,
    supportsVision,
    effectiveSelectedModel,
    modelButtonLabel,
    maxOutputTokensPlaceholder,
    isCustomWorkingDirectory,
    workingDirectoryLabel,
    workingDirectoryTooltip,
    filteredProviderGroups,
    temperatureDraft,
    topPDraft,
    presencePenaltyDraft,
    frequencyPenaltyDraft,
    maxOutputTokensDraft,
    modelSettingsError,
    toggleModelMenu,
    toggleModelSettingsMenu,
    pickWorkingDirectory,
    resetWorkingDirectory,
    selectModel,
    isModelActive,
    applyModelSettings,
    resetModelSettingsDraft,
    providerTypeLabel,
    closeAllMenus,
  };

  function primeModelMenu() {
    restoreModelStateFromCache(taskId.value);
    seedModelListFromCurrentSelection();
    void refreshModels();
  }

  function restoreModelStateFromCache(currentTaskId?: number | null) {
    if (!currentTaskId) {
      resolvedCurrentProviderId.value = null;
      resolvedCurrentModel.value = '';
      resolvedModelSupportsVision.value = false;
      providerGroups.value = [];
      resolvedCurrentTopP.value = null;
      resolvedCurrentPresencePenalty.value = null;
      resolvedCurrentFrequencyPenalty.value = null;
      return;
    }

    const cached = taskModelSelectorCache.get(currentTaskId);
    if (!cached) {
      return;
    }

    resolvedCurrentProviderId.value = cached.currentProviderId ?? null;
    resolvedCurrentModel.value = cached.currentModel;
    providerGroups.value = cached.providers.map((group) => ({
      ...group,
      availableModels: [...group.availableModels],
    }));
    resolvedCurrentTemperature.value = cached.currentTemperature ?? null;
    resolvedCurrentTopP.value = cached.currentTopP ?? null;
    resolvedCurrentPresencePenalty.value = cached.currentPresencePenalty ?? null;
    resolvedCurrentFrequencyPenalty.value = cached.currentFrequencyPenalty ?? null;
    resolvedCurrentMaxOutputTokens.value = cached.currentMaxOutputTokens ?? null;
    resolvedModelDefaultMaxOutputTokens.value = cached.currentModelDefaultMaxOutputTokens ?? null;
  }

  function seedModelListFromCurrentSelection() {
    const selected = selectedModel.value?.trim();
    if (!selected) {
      return;
    }

    resolvedCurrentModel.value = selected;
    if (resolvedCurrentProviderId.value !== null) {
      const activeGroup = providerGroups.value.find((group) => group.providerId === resolvedCurrentProviderId.value);
      if (activeGroup && !activeGroup.availableModels.includes(selected)) {
        activeGroup.availableModels = [selected, ...activeGroup.availableModels];
      }
      return;
    }

    if (providerGroups.value.length === 1 && !providerGroups.value[0].availableModels.includes(selected)) {
      providerGroups.value[0].availableModels = [selected, ...providerGroups.value[0].availableModels];
    }
  }

  async function refreshModels() {
    if (!taskId.value) {
      return;
    }

    const requestId = ++activeModelRequestId;
    const hasWarmData = providerGroups.value.some((group) => group.availableModels.length > 0);
    modelsLoading.value = !hasWarmData;
    modelsRefreshing.value = hasWarmData;
    try {
      const response = await invoke<TaskModelSelectorView>('list_provider_models', {
        taskId: taskId.value,
      });
      if (requestId !== activeModelRequestId) {
        return;
      }
      applyProviderModels(response, taskId.value);
    } finally {
      if (requestId === activeModelRequestId) {
        modelsLoading.value = false;
        modelsRefreshing.value = false;
      }
    }
  }

  function applyProviderModels(response: TaskModelSelectorView, currentTaskId: number) {
    const normalizedProviders = response.providers.map((group) => ({
      providerId: group.providerId ?? null,
      providerName: group.providerName,
      providerType: group.providerType,
      providerCacheKey: group.providerCacheKey,
      availableModels: Array.from(
        new Set(
          [
            ...(response.currentProviderId === group.providerId ? [response.currentModel] : []),
            ...group.availableModels,
          ]
            .map((model) => model.trim())
            .filter(Boolean),
        ),
      ),
    }));

    const cacheEntry: CachedTaskModelSelector = {
      currentProviderId: response.currentProviderId ?? null,
      currentModel: response.currentModel,
      currentTemperature: response.currentTemperature ?? null,
      currentTopP: response.currentTopP ?? null,
      currentPresencePenalty: response.currentPresencePenalty ?? null,
      currentFrequencyPenalty: response.currentFrequencyPenalty ?? null,
      currentMaxOutputTokens: response.currentMaxOutputTokens ?? null,
      currentModelDefaultMaxOutputTokens: response.currentModelCapabilities.maxOutputTokens ?? null,
      providers: normalizedProviders,
    };

    taskModelSelectorCache.set(currentTaskId, cacheEntry);
    resolvedCurrentProviderId.value = cacheEntry.currentProviderId ?? null;
    resolvedCurrentModel.value = cacheEntry.currentModel;
    resolvedCurrentTemperature.value = cacheEntry.currentTemperature ?? null;
    resolvedCurrentTopP.value = cacheEntry.currentTopP ?? null;
    resolvedCurrentPresencePenalty.value = cacheEntry.currentPresencePenalty ?? null;
    resolvedCurrentFrequencyPenalty.value = cacheEntry.currentFrequencyPenalty ?? null;
    resolvedCurrentMaxOutputTokens.value = cacheEntry.currentMaxOutputTokens ?? null;
    resolvedModelDefaultMaxOutputTokens.value = cacheEntry.currentModelDefaultMaxOutputTokens ?? null;
    resolvedModelSupportsVision.value = response.currentModelCapabilities.supportsVision;
    providerGroups.value = cacheEntry.providers.map((group) => ({
      ...group,
      availableModels: [...group.availableModels],
    }));
    resetModelSettingsDraft();
  }

  function syncModelSettingsMenuPosition() {
    if (!modelSettingsOpen.value) {
      return;
    }

    const anchor = modelSettingsAnchorRef.value;
    if (!anchor) {
      return;
    }

    const rect = anchor.getBoundingClientRect();
    const menuWidth = Math.max(320, rect.width + 260);
    const viewportPadding = 12;
    const left = Math.min(
      Math.max(viewportPadding, rect.right - menuWidth),
      window.innerWidth - menuWidth - viewportPadding,
    );

    modelSettingsStyle.value = {
      position: 'fixed',
      left: `${left}px`,
      bottom: `${Math.max(viewportPadding, window.innerHeight - rect.top + 10)}px`,
      width: `${menuWidth}px`,
    };
  }

  function syncModelMenuPosition() {
    if (!modelMenuOpen.value) {
      return;
    }

    const anchor = modelMenuAnchorRef.value;
    if (!anchor) {
      return;
    }

    const rect = anchor.getBoundingClientRect();
    const menuWidth = Math.max(rect.width, 320);
    const viewportPadding = 12;
    const left = Math.min(
      Math.max(viewportPadding, rect.left),
      window.innerWidth - menuWidth - viewportPadding,
    );
    const maxHeight = Math.min(416, window.innerHeight - 144);

    modelMenuStyle.value = {
      position: 'fixed',
      left: `${left}px`,
      bottom: `${Math.max(viewportPadding, window.innerHeight - rect.top + 10)}px`,
      width: `${menuWidth}px`,
      maxHeight: `${maxHeight}px`,
    };
  }

  function syncFloatingMenus() {
    syncModelMenuPosition();
    syncModelSettingsMenuPosition();
  }
}

function normalizePath(path?: string) {
  if (!path) {
    return '';
  }

  const normalized = path.replaceAll('\\', '/');
  if (normalized.startsWith('//?/UNC/')) {
    return `//${normalized.slice('//?/UNC/'.length)}`;
  }
  if (normalized.startsWith('//?/')) {
    return normalized.slice('//?/'.length);
  }
  return normalized;
}

function parseOptionalNumber(value: string | number | null | undefined) {
  if (value == null) {
    return null;
  }
  const normalized = typeof value === 'string' ? value.trim() : String(value);
  if (!normalized) {
    return null;
  }
  const parsed = Number(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseOptionalInteger(value: string | number | null | undefined) {
  if (value == null) {
    return null;
  }
  const normalized = typeof value === 'string' ? value.trim() : String(value);
  if (!normalized) {
    return null;
  }
  const parsed = Number(normalized);
  return Number.isInteger(parsed) ? parsed : null;
}
