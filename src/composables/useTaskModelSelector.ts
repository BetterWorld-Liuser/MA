import { onMounted, onUnmounted, watch, type Ref } from 'vue';
import { providerTypeLabel } from './taskModelSelectorShared';
import { useFloatingModelMenus } from './useFloatingModelMenus';
import { useModelSettingsDraft } from './useModelSettingsDraft';
import { useTaskModelCatalog } from './useTaskModelCatalog';
import { useWorkingDirectorySelector } from './useWorkingDirectorySelector';

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
  emitSetModel: (selection: { modelConfigId: number }) => void;
  emitSetModelSettings: (settings: {
    temperature?: number | null;
    topP?: number | null;
    presencePenalty?: number | null;
    frequencyPenalty?: number | null;
    maxOutputTokens?: number | null;
  }) => void;
  emitSetWorkingDirectory: (path?: string | null) => void;
};

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
  const catalog = useTaskModelCatalog({
    taskId,
    selectedModel,
    selectedTemperature,
    selectedTopP,
    selectedPresencePenalty,
    selectedFrequencyPenalty,
    selectedMaxOutputTokens,
  });

  const floatingMenus = useFloatingModelMenus({
    filteredModelItems: catalog.filteredModelItems,
    modelSearchQuery: catalog.modelSearchQuery,
    plusMenuOpen,
  });

  function closeModelSettingsMenu() {
    modelSettings.modelSettingsError.value = '';
    floatingMenus.closeModelSettingsMenu();
  }

  const modelSettings = useModelSettingsDraft({
    resolvedCurrentTemperature: catalog.resolvedCurrentTemperature,
    resolvedCurrentTopP: catalog.resolvedCurrentTopP,
    resolvedCurrentPresencePenalty: catalog.resolvedCurrentPresencePenalty,
    resolvedCurrentFrequencyPenalty: catalog.resolvedCurrentFrequencyPenalty,
    resolvedCurrentMaxOutputTokens: catalog.resolvedCurrentMaxOutputTokens,
    resolvedModelDefaultMaxOutputTokens: catalog.resolvedModelDefaultMaxOutputTokens,
    emitSetModelSettings,
    closeModelSettingsMenu,
  });

  const workingDirectoryState = useWorkingDirectorySelector({
    workingDirectory,
    workspacePath,
    emitSetWorkingDirectory,
  });

  watch(taskId, (nextTaskId) => {
    closeAllMenus();
    catalog.restoreModelStateFromCache(nextTaskId);
    modelSettings.resetModelSettingsDraft();
    void catalog.refreshModels(modelSettings.resetModelSettingsDraft);
  });

  watch(
    [
      selectedModel,
      selectedTemperature,
      selectedTopP,
      selectedPresencePenalty,
      selectedFrequencyPenalty,
      selectedMaxOutputTokens,
    ],
    () => {
      catalog.syncResolvedFromSelectedInputs(
        floatingMenus.modelSettingsOpen.value,
        modelSettings.resetModelSettingsDraft,
      );
    },
    { immediate: true },
  );

  watch(settingsOpen, (open) => {
    if (open) {
      closeAllMenus();
    }
  });

  onMounted(() => {
    document.addEventListener('mousedown', handleModelMenuPointerDown);
    window.addEventListener('resize', floatingMenus.syncFloatingMenus);
    window.addEventListener('scroll', floatingMenus.syncFloatingMenus, true);
    catalog.restoreModelStateFromCache(taskId.value);
    modelSettings.resetModelSettingsDraft();
    void catalog.refreshModels(modelSettings.resetModelSettingsDraft);
  });

  onUnmounted(() => {
    document.removeEventListener('mousedown', handleModelMenuPointerDown);
    window.removeEventListener('resize', floatingMenus.syncFloatingMenus);
    window.removeEventListener('scroll', floatingMenus.syncFloatingMenus, true);
  });

  function primeModelMenu() {
    catalog.restoreModelStateFromCache(taskId.value);
    void catalog.refreshModels(modelSettings.resetModelSettingsDraft);
  }

  function handleModelMenuPointerDown(event: MouseEvent) {
    floatingMenus.handleModelMenuPointerDown(event, () => {
      modelSettings.modelSettingsError.value = '';
    });
  }

  function closeAllMenus() {
    closeComposerMenus();
    floatingMenus.closeModelMenu();
    closeModelSettingsMenu();
  }

  function selectModel(modelConfigId: number, model: string) {
    catalog.resolvedCurrentModelConfigId.value = modelConfigId;
    catalog.resolvedCurrentModel.value = model;
    emitSetModel({ modelConfigId });
    floatingMenus.closeModelMenu();
  }

  function isModelActive(modelConfigId: number, model: string) {
    if (catalog.resolvedCurrentModelConfigId.value != null) {
      return catalog.resolvedCurrentModelConfigId.value === modelConfigId;
    }
    return model === catalog.effectiveSelectedModel.value;
  }

  async function toggleModelMenu() {
    await floatingMenus.toggleModelMenu(primeModelMenu);
  }

  async function toggleModelSettingsMenu() {
    await floatingMenus.toggleModelSettingsMenu(primeModelMenu, modelSettings.resetModelSettingsDraft);
  }

  return {
    disabled,
    modelMenuAnchorRef: floatingMenus.modelMenuAnchorRef,
    modelMenuPanelRef: floatingMenus.modelMenuPanelRef,
    modelSearchRef: floatingMenus.modelSearchRef,
    modelMenuOpen: floatingMenus.modelMenuOpen,
    modelSettingsAnchorRef: floatingMenus.modelSettingsAnchorRef,
    modelSettingsPanelRef: floatingMenus.modelSettingsPanelRef,
    modelSettingsOpen: floatingMenus.modelSettingsOpen,
    modelItems: catalog.modelItems,
    modelSearchQuery: catalog.modelSearchQuery,
    modelsLoading: catalog.modelsLoading,
    modelsRefreshing: catalog.modelsRefreshing,
    modelMenuStyle: floatingMenus.modelMenuStyle,
    modelSettingsStyle: floatingMenus.modelSettingsStyle,
    supportsVision: catalog.supportsVision,
    effectiveSelectedModel: catalog.effectiveSelectedModel,
    modelButtonLabel: catalog.modelButtonLabel,
    maxOutputTokensPlaceholder: modelSettings.maxOutputTokensPlaceholder,
    isCustomWorkingDirectory: workingDirectoryState.isCustomWorkingDirectory,
    workingDirectoryLabel: workingDirectoryState.workingDirectoryLabel,
    workingDirectoryTooltip: workingDirectoryState.workingDirectoryTooltip,
    filteredModelItems: catalog.filteredModelItems,
    temperatureDraft: modelSettings.temperatureDraft,
    topPDraft: modelSettings.topPDraft,
    presencePenaltyDraft: modelSettings.presencePenaltyDraft,
    frequencyPenaltyDraft: modelSettings.frequencyPenaltyDraft,
    maxOutputTokensDraft: modelSettings.maxOutputTokensDraft,
    modelSettingsError: modelSettings.modelSettingsError,
    toggleModelMenu,
    toggleModelSettingsMenu,
    pickWorkingDirectory: workingDirectoryState.pickWorkingDirectory,
    resetWorkingDirectory: workingDirectoryState.resetWorkingDirectory,
    selectModel,
    isModelActive,
    applyModelSettings: modelSettings.applyModelSettings,
    resetModelSettingsDraft: modelSettings.resetModelSettingsDraft,
    providerTypeLabel,
    closeAllMenus,
  };
}
