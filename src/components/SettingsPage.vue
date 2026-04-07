<template>
  <section class="settings-shell">
    <header class="settings-header">
      <div class="flex items-start gap-3">
        <Button
          variant="ghost"
          size="icon"
          class="mt-0.5 rounded-xl border border-[color:var(--ma-line-soft)]"
          @click="emit('close')"
        >
          <Icon :icon="arrowLeftIcon" class="h-4 w-4" />
        </Button>
        <div>
          <p class="text-[11px] uppercase tracking-[0.18em] text-text-dim">Settings</p>
          <h2 class="mt-1 text-[22px] font-semibold tracking-[-0.02em] text-text">应用设置</h2>
          <p class="mt-2 max-w-[720px] text-[13px] leading-6 text-text-muted">
            外观、模型、供应商和角色都放在这里。设置页按职责拆开，默认运行直接收进模型页，避免把模型能力和入口配置拆成两套心智模型。
          </p>
        </div>
      </div>
    </header>

    <div class="settings-layout">
      <aside class="settings-sidebar">
        <div class="settings-sidebar-header">
          <p class="text-[10px] uppercase tracking-[0.18em] text-text-dim">Sections</p>
          <p class="mt-1 text-[12px] leading-5 text-text-muted">把全局外观、模型能力、供应商接入和角色设置集中在一个固定位置。</p>
        </div>

        <nav class="space-y-2">
          <button
            v-for="section in sectionOptions"
            :key="section.value"
            type="button"
            class="settings-nav-item"
            :class="activeSection === section.value ? 'settings-nav-item-active' : ''"
            @click="activeSection = section.value"
          >
            <Icon :icon="section.icon" class="h-4 w-4 shrink-0" />
            <span class="min-w-0 flex-1 text-left">
              <span class="block truncate text-[13px] font-medium text-text">{{ section.label }}</span>
              <span class="mt-0.5 block truncate text-[11px] text-text-dim">{{ section.description }}</span>
            </span>
          </button>
        </nav>
      </aside>

      <div class="min-h-0 overflow-y-auto">
        <AppearanceSettingsSection
          v-if="activeSection === 'appearance'"
          :theme="theme"
          :theme-options="themeOptions"
          @update-theme="emit('updateTheme', $event)"
        />

        <ModelSettingsSection
          v-else-if="activeSection === 'models'"
          :settings="settings"
          :busy="busy"
          :show-editor="modelEditorOpen"
          :active-editor-id="activeEditorId"
          :active-provider-model-id="activeProviderModelId"
          :provider-options="modelProviderOptions"
          :selected-provider-id-string="modelProviderIdString"
          :provider-model-id="providerModelId"
          :provider-model-display-name="providerModelDisplayName"
          :provider-model-context-window="providerModelContextWindow"
          :provider-model-max-output-tokens="providerModelMaxOutputTokens"
          :provider-model-supports-tool-use="providerModelSupportsToolUse"
          :provider-model-supports-vision="providerModelSupportsVision"
          :provider-model-supports-audio="providerModelSupportsAudio"
          :provider-model-supports-pdf="providerModelSupportsPdf"
          :provider-model-server-tools="providerModelServerTools"
          :provider-model-id-options="providerModelIdOptions"
          :server-tool-definitions="serverToolDefinitions"
          :server-tool-format-options="serverToolFormatOptions"
          :is-server-tool-enabled="isServerToolEnabled"
          :provider-type-label="providerTypeLabel"
          :format-capabilities-summary="formatCapabilitiesSummary"
          @save-default-model="submitDefaultModel"
          @start-create-provider-model="startCreateProviderModel"
          @start-edit="hydrateProviderContext"
          @start-edit-provider-model="startEditProviderModel"
          @delete-provider-model="emit('deleteProviderModel', $event)"
          @update:selected-provider-id-string="selectModelProvider"
          @update:provider-model-id="providerModelId = $event"
          @update:provider-model-display-name="providerModelDisplayName = $event"
          @update:provider-model-context-window="providerModelContextWindow = $event"
          @update:provider-model-max-output-tokens="providerModelMaxOutputTokens = $event"
          @update:provider-model-supports-tool-use="providerModelSupportsToolUse = $event"
          @update:provider-model-supports-vision="providerModelSupportsVision = $event"
          @update:provider-model-supports-audio="providerModelSupportsAudio = $event"
          @update:provider-model-supports-pdf="providerModelSupportsPdf = $event"
          @submit-provider-model="submitProviderModel"
          @reset-provider-model-form="resetProviderModelForm"
          @close-editor="closeModelEditor"
          @server-tool-toggle="onServerToolToggle"
          @set-server-tool-format="setServerToolFormat"
        />

        <ProviderChannelsSection
          v-else-if="activeSection === 'providers'"
          :settings="settings"
          :busy="busy"
          :probe-models="probeModels"
          :probe-suggested-models="probeSuggestedModels"
          :probe-models-loading="probeModelsLoading"
          :provider-test-loading="providerTestLoading"
          :provider-test-message="providerTestMessage"
          :provider-test-success="providerTestSuccess"
          :show-editor="providerEditorOpen"
          :active-editor-id="activeEditorId"
          :provider-type="providerType"
          :provider-name="providerName"
          :provider-base-url="providerBaseUrl"
          :provider-api-key="providerApiKey"
          :provider-probe-model="providerProbeModel"
          :provider-type-options="providerTypeOptions"
          :provider-name-placeholder="providerNamePlaceholder"
          :base-url-placeholder="baseUrlPlaceholder"
          :base-url-preview="baseUrlPreview"
          :base-url-hint="baseUrlHint"
          :api-key-placeholder="apiKeyPlaceholder"
          :probe-model-placeholder="probeModelPlaceholder"
          :probe-model-select-placeholder="probeModelSelectPlaceholder"
          :provider-type-label="providerTypeLabel"
          @start-create="startCreate"
          @start-edit="startEdit"
          @delete-provider="emit('deleteProvider', $event)"
          @update:provider-type="providerType = $event"
          @update:provider-name="providerName = $event"
          @update:provider-base-url="providerBaseUrl = $event"
          @update:provider-api-key="providerApiKey = $event"
          @update:provider-probe-model="providerProbeModel = $event"
          @submit-provider="submitProvider"
          @test-provider="testProvider"
          @reset-form="resetForm"
          @request-probe-models-now="requestProbeModelsNow"
          @close-editor="closeProviderEditor"
        />

        <AgentSettingsSection
          v-else-if="activeSection === 'agents'"
          :settings="settings"
          :busy="busy"
          :active-agent-name="activeAgentName"
          :editing-built-in-march="editingBuiltInMarch"
          :agent-name="agentName"
          :agent-display-name="agentDisplayName"
          :agent-description="agentDescription"
          :agent-avatar-color="agentAvatarColor"
          :agent-provider-id-string="agentProviderIdString"
          :agent-model-id="agentModelId"
          :agent-system-prompt="agentSystemPrompt"
          :resolved-agent-name="resolvedAgentName"
          :agent-provider-options="agentProviderOptions"
          :agent-model-options="agentModelOptions"
          :format-agent-binding="formatAgentBinding"
          :format-agent-source="formatAgentSource"
          @start-create-agent="startCreateAgent"
          @start-edit-agent="startEditAgent"
          @restore-march-prompt="emit('restoreMarchPrompt')"
          @delete-agent="emit('deleteAgent', $event)"
          @update:agent-name="agentName = $event"
          @update:agent-display-name="agentDisplayName = $event"
          @update:agent-description="agentDescription = $event"
          @update:agent-avatar-color="agentAvatarColor = $event"
          @update:agent-provider-id-string="agentProviderIdString = $event"
          @update:agent-model-id="agentModelId = $event"
          @update:agent-system-prompt="agentSystemPrompt = $event"
          @submit-agent="submitAgent"
          @reset-agent-form="resetAgentForm"
        />

      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { Icon } from '@iconify/vue';
import arrowLeftIcon from '@iconify-icons/lucide/arrow-left';
import moonIcon from '@iconify-icons/lucide/moon-star';
import serverIcon from '@iconify-icons/lucide/server-cog';
import sunIcon from '@iconify-icons/lucide/sun-medium';
import type { ThemeMode } from '@/composables/useAppearanceSettings';
import type { ProviderSettingsView } from '@/data/mock';
import AgentSettingsSection from '@/components/settings/AgentSettingsSection.vue';
import AppearanceSettingsSection from '@/components/settings/AppearanceSettingsSection.vue';
import ModelSettingsSection from '@/components/settings/ModelSettingsSection.vue';
import ProviderChannelsSection from '@/components/settings/ProviderChannelsSection.vue';
import { Button } from '@/components/ui/button';
import {
  defaultProviderBaseUrl,
  resolveProviderRequestPreview,
} from '@/lib/providerBaseUrl';

const props = defineProps<{
  theme: ThemeMode;
  settings: ProviderSettingsView | null;
  busy?: boolean;
  probeModels: string[];
  probeSuggestedModels: string[];
  probeModelsLoading?: boolean;
  providerTestLoading?: boolean;
  providerTestMessage?: string;
  providerTestSuccess?: boolean;
}>();

const emit = defineEmits<{
  close: [];
  updateTheme: [theme: ThemeMode];
  saveProvider: [input: { id?: number; providerType: string; name: string; baseUrl: string; apiKey: string }];
  saveProviderModel: [input: {
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
  }];
  testProvider: [input: { id?: number; providerType: string; name: string; baseUrl: string; apiKey: string; probeModel?: string }];
  deleteProvider: [providerId: number];
  deleteProviderModel: [providerModelId: number];
  saveAgent: [input: {
    name: string;
    displayName: string;
    description: string;
    systemPrompt: string;
    avatarColor?: string;
    providerId?: number | null;
    modelId?: string | null;
    useCustomMarchPrompt?: boolean;
  }];
  deleteAgent: [name: string];
  restoreMarchPrompt: [];
  saveDefaultModel: [input: { modelConfigId?: number | null }];
  requestProbeModels: [input: { id?: number; providerType: string; baseUrl: string; apiKey: string; probeModel?: string; forceRefresh?: boolean }];
}>();

const activeSection = ref<'appearance' | 'models' | 'providers' | 'agents'>('appearance');
const activeEditorId = ref<number | null>(null);
const providerType = ref('openai_compat');
const providerName = ref('');
const providerBaseUrl = ref('');
const providerApiKey = ref('');
const providerProbeModel = ref('');
const providerEditorOpen = ref(false);
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
const activeAgentName = ref('');
const agentName = ref('');
const agentDisplayName = ref('');
const agentDescription = ref('');
const agentAvatarColor = ref('#64748B');
const agentProviderIdString = ref('');
const agentModelId = ref('');
const agentSystemPrompt = ref('');

const sectionOptions = [
  {
    value: 'appearance' as const,
    label: '外观',
    description: '主题与整体观感',
    icon: sunIcon,
  },
  {
    value: 'models' as const,
    label: '模型',
    description: '模型能力与确认',
    icon: serverIcon,
  },
  {
    value: 'providers' as const,
    label: '供应商',
    description: '凭据、端点与协议',
    icon: serverIcon,
  },
  {
    value: 'agents' as const,
    label: '角色',
    description: 'March 与自定义 agent',
    icon: serverIcon,
  },
];

const themeOptions = [
  {
    value: 'dark' as const,
    label: '深色主题',
    description: '保持 March 当前的低照度桌面感，适合长时间编码和夜间使用。',
    icon: moonIcon,
  },
  {
    value: 'light' as const,
    label: '浅色主题',
    description: '提供更轻盈的阅读层次，适合白天环境和文档密集型工作流。',
    icon: sunIcon,
  },
];

const activeEditorProvider = computed(() =>
  props.settings?.providers.find((provider) => provider.id === activeEditorId.value) ?? null,
);

const editingBuiltInMarch = computed(() => activeAgentName.value === 'march');

const providerTypeOptions = [
  { value: 'openai_compat', label: 'OpenAI-compatible' },
  { value: 'openai', label: 'OpenAI' },
  { value: 'anthropic', label: 'Anthropic' },
  { value: 'gemini', label: 'Gemini' },
  { value: 'fireworks', label: 'Fireworks' },
  { value: 'together', label: 'Together' },
  { value: 'groq', label: 'Groq' },
  { value: 'mimo', label: 'Mimo' },
  { value: 'nebius', label: 'Nebius' },
  { value: 'xai', label: 'xAI' },
  { value: 'deepseek', label: 'DeepSeek' },
  { value: 'zai', label: 'ZAI' },
  { value: 'bigmodel', label: 'BigModel' },
  { value: 'cohere', label: 'Cohere' },
  { value: 'ollama', label: 'Ollama' },
];

const serverToolDefinitions = [
  {
    capability: 'web_search',
    label: 'Web Search',
    formats: ['anthropic', 'openai_responses', 'openai_chat_completions', 'gemini'],
  },
  {
    capability: 'code_execution',
    label: 'Code Execution',
    formats: ['anthropic', 'openai_responses', 'openai_chat_completions', 'gemini'],
  },
  { capability: 'file_search', label: 'File Search', formats: ['openai_responses'] },
] as const;

const serverToolFormatLabels: Record<string, string> = {
  anthropic: 'Anthropic',
  openai_responses: 'OpenAI / Responses',
  openai_chat_completions: 'OpenAI-compatible / Chat Completions',
  gemini: 'Gemini',
};

const agentProviderOptions = computed(() => [
  { value: '', label: '跟随任务默认' },
  ...(props.settings?.providers ?? []).map((provider) => ({
    value: String(provider.id),
    label: provider.name,
  })),
]);

const resolvedAgentName = computed(() => {
  if (editingBuiltInMarch.value) {
    return 'march';
  }
  const normalized = agentName.value.trim().toLowerCase().replaceAll(' ', '-');
  return normalized || '';
});

const selectedAgentProvider = computed(() => {
  const providerId = Number(agentProviderIdString.value);
  if (!Number.isFinite(providerId) || providerId <= 0) {
    return null;
  }
  return props.settings?.providers.find((provider) => provider.id === providerId) ?? null;
});

const agentModelOptions = computed(() => {
  const provider = selectedAgentProvider.value;
  if (!provider) {
    return [];
  }
  return [
    { value: '', label: '跟随任务默认' },
    ...provider.models.map((model) => ({
      value: model.modelId,
      label: model.displayName || model.modelId,
    })),
  ];
});

const modelProviderOptions = computed(() =>
  (props.settings?.providers ?? []).map((provider) => ({
    value: String(provider.id),
    label: `${provider.name} · ${providerTypeLabel(provider.providerType)}`,
  })),
);

const modelProviderIdString = computed(() => (activeEditorId.value ? String(activeEditorId.value) : ''));

const probeModelSelectPlaceholder = computed(() => {
  if (props.probeModelsLoading) {
    return '正在读取供应商模型列表…';
  }
  if (props.probeModels.length) {
    return '从供应商模型列表中选择';
  }
  return '暂无可选列表，仍可先手动填写';
});

const providerModelIdOptions = computed(() => {
  const configured = activeEditorProvider.value?.models.map((model) => model.modelId) ?? [];
  const merged = Array.from(new Set([...props.probeModels, ...configured]))
    .map((model) => model.trim())
    .filter(Boolean);

  return merged.map((model) => ({
    value: model,
    label: model,
  }));
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

function startCreate() {
  activeSection.value = 'providers';
  providerEditorOpen.value = true;
  applyProviderEditorState();
  requestProbeModelsIfNeeded();
  closeModelEditor();
  clearProviderModelDraft();
}

function startCreateAgent() {
  activeSection.value = 'agents';
  applyAgentEditorState();
}

function startEditAgent(agent: ProviderSettingsView['agents'][number]) {
  activeSection.value = 'agents';
  applyAgentEditorState(agent);
}

function startEdit(provider: ProviderSettingsView['providers'][number]) {
  activeSection.value = 'providers';
  providerEditorOpen.value = true;
  applyProviderEditorState(provider);
  requestProbeModelsIfNeeded();
  closeModelEditor();
  clearProviderModelDraft();
}

function hydrateProviderContext(provider: ProviderSettingsView['providers'][number]) {
  applyProviderEditorState(provider);
}

function closeProviderEditor() {
  providerEditorOpen.value = false;
  activeEditorId.value = null;
  providerApiKey.value = '';
  providerProbeModel.value = '';
}

function closeModelEditor() {
  modelEditorOpen.value = false;
  activeProviderModelId.value = null;
}

function selectModelProvider(providerIdString: string) {
  const providerId = Number(providerIdString);
  if (!Number.isFinite(providerId) || providerId <= 0) {
    return;
  }
  const provider = props.settings?.providers.find((item) => item.id === providerId);
  if (!provider) {
    return;
  }
  applyProviderEditorState(provider);
  if (!activeProviderModelId.value) {
    providerModelServerTools.value = {};
  }
  requestProbeModelsIfNeeded();
}

function resetForm() {
  if (activeEditorId.value) {
    const provider = props.settings?.providers.find((item) => item.id === activeEditorId.value);
    if (provider) {
      startEdit(provider);
      return;
    }
  }
  startCreate();
}

function resetAgentForm() {
  if (activeAgentName.value) {
    const agent = props.settings?.agents.find((item) => item.name === activeAgentName.value);
    if (agent) {
      applyAgentEditorState(agent);
      return;
    }
  }
  startCreateAgent();
}

function applyProviderEditorState(provider?: ProviderSettingsView['providers'][number]) {
  activeEditorId.value = provider?.id ?? null;
  providerType.value = provider?.providerType ?? 'openai_compat';
  providerName.value = provider?.name ?? '';
  providerBaseUrl.value = provider?.baseUrl ?? '';
  providerApiKey.value = provider?.apiKey ?? '';
  providerProbeModel.value = '';
}

function applyAgentEditorState(agent?: ProviderSettingsView['agents'][number]) {
  activeAgentName.value = agent?.name ?? '';
  agentName.value = agent?.name ?? '';
  agentDisplayName.value = agent?.displayName ?? '';
  agentDescription.value = agent?.description ?? '';
  agentAvatarColor.value = agent?.avatarColor || '#64748B';
  agentProviderIdString.value = agent?.providerId ? String(agent.providerId) : '';
  agentModelId.value = agent?.modelId ?? '';
  agentSystemPrompt.value = agent?.systemPrompt ?? '';
}

function submitProvider() {
  emit('saveProvider', {
    id: activeEditorId.value ?? undefined,
    providerType: providerType.value,
    name: providerName.value,
    baseUrl: providerBaseUrl.value,
    apiKey: providerApiKey.value,
  });
}

function submitAgent() {
  if (!resolvedAgentName.value) {
    return;
  }

  emit('saveAgent', {
    name: resolvedAgentName.value,
    displayName: agentDisplayName.value,
    description: editingBuiltInMarch.value ? '' : agentDescription.value,
    systemPrompt: agentSystemPrompt.value,
    avatarColor: agentAvatarColor.value,
    providerId: agentProviderIdString.value ? Number(agentProviderIdString.value) : null,
    modelId: agentModelId.value.trim() || null,
    useCustomMarchPrompt: editingBuiltInMarch.value ? true : undefined,
  });
}

function startCreateProviderModel() {
  modelEditorOpen.value = true;
  if (!activeEditorId.value) {
    const firstProvider = props.settings?.providers[0];
    if (firstProvider) {
      applyProviderEditorState(firstProvider);
    }
  }
  requestProbeModelsIfNeeded();
  clearProviderModelDraft();
}

function startEditProviderModel(model: NonNullable<typeof activeEditorProvider.value>['models'][number]) {
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

function resetProviderModelForm() {
  startCreateProviderModel();
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

function submitProviderModel() {
  if (!activeEditorProvider.value) {
    return;
  }

  const serverTools = collectConfiguredServerTools();

  emit('saveProviderModel', {
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

function testProvider() {
  emit('testProvider', {
    id: activeEditorId.value ?? undefined,
    providerType: providerType.value,
    name: providerName.value,
    baseUrl: providerBaseUrl.value,
    apiKey: providerApiKey.value,
    probeModel: providerProbeModel.value,
  });
}

function submitDefaultModel(modelConfigId: number) {
  if (!Number.isFinite(modelConfigId) || modelConfigId <= 0) {
    return;
  }
  emit('saveDefaultModel', {
    modelConfigId,
  });
}

function providerTypeLabel(providerTypeValue: string) {
  return providerTypeOptions.find((option) => option.value === providerTypeValue)?.label ?? providerTypeValue;
}

function formatAgentBinding(providerId?: number | null, modelId?: string | null) {
  if (!providerId || !modelId) {
    return '模型：跟随任务默认';
  }
  const provider = props.settings?.providers.find((item) => item.id === providerId);
  return `模型：${provider?.name ?? providerId} / ${modelId}`;
}

function formatAgentSource(source: string) {
  if (source === 'project') {
    return '来源：项目';
  }
  if (source === 'built_in') {
    return '来源：内置';
  }
  return '来源：用户';
}

function requestProbeModelsNow(forceRefresh = false) {
  emit('requestProbeModels', {
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

function isServerToolEnabled(capability: string) {
  return Boolean(providerModelServerTools.value[capability]);
}

function collectConfiguredServerTools() {
  const supportedCapabilities = new Set(serverToolDefinitions.map((tool) => tool.capability));
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

function formatTokenMetric(value: number) {
  if (value >= 1_000_000) {
    return `${Math.round(value / 100_000) / 10}M`;
  }
  if (value >= 1_000) {
    return `${Math.round(value / 100) / 10}K`;
  }
  return String(value);
}

</script>
