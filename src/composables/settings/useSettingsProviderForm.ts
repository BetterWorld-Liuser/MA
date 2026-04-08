import { computed, ref, type Ref } from 'vue';
import type { ProviderSettingsView } from '@/data/mock';
import { defaultProviderBaseUrl, resolveProviderRequestPreview } from '@/lib/providerBaseUrl';
import { providerTypeLabel, providerTypeOptions } from './settingsShared';

type ReadonlyRef<T> = Readonly<Ref<T>>;
type ProviderItem = ProviderSettingsView['providers'][number];

type SaveProviderInput = {
  id?: number;
  providerType: string;
  name: string;
  baseUrl: string;
  apiKey: string;
};

type TestProviderInput = SaveProviderInput & {
  probeModel?: string;
};

type RequestProbeModelsInput = {
  id?: number;
  providerType: string;
  baseUrl: string;
  apiKey: string;
  probeModel?: string;
  forceRefresh?: boolean;
};

export function useSettingsProviderForm({
  settings,
  probeModels,
  probeModelsLoading,
  onSaveProvider,
  onTestProvider,
  onRequestProbeModels,
}: {
  settings: ReadonlyRef<ProviderSettingsView | null>;
  probeModels: ReadonlyRef<string[]>;
  probeModelsLoading: ReadonlyRef<boolean | undefined>;
  onSaveProvider: (input: SaveProviderInput) => void;
  onTestProvider: (input: TestProviderInput) => void;
  onRequestProbeModels: (input: RequestProbeModelsInput) => void;
}) {
  const activeEditorId = ref<number | null>(null);
  const providerType = ref('openai_compat');
  const providerName = ref('');
  const providerBaseUrl = ref('');
  const providerApiKey = ref('');
  const providerProbeModel = ref('');
  const providerEditorOpen = ref(false);

  const probeModelSelectPlaceholder = computed(() => {
    if (probeModelsLoading.value) {
      return '正在读取供应商模型列表…';
    }
    if (probeModels.value.length) {
      return '从供应商模型列表中选择';
    }
    return '暂无可选列表，仍可先手动填写';
  });

  const providerNamePlaceholder = computed(() => {
    if (providerType.value === 'openai_compat') {
      return 'OpenRouter / Local vLLM';
    }
    return providerTypeLabel(providerType.value);
  });

  const baseUrlPlaceholder = computed(() => defaultProviderBaseUrl(providerType.value));
  const baseUrlPreview = computed(() => resolveProviderRequestPreview(providerType.value, providerBaseUrl.value));

  const baseUrlHint = computed(() => {
    if (providerType.value === 'openai_compat') {
      return '这个类型通常需要显式填写自定义端点，例如 OpenRouter、硅基流动或自建网关。';
    }

    return '可选。留空时使用该 provider 的默认官方端点；填写后会改走你指定的兼容入口。';
  });

  const apiKeyPlaceholder = computed(() => {
    if (providerType.value === 'ollama') {
      return activeEditorId.value ? '留空即可，当前类型默认不需要 API key' : '可留空';
    }
    return activeEditorId.value ? '留空则保持当前 API key' : 'sk-...';
  });

  const probeModelPlaceholder = computed(() => {
    if (providerType.value === 'openai_compat') {
      return '例如 gpt-4o-mini / kimi-k2 / qwen2.5-coder';
    }
    return '留空则使用内置建议模型';
  });

  function applyProviderEditorState(provider?: ProviderItem) {
    activeEditorId.value = provider?.id ?? null;
    providerType.value = provider?.providerType ?? 'openai_compat';
    providerName.value = provider?.name ?? '';
    providerBaseUrl.value = provider?.baseUrl ?? '';
    providerApiKey.value = provider?.apiKey ?? '';
    providerProbeModel.value = '';
  }

  function requestProbeModelsNow(forceRefresh = false) {
    onRequestProbeModels({
      id: activeEditorId.value ?? undefined,
      providerType: providerType.value,
      baseUrl: providerBaseUrl.value,
      apiKey: providerApiKey.value,
      probeModel: providerProbeModel.value,
      forceRefresh,
    });
  }

  function requestProbeModelsIfNeeded() {
    requestProbeModelsNow(false);
  }

  function startCreate() {
    providerEditorOpen.value = true;
    applyProviderEditorState();
    requestProbeModelsIfNeeded();
  }

  function startEdit(provider: ProviderItem) {
    providerEditorOpen.value = true;
    applyProviderEditorState(provider);
    requestProbeModelsIfNeeded();
  }

  function hydrateProviderContext(provider: ProviderItem) {
    applyProviderEditorState(provider);
  }

  function closeProviderEditor() {
    providerEditorOpen.value = false;
    activeEditorId.value = null;
    providerApiKey.value = '';
    providerProbeModel.value = '';
  }

  function resetForm() {
    if (activeEditorId.value) {
      const provider = settings.value?.providers.find((item) => item.id === activeEditorId.value);
      if (provider) {
        startEdit(provider);
        return;
      }
    }
    startCreate();
  }

  function submitProvider() {
    onSaveProvider({
      id: activeEditorId.value ?? undefined,
      providerType: providerType.value,
      name: providerName.value,
      baseUrl: providerBaseUrl.value,
      apiKey: providerApiKey.value,
    });
  }

  function testProvider() {
    onTestProvider({
      id: activeEditorId.value ?? undefined,
      providerType: providerType.value,
      name: providerName.value,
      baseUrl: providerBaseUrl.value,
      apiKey: providerApiKey.value,
      probeModel: providerProbeModel.value,
    });
  }

  return {
    activeEditorId,
    providerType,
    providerName,
    providerBaseUrl,
    providerApiKey,
    providerProbeModel,
    providerEditorOpen,
    providerTypeOptions,
    probeModelSelectPlaceholder,
    providerNamePlaceholder,
    baseUrlPlaceholder,
    baseUrlPreview,
    baseUrlHint,
    apiKeyPlaceholder,
    probeModelPlaceholder,
    applyProviderEditorState,
    requestProbeModelsNow,
    requestProbeModelsIfNeeded,
    startCreate,
    startEdit,
    hydrateProviderContext,
    closeProviderEditor,
    resetForm,
    submitProvider,
    testProvider,
  };
}
