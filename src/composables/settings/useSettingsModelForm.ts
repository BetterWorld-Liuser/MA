import { computed, ref, type Ref } from 'vue';
import type { ProviderSettingsView } from '@/data/mock';
import {
  providerTypeLabel,
  serverToolDefinitions,
  serverToolFormatLabels,
} from './settingsShared';

type ReadonlyRef<T> = Readonly<Ref<T>>;
type ProviderItem = ProviderSettingsView['providers'][number];
type ProviderModelItem = ProviderItem['models'][number];

type SaveProviderModelInput = {
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
};

export function useSettingsModelForm({
  settings,
  probeModels,
  activeEditorId,
  providerType,
  applyProviderEditorState,
  requestProbeModelsIfNeeded,
  onSaveProviderModel,
}: {
  settings: ReadonlyRef<ProviderSettingsView | null>;
  probeModels: ReadonlyRef<string[]>;
  activeEditorId: Ref<number | null>;
  providerType: Ref<string>;
  applyProviderEditorState: (provider?: ProviderItem) => void;
  requestProbeModelsIfNeeded: () => void;
  onSaveProviderModel: (input: SaveProviderModelInput) => void;
}) {
  const modelEditorOpen = ref(false);
  const activeProviderModelId = ref<number | null>(null);
  const providerModelId = ref('');
  const providerModelDisplayName = ref('');
  const providerModelContextWindow = ref('256000');
  const providerModelMaxOutputTokens = ref('128000');
  const providerModelSupportsToolUse = ref(false);
  const providerModelSupportsVision = ref(false);
  const providerModelSupportsAudio = ref(false);
  const providerModelSupportsPdf = ref(false);
  const providerModelServerTools = ref<Record<string, string>>({});

  const activeEditorProvider = computed(() =>
    settings.value?.providers.find((provider) => provider.id === activeEditorId.value) ?? null,
  );

  const modelProviderOptions = computed(() =>
    (settings.value?.providers ?? []).map((provider) => ({
      value: String(provider.id),
      label: `${provider.name} · ${providerTypeLabel(provider.providerType)}`,
    })),
  );

  const modelProviderIdString = computed(() => (activeEditorId.value ? String(activeEditorId.value) : ''));

  const providerModelIdOptions = computed(() => {
    const configured = activeEditorProvider.value?.models.map((model) => model.modelId) ?? [];
    const merged = Array.from(new Set([...probeModels.value, ...configured]))
      .map((model) => model.trim())
      .filter(Boolean);

    return merged.map((model) => ({
      value: model,
      label: model,
    }));
  });

  function closeModelEditor() {
    modelEditorOpen.value = false;
    activeProviderModelId.value = null;
  }

  function clearProviderModelDraft() {
    activeProviderModelId.value = null;
    providerModelId.value = '';
    providerModelDisplayName.value = '';
    providerModelContextWindow.value = '256000';
    providerModelMaxOutputTokens.value = '128000';
    providerModelSupportsToolUse.value = false;
    providerModelSupportsVision.value = false;
    providerModelSupportsAudio.value = false;
    providerModelSupportsPdf.value = false;
    providerModelServerTools.value = {};
  }

  function startCreateProviderModel() {
    modelEditorOpen.value = true;
    if (!activeEditorId.value) {
      const firstProvider = settings.value?.providers[0];
      if (firstProvider) {
        applyProviderEditorState(firstProvider);
      }
    }
    requestProbeModelsIfNeeded();
    clearProviderModelDraft();
  }

  function startEditProviderModel(model: ProviderModelItem) {
    modelEditorOpen.value = true;
    requestProbeModelsIfNeeded();
    activeProviderModelId.value = model.id;
    providerModelId.value = model.modelId;
    providerModelDisplayName.value = model.displayName ?? '';
    providerModelContextWindow.value = String(model.capabilities.contextWindow);
    providerModelMaxOutputTokens.value = String(model.capabilities.maxOutputTokens);
    providerModelSupportsToolUse.value = model.capabilities.supportsToolUse;
    providerModelSupportsVision.value = model.capabilities.supportsVision;
    providerModelSupportsAudio.value = model.capabilities.supportsAudio;
    providerModelSupportsPdf.value = model.capabilities.supportsPdf;
    providerModelServerTools.value = Object.fromEntries(
      model.capabilities.serverTools.map((tool) => [tool.capability, tool.format]),
    );
  }

  function selectModelProvider(providerIdString: string) {
    const providerId = Number(providerIdString);
    if (!Number.isFinite(providerId) || providerId <= 0) {
      return;
    }
    const provider = settings.value?.providers.find((item) => item.id === providerId);
    if (!provider) {
      return;
    }
    applyProviderEditorState(provider);
    if (!activeProviderModelId.value) {
      providerModelServerTools.value = {};
    }
    requestProbeModelsIfNeeded();
  }

  function resetProviderModelForm() {
    startCreateProviderModel();
  }

  function isServerToolFormatCompatibleWithProviderType(format: string, providerTypeValue: string) {
    if (!providerTypeValue) {
      return true;
    }
    if (format === 'openai_responses') {
      return providerTypeValue === 'openai';
    }
    if (format === 'openai_chat_completions') {
      return !['anthropic', 'gemini', 'openai'].includes(providerTypeValue);
    }
    return format === providerTypeValue;
  }

  function serverToolFormatOptionLabel(capability: string, format: string) {
    const providerLabel = serverToolFormatLabels[format] ?? format;
    if (capability === 'web_search' && format === 'openai_responses') {
      return `${providerLabel} (web_search)`;
    }
    if (capability === 'web_search' && format === 'openai_chat_completions') {
      return `${providerLabel} (web_search_preview)`;
    }
    if (capability === 'web_search' && format === 'anthropic') {
      return `${providerLabel} (web_search_20250305)`;
    }
    if (capability === 'web_search' && format === 'gemini') {
      return `${providerLabel} (google_search)`;
    }
    if (capability === 'code_execution' && format === 'openai_responses') {
      return `${providerLabel} (code_interpreter)`;
    }
    if (capability === 'code_execution' && format === 'openai_chat_completions') {
      return `${providerLabel} (code_interpreter)`;
    }
    if (capability === 'code_execution' && format === 'anthropic') {
      return `${providerLabel} (code_execution_20250522)`;
    }
    if (capability === 'code_execution' && format === 'gemini') {
      return `${providerLabel} (code_execution)`;
    }
    if (capability === 'file_search' && format === 'openai_responses') {
      return `${providerLabel} (file_search)`;
    }
    return providerLabel;
  }

  function serverToolFormatOptions(capability: string) {
    const definition = serverToolDefinitions.find((tool) => tool.capability === capability);
    const formats = (definition?.formats ?? []).filter((format) =>
      isServerToolFormatCompatibleWithProviderType(format, providerType.value),
    );
    return formats.map((format) => ({
      value: format,
      label: serverToolFormatOptionLabel(capability, format),
    }));
  }

  function isServerToolEnabled(capability: string) {
    return Boolean(providerModelServerTools.value[capability]);
  }

  function collectConfiguredServerTools() {
    const supportedCapabilities = new Set<string>(serverToolDefinitions.map((tool) => tool.capability));
    return Object.entries(providerModelServerTools.value)
      .map(([capability, format]) => ({
        capability: capability.trim(),
        format: String(format ?? '').trim(),
      }))
      .filter(
        (tool) =>
          tool.capability
          && supportedCapabilities.has(tool.capability)
          && tool.format,
      );
  }

  function toggleServerTool(capability: string, enabled: boolean) {
    if (enabled) {
      const [firstFormat] = serverToolFormatOptions(capability);
      if (firstFormat) {
        providerModelServerTools.value = {
          ...providerModelServerTools.value,
          [capability]: providerModelServerTools.value[capability] || firstFormat.value,
        };
        providerModelSupportsToolUse.value = true;
      }
      return;
    }

    const next = { ...providerModelServerTools.value };
    delete next[capability];
    providerModelServerTools.value = next;
  }

  function setServerToolFormat(capability: string, format: string) {
    if (!format) {
      toggleServerTool(capability, false);
      return;
    }
    providerModelServerTools.value = {
      ...providerModelServerTools.value,
      [capability]: format,
    };
  }

  function onServerToolToggle(capability: string, enabled: boolean) {
    toggleServerTool(capability, enabled);
  }

  function submitProviderModel() {
    if (!activeEditorProvider.value) {
      return;
    }

    const serverTools = collectConfiguredServerTools();

    onSaveProviderModel({
      id: activeProviderModelId.value ?? undefined,
      providerId: activeEditorProvider.value.id,
      modelId: providerModelId.value,
      displayName: providerModelDisplayName.value,
      contextWindow: Math.max(1, Number(providerModelContextWindow.value) || 256000),
      maxOutputTokens: Math.max(1, Number(providerModelMaxOutputTokens.value) || 128000),
      supportsToolUse: providerModelSupportsToolUse.value || serverTools.length > 0,
      supportsVision: providerModelSupportsVision.value,
      supportsAudio: providerModelSupportsAudio.value,
      supportsPdf: providerModelSupportsPdf.value,
      serverTools,
    });
  }

  function formatTokenMetric(value: number) {
    if (value >= 1_000_000) {
      return `${Math.round(value / 100_000) / 10}M`;
    }
    if (value >= 1_000) {
      return `${Math.round(value / 100) / 10}K`;
    }
    return String(value);
  }

  function formatCapabilitiesSummary(capabilities: {
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
    const serverToolLabels = capabilities.serverTools.map((tool) => {
      if (tool.capability === 'web_search') {
        return '搜索';
      }
      if (tool.capability === 'code_execution') {
        return '代码执行';
      }
      if (tool.capability === 'file_search') {
        return '文件检索';
      }
      return tool.capability;
    });
    const featureLabels = [
      capabilities.supportsToolUse ? '工具' : null,
      capabilities.supportsVision ? '图片' : null,
      capabilities.supportsAudio ? '音频' : null,
      capabilities.supportsPdf ? 'PDF' : null,
      ...serverToolLabels,
    ].filter(Boolean);
    const summary = featureLabels.length ? featureLabels.join(' · ') : '纯文本';
    return `${formatTokenMetric(capabilities.contextWindow)} context · ${formatTokenMetric(capabilities.maxOutputTokens)} output · ${summary}`;
  }

  return {
    activeEditorProvider,
    modelEditorOpen,
    activeProviderModelId,
    providerModelId,
    providerModelDisplayName,
    providerModelContextWindow,
    providerModelMaxOutputTokens,
    providerModelSupportsToolUse,
    providerModelSupportsVision,
    providerModelSupportsAudio,
    providerModelSupportsPdf,
    providerModelServerTools,
    modelProviderOptions,
    modelProviderIdString,
    providerModelIdOptions,
    serverToolDefinitions,
    closeModelEditor,
    clearProviderModelDraft,
    startCreateProviderModel,
    startEditProviderModel,
    selectModelProvider,
    resetProviderModelForm,
    serverToolFormatOptions,
    isServerToolEnabled,
    setServerToolFormat,
    onServerToolToggle,
    submitProviderModel,
    formatCapabilitiesSummary,
  };
}
