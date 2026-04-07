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
            外观和 provider 都放在这里。主题会立即生效并保存在本地，provider 配置仍然由用户目录下的设置库统一管理。
          </p>
        </div>
      </div>
    </header>

    <div class="settings-layout">
      <aside class="settings-sidebar">
        <div class="settings-sidebar-header">
          <p class="text-[10px] uppercase tracking-[0.18em] text-text-dim">Sections</p>
          <p class="mt-1 text-[12px] leading-5 text-text-muted">把全局外观和运行入口集中在一个固定位置。</p>
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

        <ProviderSettingsSection
          v-else-if="activeSection === 'providers'"
          :settings="settings"
          :busy="busy"
          :probe-models="probeModels"
          :probe-suggested-models="probeSuggestedModels"
          :probe-models-loading="probeModelsLoading"
          :provider-test-message="providerTestMessage"
          :provider-test-success="providerTestSuccess"
          :active-editor-id="activeEditorId"
          :active-provider-model-id="activeProviderModelId"
          :provider-type="providerType"
          :provider-name="providerName"
          :provider-base-url="providerBaseUrl"
          :provider-api-key="providerApiKey"
          :provider-probe-model="providerProbeModel"
          :provider-type-options="providerTypeOptions"
          :provider-name-placeholder="providerNamePlaceholder"
          :base-url-placeholder="baseUrlPlaceholder"
          :base-url-hint="baseUrlHint"
          :api-key-placeholder="apiKeyPlaceholder"
          :probe-model-placeholder="probeModelPlaceholder"
          :probe-model-select-placeholder="probeModelSelectPlaceholder"
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
          :provider-model-id-suggestions="providerModelIdSuggestions"
          :server-tool-definitions="serverToolDefinitions"
          :server-tool-format-options="serverToolFormatOptions"
          :is-server-tool-enabled="isServerToolEnabled"
          :provider-type-label="providerTypeLabel"
          :format-capabilities-summary="formatCapabilitiesSummary"
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
          @start-create-provider-model="startCreateProviderModel"
          @start-edit-provider-model="startEditProviderModel"
          @delete-provider-model="emit('deleteProviderModel', $event)"
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
          @server-tool-toggle="onServerToolToggle"
          @set-server-tool-format="setServerToolFormat"
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

        <div v-else class="space-y-5">
          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">默认运行配置</h3>
                <p class="settings-section-copy">这是应用级默认值，用来决定新任务初始使用哪个 provider 与模型。</p>
              </div>
              <Button
                variant="outline"
                size="sm"
                :disabled="!defaultProviderIdLocal || modelsLoading"
                @click="requestModels"
              >
                {{ modelsLoading ? '刷新中…' : '刷新模型' }}
              </Button>
            </div>

            <div class="space-y-4">
              <div class="dialog-field">
                <label class="dialog-label" for="default-provider">默认 Provider</label>
                <SettingsSelect
                  v-model="defaultProviderIdString"
                  :options="providerOptions"
                  placeholder="请选择"
                />
                <p class="dialog-hint">这里选的是全局默认入口，只用于之后新建任务的初始 provider / model。</p>
              </div>
              <div class="dialog-field">
                <label class="dialog-label" for="default-model">默认模型</label>
                <template v-if="availableModels.length">
                  <SettingsSelect
                    v-model="defaultModelLocal"
                    :options="modelOptions"
                    placeholder="请选择模型"
                    searchable
                    search-placeholder="搜索模型…"
                  />
                </template>
                <template v-else>
                  <Input id="default-model" v-model="defaultModelLocal" placeholder="gpt-5.3-codex / qwen2.5-coder" />
                </template>
                <div v-if="!availableModels.length && suggestedModels.length" class="mt-2 flex flex-wrap gap-2">
                  <button
                    v-for="model in suggestedModels"
                    :key="model"
                    type="button"
                    class="rounded-full border border-[color:var(--ma-line-soft)] px-2.5 py-1 text-[11px] text-text-dim transition hover:bg-bg-hover hover:text-text"
                    @click="defaultModelLocal = model"
                  >
                    {{ model }}
                  </button>
                </div>
              </div>
              <div class="flex items-center justify-end">
                <Button :disabled="busy || !defaultProviderIdLocal || !defaultModelLocal.trim()" @click="submitDefaultProvider">
                  保存默认配置
                </Button>
              </div>
            </div>
          </section>

          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">说明</h3>
                <p class="settings-section-copy">默认运行配置是应用级入口，不与任何单个 Provider 绑定。</p>
              </div>
            </div>

            <div class="grid gap-3 md:grid-cols-3">
              <article class="settings-info-card">
                <p class="settings-info-label">作用范围</p>
                <p class="settings-info-value">只影响之后新建的任务；已有任务保持自己的 provider 与模型</p>
              </article>
              <article class="settings-info-card">
                <p class="settings-info-label">模型来源</p>
                <p class="settings-info-value">来自当前默认 Provider 的可读模型列表</p>
              </article>
              <article class="settings-info-card">
                <p class="settings-info-label">关系边界</p>
                <p class="settings-info-value">与 Provider 凭据编辑分离，避免混淆全局配置与接入配置</p>
              </article>
            </div>
          </section>
        </div>
      </div>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import arrowLeftIcon from '@iconify-icons/lucide/arrow-left';
import moonIcon from '@iconify-icons/lucide/moon-star';
import slidersHorizontalIcon from '@iconify-icons/lucide/sliders-horizontal';
import serverIcon from '@iconify-icons/lucide/server-cog';
import sunIcon from '@iconify-icons/lucide/sun-medium';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { ThemeMode } from '@/composables/useAppearanceSettings';
import type { ProviderSettingsView } from '@/data/mock';
import AgentSettingsSection from '@/components/settings/AgentSettingsSection.vue';
import AppearanceSettingsSection from '@/components/settings/AppearanceSettingsSection.vue';
import ProviderSettingsSection from '@/components/settings/ProviderSettingsSection.vue';
import SettingsSelect from './SettingsSelect.vue';

const props = defineProps<{
  theme: ThemeMode;
  settings: ProviderSettingsView | null;
  busy?: boolean;
  modelsLoading?: boolean;
  availableModels: string[];
  suggestedModels: string[];
  probeModels: string[];
  probeSuggestedModels: string[];
  probeModelsLoading?: boolean;
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
  saveDefaultProvider: [input: { providerId: number; model: string }];
  requestModels: [providerId: number];
  requestProbeModels: [input: { id?: number; providerType: string; baseUrl: string; apiKey: string; probeModel?: string }];
}>();

const activeSection = ref<'appearance' | 'providers' | 'agents' | 'defaults'>('appearance');
const activeEditorId = ref<number | null>(null);
const providerType = ref('openai_compat');
const providerName = ref('');
const providerBaseUrl = ref('');
const providerApiKey = ref('');
const providerProbeModel = ref('');
const activeProviderModelId = ref<number | null>(null);
const providerModelId = ref('');
const providerModelDisplayName = ref('');
const providerModelContextWindow = ref('131072');
const providerModelMaxOutputTokens = ref('4096');
const providerModelSupportsToolUse = ref(false);
const providerModelSupportsVision = ref(false);
const providerModelSupportsAudio = ref(false);
const providerModelSupportsPdf = ref(false);
const providerModelServerTools = ref<Record<string, string>>({});
const defaultProviderIdString = ref('');
const defaultModelLocal = ref('');
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
    value: 'providers' as const,
    label: 'Providers',
    description: '模型入口与凭据',
    icon: serverIcon,
  },
  {
    value: 'agents' as const,
    label: '角色',
    description: 'March 与自定义 agent',
    icon: serverIcon,
  },
  {
    value: 'defaults' as const,
    label: '默认运行',
    description: '默认 provider 与模型',
    icon: slidersHorizontalIcon,
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

const defaultProviderIdLocal = computed(() => {
  const parsed = Number(defaultProviderIdString.value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
});

const activeEditorProvider = computed(() =>
  props.settings?.providers.find((provider) => provider.id === activeEditorId.value) ?? null,
);

const editingBuiltInMarch = computed(() => activeAgentName.value === 'march');

const providerOptions = computed(() =>
  (props.settings?.providers ?? []).map((provider) => ({
    value: String(provider.id),
    label: `${provider.name} · ${providerTypeLabel(provider.providerType)}`,
  })),
);

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
  { capability: 'web_search', label: 'Web Search', formats: ['anthropic', 'openai', 'gemini'] },
  { capability: 'code_execution', label: 'Code Execution', formats: ['anthropic', 'openai', 'gemini'] },
  { capability: 'file_search', label: 'File Search', formats: ['openai'] },
] as const;

const serverToolFormatLabels: Record<string, string> = {
  anthropic: 'Anthropic',
  openai: 'OpenAI',
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

const modelOptions = computed(() =>
  props.availableModels.map((model) => ({
    value: model,
    label: model,
  })),
);

const probeModelOptions = computed(() =>
  props.probeModels.map((model) => ({
    value: model,
    label: model,
  })),
);

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

const providerModelIdSuggestions = computed(() =>
  Array.from(
    new Set([
      ...props.probeSuggestedModels,
      ...props.probeModels.slice(0, 8),
      ...(activeEditorProvider.value?.models.map((model) => model.modelId) ?? []),
    ]),
  )
    .map((model) => model.trim())
    .filter(Boolean)
    .slice(0, 10),
);

const providerNamePlaceholder = computed(() => {
  if (providerType.value === 'openai_compat') {
    return 'OpenRouter / Local vLLM';
  }
  return providerTypeLabel(providerType.value);
});

const providerBaseUrlDefaults: Record<string, string> = {
  openai_compat: 'https://api.openai.com/v1',
  openai: 'https://api.openai.com/v1',
  anthropic: 'https://api.anthropic.com/v1',
  gemini: 'https://generativelanguage.googleapis.com/v1beta',
  fireworks: 'https://api.fireworks.ai/inference/v1',
  together: 'https://api.together.xyz/v1',
  groq: 'https://api.groq.com/openai/v1',
  mimo: 'https://api.mimo.org/v1',
  nebius: 'https://api.studio.nebius.com/v1',
  xai: 'https://api.x.ai/v1',
  deepseek: 'https://api.deepseek.com/v1',
  zai: 'https://api.z.ai/api/paas/v4',
  bigmodel: 'https://open.bigmodel.cn/api/paas/v4',
  cohere: 'https://api.cohere.com/v2',
  ollama: 'http://localhost:11434/v1',
};

const baseUrlPlaceholder = computed(
  () => providerBaseUrlDefaults[providerType.value] ?? 'https://api.example.com/v1',
);

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

watch(
  () => props.settings,
  (settings) => {
    defaultProviderIdString.value = settings?.defaultProviderId ? String(settings.defaultProviderId) : '';
    defaultModelLocal.value = settings?.defaultModel ?? '';
  },
  { immediate: true },
);

watch(defaultProviderIdLocal, (providerId, previous) => {
  if (!providerId || providerId === previous) {
    return;
  }
  emit('requestModels', providerId);
});

watch(
  [activeSection, activeEditorId, providerType, providerBaseUrl, providerApiKey, providerProbeModel],
  () => {
    if (activeSection.value !== 'providers') {
      return;
    }
    scheduleProbeModelsRequest();
  },
);

let probeModelRequestTimer: ReturnType<typeof window.setTimeout> | null = null;

onUnmounted(() => {
  if (probeModelRequestTimer) {
    window.clearTimeout(probeModelRequestTimer);
  }
});

function startCreate() {
  activeSection.value = 'providers';
  applyProviderEditorState();
  resetProviderModelForm();
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
  applyProviderEditorState(provider);
  resetProviderModelForm();
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
  providerApiKey.value = '';
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
  activeProviderModelId.value = null;
  providerModelId.value = '';
  providerModelDisplayName.value = '';
  providerModelContextWindow.value = '131072';
  providerModelMaxOutputTokens.value = '4096';
  providerModelSupportsToolUse.value = false;
  providerModelSupportsVision.value = false;
  providerModelSupportsAudio.value = false;
  providerModelSupportsPdf.value = false;
  providerModelServerTools.value = {};
}

function startEditProviderModel(model: NonNullable<typeof activeEditorProvider.value>['models'][number]) {
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

function submitProviderModel() {
  if (!activeEditorProvider.value) {
    return;
  }

  const serverTools = serverToolDefinitions
    .map((tool) => ({
      capability: tool.capability,
      format: providerModelServerTools.value[tool.capability]?.trim() ?? '',
    }))
    .filter((tool) => tool.format);

  emit('saveProviderModel', {
    id: activeProviderModelId.value ?? undefined,
    providerId: activeEditorProvider.value.id,
    modelId: providerModelId.value,
    displayName: providerModelDisplayName.value,
    contextWindow: Math.max(1, Number(providerModelContextWindow.value) || 131072),
    maxOutputTokens: Math.max(1, Number(providerModelMaxOutputTokens.value) || 4096),
    supportsToolUse: providerModelSupportsToolUse.value || serverTools.length > 0,
    supportsVision: providerModelSupportsVision.value,
    supportsAudio: providerModelSupportsAudio.value,
    supportsPdf: providerModelSupportsPdf.value,
    serverTools,
  });
  resetProviderModelForm();
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

function requestModels() {
  if (!defaultProviderIdLocal.value) {
    return;
  }
  emit('requestModels', defaultProviderIdLocal.value);
}

function submitDefaultProvider() {
  if (!defaultProviderIdLocal.value) {
    return;
  }
  emit('saveDefaultProvider', {
    providerId: defaultProviderIdLocal.value,
    model: defaultModelLocal.value,
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

function requestProbeModelsNow() {
  if (probeModelRequestTimer) {
    window.clearTimeout(probeModelRequestTimer);
    probeModelRequestTimer = null;
  }
  emit('requestProbeModels', {
    id: activeEditorId.value ?? undefined,
    providerType: providerType.value,
    baseUrl: providerBaseUrl.value,
    apiKey: providerApiKey.value,
    probeModel: providerProbeModel.value,
  });
}

function scheduleProbeModelsRequest() {
  if (probeModelRequestTimer) {
    window.clearTimeout(probeModelRequestTimer);
  }
  probeModelRequestTimer = window.setTimeout(() => {
    requestProbeModelsNow();
  }, 350);
}

function serverToolFormatOptions(capability: string) {
  const definition = serverToolDefinitions.find((tool) => tool.capability === capability);
  return (definition?.formats ?? []).map((format) => ({
    value: format,
    label: serverToolFormatOptionLabel(capability, format),
  }));
}

function serverToolFormatOptionLabel(capability: string, format: string) {
  const providerLabel = serverToolFormatLabels[format] ?? format;
  if (capability === 'web_search' && format === 'openai') {
    return `${providerLabel} (web_search_preview)`;
  }
  if (capability === 'web_search' && format === 'anthropic') {
    return `${providerLabel} (web_search_20250305)`;
  }
  if (capability === 'web_search' && format === 'gemini') {
    return `${providerLabel} (google_search)`;
  }
  if (capability === 'code_execution' && format === 'openai') {
    return `${providerLabel} (code_interpreter)`;
  }
  if (capability === 'code_execution' && format === 'anthropic') {
    return `${providerLabel} (code_execution_20250522)`;
  }
  if (capability === 'code_execution' && format === 'gemini') {
    return `${providerLabel} (code_execution)`;
  }
  if (capability === 'file_search' && format === 'openai') {
    return `${providerLabel} (file_search)`;
  }
  return providerLabel;
}

function isServerToolEnabled(capability: string) {
  return Boolean(providerModelServerTools.value[capability]);
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

function onServerToolToggle(capability: string, event: Event) {
  toggleServerTool(capability, (event.target as HTMLInputElement | null)?.checked ?? false);
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
