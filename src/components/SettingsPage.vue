<template>
  <section class="settings-shell">
    <header class="settings-header">
      <div>
        <p class="text-[11px] uppercase tracking-[0.18em] text-text-dim">Settings</p>
        <h2 class="mt-1 text-[22px] font-semibold tracking-[-0.02em] text-text">应用设置</h2>
        <p class="mt-2 max-w-[720px] text-[13px] leading-6 text-text-muted">
          外观和 provider 都放在这里。主题会立即生效并保存在本地，provider 配置仍然由用户目录下的设置库统一管理。
        </p>
      </div>
      <Button variant="ghost" size="icon" class="rounded-xl border border-[color:var(--ma-line-soft)]" @click="emit('close')">
        <Icon :icon="xIcon" class="h-4 w-4" />
      </Button>
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
        <div v-if="activeSection === 'appearance'" class="space-y-5">
          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">主题</h3>
                <p class="settings-section-copy">目前提供深色与浅色两套主题。切换后立即作用于整套应用壳层与面板组件。</p>
              </div>
            </div>

            <div class="grid gap-4 lg:grid-cols-2">
              <button
                v-for="option in themeOptions"
                :key="option.value"
                type="button"
                class="theme-card"
                :class="theme === option.value ? 'theme-card-active' : ''"
                @click="emit('updateTheme', option.value)"
              >
                <div class="flex items-start justify-between gap-4">
                  <div>
                    <div class="flex items-center gap-2">
                      <Icon :icon="option.icon" class="h-4 w-4 text-accent" />
                      <h4 class="text-[15px] font-medium text-text">{{ option.label }}</h4>
                    </div>
                    <p class="mt-2 text-[12px] leading-5 text-text-muted">{{ option.description }}</p>
                  </div>
                  <span class="theme-card-check">
                    <Icon v-if="theme === option.value" :icon="checkIcon" class="h-3.5 w-3.5" />
                  </span>
                </div>

                <div class="theme-preview" :data-preview-theme="option.value">
                  <div class="theme-preview-titlebar">
                    <span class="theme-preview-logo">M</span>
                    <span class="text-[10px] font-medium">March</span>
                  </div>
                  <div class="theme-preview-body">
                    <div class="theme-preview-sidebar">
                      <span class="theme-preview-chip theme-preview-chip-active"></span>
                      <span class="theme-preview-chip"></span>
                      <span class="theme-preview-chip"></span>
                    </div>
                    <div class="theme-preview-main">
                      <div class="theme-preview-message"></div>
                      <div class="theme-preview-message theme-preview-message-secondary"></div>
                      <div class="theme-preview-input"></div>
                    </div>
                  </div>
                </div>
              </button>
            </div>
          </section>

          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">外观说明</h3>
                <p class="settings-section-copy">主题切换只影响 UI 呈现，不会触发任务、上下文或 provider 的运行时变更。</p>
              </div>
            </div>

            <div class="grid gap-3 md:grid-cols-3">
              <article class="settings-info-card">
                <p class="settings-info-label">持久化</p>
                <p class="settings-info-value">保存在当前设备本地</p>
              </article>
              <article class="settings-info-card">
                <p class="settings-info-label">生效方式</p>
                <p class="settings-info-value">即时切换，无需重启</p>
              </article>
              <article class="settings-info-card">
                <p class="settings-info-label">默认主题</p>
                <p class="settings-info-value">深色，保持当前视觉延续</p>
              </article>
            </div>
          </section>
        </div>

        <div v-else-if="activeSection === 'providers'" class="settings-grid">
          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">Providers</h3>
                <p class="settings-section-copy">全局配置，保存在用户目录下。</p>
              </div>
              <Button variant="outline" size="sm" @click="startCreate">新增 Provider</Button>
            </div>

            <div v-if="settings?.providers.length" class="space-y-3">
              <article
                v-for="provider in settings.providers"
                :key="provider.id"
                class="settings-provider-card"
                :class="provider.id === activeEditorId ? 'settings-provider-card-active' : ''"
              >
                <div class="flex items-start justify-between gap-3">
                  <div class="min-w-0">
                    <div class="flex items-center gap-2">
                      <h4 class="truncate text-[14px] font-medium text-text">{{ provider.name }}</h4>
                      <span v-if="provider.id === settings.defaultProviderId" class="settings-default-badge">默认</span>
                    </div>
                    <p class="mt-1 text-[11px] uppercase tracking-[0.12em] text-text-dim">{{ providerTypeLabel(provider.providerType) }}</p>
                    <p v-if="provider.baseUrl" class="mt-1 truncate font-mono text-[11px] text-text-dim">{{ provider.baseUrl }}</p>
                    <p class="mt-2 text-[12px] text-text-muted">Key: {{ provider.apiKeyHint }}</p>
                  </div>
                  <div class="flex shrink-0 items-center gap-1">
                    <Button variant="ghost" size="sm" @click="startEdit(provider)">编辑</Button>
                    <Button variant="ghost" size="sm" class="text-[#d44a4a] hover:text-[#d44a4a]" @click="emit('deleteProvider', provider.id)">
                      删除
                    </Button>
                  </div>
                </div>
              </article>
            </div>

            <div v-else class="settings-empty">
              还没有配置 provider。先新增一个 provider 类型和凭据，后面模型选择器就能接上它。
            </div>
          </section>

          <section class="settings-panel">
            <div class="settings-panel-header">
              <div>
                <h3 class="settings-section-title">{{ activeEditorId ? '编辑 Provider' : '新增 Provider' }}</h3>
                <p class="settings-section-copy">这里只负责维护单个 provider 的接入信息，不再承载全局默认模型配置。</p>
              </div>
            </div>

            <form class="space-y-4" @submit.prevent="submitProvider">
              <div class="dialog-field">
                <label class="dialog-label" for="provider-type">类型</label>
                <SettingsSelect v-model="providerType" :options="providerTypeOptions" placeholder="请选择 provider 类型" />
              </div>
              <div class="dialog-field">
                <label class="dialog-label" for="provider-name">名称</label>
                <Input id="provider-name" v-model="providerName" :placeholder="providerNamePlaceholder" />
              </div>
              <div class="dialog-field">
                <label class="dialog-label" for="provider-base-url">Base URL</label>
                <Input
                  id="provider-base-url"
                  v-model="providerBaseUrl"
                  :placeholder="baseUrlPlaceholder"
                />
                <p class="dialog-hint">
                  {{ baseUrlHint }}
                </p>
              </div>
              <div class="dialog-field">
                <label class="dialog-label" for="provider-api-key">API Key</label>
                <Input
                  id="provider-api-key"
                  v-model="providerApiKey"
                  type="password"
                  :placeholder="apiKeyPlaceholder"
                />
              </div>
              <div class="dialog-field">
                <div class="flex items-center justify-between gap-3">
                  <label class="dialog-label" for="provider-probe-model">Probe Model</label>
                  <Button
                    variant="ghost"
                    size="sm"
                    type="button"
                    :disabled="busy || probeModelsLoading"
                    @click="requestProbeModelsNow"
                  >
                    {{ probeModelsLoading ? '读取中…' : '刷新列表' }}
                  </Button>
                </div>
                <template v-if="probeModels.length">
                  <SettingsSelect
                    v-model="providerProbeModel"
                    :options="probeModelOptions"
                    placeholder="从供应商模型列表中选择"
                    searchable
                    search-placeholder="搜索 probe model…"
                  />
                </template>
                <template v-else>
                  <Input
                    id="provider-probe-model"
                    v-model="providerProbeModel"
                    :placeholder="probeModelPlaceholder"
                  />
                </template>
                <Input
                  v-if="probeModels.length"
                  v-model="providerProbeModel"
                  class="mt-2"
                  :placeholder="probeModelPlaceholder"
                />
                <div v-if="!probeModels.length && probeSuggestedModels.length" class="mt-2 flex flex-wrap gap-2">
                  <button
                    v-for="model in probeSuggestedModels"
                    :key="model"
                    type="button"
                    class="rounded-full border border-[color:var(--ma-line-soft)] px-2.5 py-1 text-[11px] text-text-dim transition hover:bg-bg-hover hover:text-text"
                    @click="providerProbeModel = model"
                  >
                    {{ model }}
                  </button>
                </div>
                <p class="dialog-hint">
                  优先展示供应商 `/models` 返回的可搜索列表；若接口没返回数据，或你想测试一个未列出的模型，也可以继续手动填写。
                </p>
              </div>
              <div class="flex items-center justify-end gap-2">
                <Button variant="outline" type="button" :disabled="busy" @click="testProvider">
                  测试连通性
                </Button>
                <Button variant="ghost" type="button" @click="resetForm">清空</Button>
                <Button type="submit" :disabled="busy">{{ activeEditorId ? '保存修改' : '创建 Provider' }}</Button>
              </div>
              <p v-if="props.providerTestMessage" class="text-[12px]" :class="props.providerTestSuccess ? 'text-success' : 'text-error'">
                {{ props.providerTestMessage }}
              </p>
            </form>
          </section>
        </div>

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
import checkIcon from '@iconify-icons/lucide/check';
import moonIcon from '@iconify-icons/lucide/moon-star';
import slidersHorizontalIcon from '@iconify-icons/lucide/sliders-horizontal';
import serverIcon from '@iconify-icons/lucide/server-cog';
import sunIcon from '@iconify-icons/lucide/sun-medium';
import xIcon from '@iconify-icons/lucide/x';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { ThemeMode } from '@/composables/useAppearanceSettings';
import type { ProviderSettingsView } from '@/data/mock';
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
  testProvider: [input: { id?: number; providerType: string; name: string; baseUrl: string; apiKey: string; probeModel?: string }];
  deleteProvider: [providerId: number];
  saveDefaultProvider: [input: { providerId: number; model: string }];
  requestModels: [providerId: number];
  requestProbeModels: [input: { id?: number; providerType: string; baseUrl: string; apiKey: string; probeModel?: string }];
}>();

const activeSection = ref<'appearance' | 'providers' | 'defaults'>('appearance');
const activeEditorId = ref<number | null>(null);
const providerType = ref('openai_compat');
const providerName = ref('');
const providerBaseUrl = ref('');
const providerApiKey = ref('');
const providerProbeModel = ref('');
const defaultProviderIdString = ref('');
const defaultModelLocal = ref('');

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
  activeEditorId.value = null;
  providerType.value = 'openai_compat';
  providerName.value = '';
  providerBaseUrl.value = '';
  providerApiKey.value = '';
  providerProbeModel.value = '';
}

function startEdit(provider: ProviderSettingsView['providers'][number]) {
  activeSection.value = 'providers';
  activeEditorId.value = provider.id;
  providerType.value = provider.providerType;
  providerName.value = provider.name;
  providerBaseUrl.value = provider.baseUrl ?? '';
  providerApiKey.value = '';
  providerProbeModel.value = '';
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

function submitProvider() {
  emit('saveProvider', {
    id: activeEditorId.value ?? undefined,
    providerType: providerType.value,
    name: providerName.value,
    baseUrl: providerBaseUrl.value,
    apiKey: providerApiKey.value,
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
</script>
