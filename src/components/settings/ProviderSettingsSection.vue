<template>
  <div class="settings-grid">
    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">Providers</h3>
          <p class="settings-section-copy">全局配置，保存在用户目录下。</p>
        </div>
        <Button variant="outline" size="sm" @click="emit('startCreate')">新增 Provider</Button>
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
              <Button variant="ghost" size="sm" @click="emit('startEdit', provider)">编辑</Button>
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

      <form class="space-y-4" @submit.prevent="emit('submitProvider')">
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
              @click="emit('requestProbeModelsNow')"
            >
              {{ probeModelsLoading ? '读取中…' : '刷新列表' }}
            </Button>
          </div>
          <Input
            id="provider-probe-model"
            v-model="providerProbeModel"
            :placeholder="probeModelPlaceholder"
          />
          <SettingsSelect
            v-model="providerProbeModel"
            class="mt-2"
            :options="probeModelOptions"
            :placeholder="probeModelSelectPlaceholder"
            :disabled="busy || !probeModels.length"
            searchable
            search-placeholder="搜索 probe model…"
          />
          <div
            v-if="!probeModels.length && probeModelsLoading"
            class="mt-2 text-[11px] text-text-dim"
          >
            正在读取供应商模型列表，读取完成后可直接从下拉中选择。
          </div>
          <div
            v-else-if="!probeModels.length && probeSuggestedModels.length"
            class="mt-2 flex flex-wrap gap-2"
          >
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
          <Button variant="outline" type="button" :disabled="busy" @click="emit('testProvider')">
            测试连通性
          </Button>
          <Button variant="ghost" type="button" @click="emit('resetForm')">清空</Button>
          <Button type="submit" :disabled="busy">{{ activeEditorId ? '保存修改' : '创建 Provider' }}</Button>
        </div>
        <p v-if="providerTestMessage" class="text-[12px]" :class="providerTestSuccess ? 'text-success' : 'text-error'">
          {{ providerTestMessage }}
        </p>
      </form>

      <div v-if="activeEditorProvider" class="mt-6 border-t border-[color:var(--ma-line-soft)] pt-5">
        <div class="settings-panel-header">
          <div>
            <h3 class="settings-section-title">模型能力</h3>
            <p class="settings-section-copy">这里维护该 provider 下的模型能力。OpenAI-compatible 依赖这份配置决定图片入口、工具能力与上下文预算；已知 provider 也可以在这里补充或覆盖新模型。</p>
          </div>
          <Button variant="outline" size="sm" type="button" @click="emit('startCreateProviderModel')">
            添加模型
          </Button>
        </div>

        <div v-if="activeEditorProvider.models.length" class="space-y-3">
          <article
            v-for="model in activeEditorProvider.models"
            :key="model.id"
            class="settings-provider-card"
            :class="activeProviderModelId === model.id ? 'settings-provider-card-active' : ''"
          >
            <div class="flex items-start justify-between gap-3">
              <div class="min-w-0">
                <div class="flex items-center gap-2">
                  <h4 class="truncate text-[14px] font-medium text-text">{{ model.displayName || model.modelId }}</h4>
                  <span class="rounded-full bg-bg-hover px-2 py-0.5 text-[10px] uppercase tracking-[0.12em] text-text-dim">{{ model.modelId }}</span>
                </div>
                <p class="mt-2 text-[12px] text-text-muted">
                  {{ formatCapabilitiesSummary(model.capabilities) }}
                </p>
              </div>
              <div class="flex shrink-0 items-center gap-1">
                <Button variant="ghost" size="sm" type="button" @click="emit('startEditProviderModel', model)">编辑</Button>
                <Button variant="ghost" size="sm" type="button" class="text-[#d44a4a] hover:text-[#d44a4a]" @click="emit('deleteProviderModel', model.id)">
                  删除
                </Button>
              </div>
            </div>
          </article>
        </div>
        <div v-else class="settings-empty">
          这个 provider 还没有单独配置模型能力。
        </div>

        <form class="mt-4 space-y-4" @submit.prevent="emit('submitProviderModel')">
          <div class="grid gap-4 md:grid-cols-2">
            <div class="dialog-field">
              <label class="dialog-label" for="provider-model-id">模型 ID</label>
              <template v-if="providerModelIdOptions.length">
                <SettingsSelect
                  v-model="providerModelId"
                  :options="providerModelIdOptions"
                  placeholder="从已探测或已配置模型中选择"
                  searchable
                  search-placeholder="搜索模型 ID…"
                />
              </template>
              <template v-else>
                <Input id="provider-model-id" v-model="providerModelId" placeholder="gpt-4o-mini / qwen2.5-coder:32b" />
              </template>
              <Input
                v-if="providerModelIdOptions.length"
                v-model="providerModelId"
                class="mt-2"
                placeholder="也可以直接手填新的 model_id"
              />
            </div>
            <div class="dialog-field">
              <label class="dialog-label" for="provider-model-display-name">显示名称</label>
              <Input id="provider-model-display-name" v-model="providerModelDisplayName" placeholder="可选，留空则界面显示 model_id" />
            </div>
            <div class="dialog-field">
              <label class="dialog-label" for="provider-model-context-window">上下文窗口</label>
              <Input id="provider-model-context-window" v-model="providerModelContextWindow" type="number" min="1" />
            </div>
            <div class="dialog-field">
              <label class="dialog-label" for="provider-model-max-output">最大输出</label>
              <Input id="provider-model-max-output" v-model="providerModelMaxOutputTokens" type="number" min="1" />
            </div>
          </div>

          <div v-if="providerModelIdSuggestions.length" class="flex flex-wrap gap-2">
            <button
              v-for="model in providerModelIdSuggestions"
              :key="model"
              type="button"
              class="rounded-full border border-[color:var(--ma-line-soft)] px-2.5 py-1 text-[11px] text-text-dim transition hover:bg-bg-hover hover:text-text"
              @click="providerModelId = model"
            >
              {{ model }}
            </button>
          </div>

          <div class="dialog-field">
            <label class="dialog-label">能力</label>
            <div class="grid gap-3 md:grid-cols-2">
              <label class="flex items-center gap-2 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-2 text-[12px] text-text">
                <input v-model="providerModelSupportsToolUse" type="checkbox" />
                <span>工具调用</span>
              </label>
              <label class="flex items-center gap-2 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-2 text-[12px] text-text">
                <input v-model="providerModelSupportsVision" type="checkbox" />
                <span>图片输入</span>
              </label>
              <label class="flex items-center gap-2 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-2 text-[12px] text-text">
                <input v-model="providerModelSupportsAudio" type="checkbox" />
                <span>音频输入</span>
              </label>
              <label class="flex items-center gap-2 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-2 text-[12px] text-text">
                <input v-model="providerModelSupportsPdf" type="checkbox" />
                <span>PDF 输入</span>
              </label>
            </div>
          </div>

          <div class="dialog-field">
            <label class="dialog-label">Server-side Tools</label>
            <div class="space-y-3">
              <div
                v-for="tool in serverToolDefinitions"
                :key="tool.capability"
                class="grid gap-3 rounded-2xl border border-[color:var(--ma-line-soft)] px-3 py-3 md:grid-cols-[minmax(0,1fr)_220px]"
              >
                <label class="flex items-center gap-2 text-[12px] text-text">
                  <input
                    :checked="isServerToolEnabled(tool.capability)"
                    type="checkbox"
                    @change="emit('serverToolToggle', tool.capability, $event)"
                  />
                  <span>{{ tool.label }}</span>
                </label>
                <SettingsSelect
                  :model-value="providerModelServerTools[tool.capability] ?? ''"
                  :options="serverToolFormatOptions(tool.capability)"
                  placeholder="选择格式"
                  :disabled="!isServerToolEnabled(tool.capability)"
                  @update:model-value="emit('setServerToolFormat', tool.capability, $event)"
                />
              </div>
            </div>
            <p class="dialog-hint">
              这些工具由 provider 侧执行，March 只负责保存能力配置并在后续请求翻译层中注入对应定义。
            </p>
          </div>

          <div class="flex items-center justify-end gap-2">
            <Button variant="ghost" type="button" @click="emit('resetProviderModelForm')">清空</Button>
            <Button type="submit" :disabled="busy || !providerModelId.trim()">
              {{ activeProviderModelId ? '保存模型能力' : '添加模型能力' }}
            </Button>
          </div>
        </form>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import SettingsSelect from '@/components/SettingsSelect.vue';
import type { ProviderSettingsView } from '@/data/mock';

const props = defineProps<{
  settings: ProviderSettingsView | null;
  busy?: boolean;
  probeModels: string[];
  probeSuggestedModels: string[];
  probeModelsLoading?: boolean;
  providerTestMessage?: string;
  providerTestSuccess?: boolean;
  activeEditorId: number | null;
  activeProviderModelId: number | null;
  providerType: string;
  providerName: string;
  providerBaseUrl: string;
  providerApiKey: string;
  providerProbeModel: string;
  providerTypeOptions: Array<{ value: string; label: string }>;
  providerNamePlaceholder: string;
  baseUrlPlaceholder: string;
  baseUrlHint: string;
  apiKeyPlaceholder: string;
  probeModelPlaceholder: string;
  probeModelSelectPlaceholder: string;
  providerModelId: string;
  providerModelDisplayName: string;
  providerModelContextWindow: string;
  providerModelMaxOutputTokens: string;
  providerModelSupportsToolUse: boolean;
  providerModelSupportsVision: boolean;
  providerModelSupportsAudio: boolean;
  providerModelSupportsPdf: boolean;
  providerModelServerTools: Record<string, string>;
  providerModelIdOptions: Array<{ value: string; label: string }>;
  providerModelIdSuggestions: string[];
  serverToolDefinitions: ReadonlyArray<{ capability: string; label: string; formats: readonly string[] }>;
  serverToolFormatOptions: (capability: string) => Array<{ value: string; label: string }>;
  isServerToolEnabled: (capability: string) => boolean;
  providerTypeLabel: (providerTypeValue: string) => string;
  formatCapabilitiesSummary: (capabilities: ProviderSettingsView['providers'][number]['models'][number]['capabilities']) => string;
}>();

const emit = defineEmits<{
  startCreate: [];
  startEdit: [provider: ProviderSettingsView['providers'][number]];
  deleteProvider: [providerId: number];
  'update:providerType': [value: string];
  'update:providerName': [value: string];
  'update:providerBaseUrl': [value: string];
  'update:providerApiKey': [value: string];
  'update:providerProbeModel': [value: string];
  'update:providerModelId': [value: string];
  'update:providerModelDisplayName': [value: string];
  'update:providerModelContextWindow': [value: string];
  'update:providerModelMaxOutputTokens': [value: string];
  'update:providerModelSupportsToolUse': [value: boolean];
  'update:providerModelSupportsVision': [value: boolean];
  'update:providerModelSupportsAudio': [value: boolean];
  'update:providerModelSupportsPdf': [value: boolean];
  submitProvider: [];
  testProvider: [];
  resetForm: [];
  requestProbeModelsNow: [];
  startCreateProviderModel: [];
  startEditProviderModel: [model: NonNullable<ProviderSettingsView['providers'][number]['models'][number]>];
  deleteProviderModel: [providerModelId: number];
  submitProviderModel: [];
  resetProviderModelForm: [];
  serverToolToggle: [capability: string, event: Event];
  setServerToolFormat: [capability: string, format: string];
}>();

const providerType = computed({
  get: () => props.providerType,
  set: (value: string) => emit('update:providerType', value),
});

const providerName = computed({
  get: () => props.providerName,
  set: (value: string) => emit('update:providerName', value),
});
const providerBaseUrl = computed({
  get: () => props.providerBaseUrl,
  set: (value: string) => emit('update:providerBaseUrl', value),
});
const providerApiKey = computed({
  get: () => props.providerApiKey,
  set: (value: string) => emit('update:providerApiKey', value),
});
const providerProbeModel = computed({
  get: () => props.providerProbeModel,
  set: (value: string) => emit('update:providerProbeModel', value),
});
const providerModelId = computed({
  get: () => props.providerModelId,
  set: (value: string) => emit('update:providerModelId', value),
});
const providerModelDisplayName = computed({
  get: () => props.providerModelDisplayName,
  set: (value: string) => emit('update:providerModelDisplayName', value),
});
const providerModelContextWindow = computed({
  get: () => props.providerModelContextWindow,
  set: (value: string) => emit('update:providerModelContextWindow', value),
});
const providerModelMaxOutputTokens = computed({
  get: () => props.providerModelMaxOutputTokens,
  set: (value: string) => emit('update:providerModelMaxOutputTokens', value),
});
const providerModelSupportsToolUse = computed({
  get: () => props.providerModelSupportsToolUse,
  set: (value: boolean) => emit('update:providerModelSupportsToolUse', value),
});
const providerModelSupportsVision = computed({
  get: () => props.providerModelSupportsVision,
  set: (value: boolean) => emit('update:providerModelSupportsVision', value),
});
const providerModelSupportsAudio = computed({
  get: () => props.providerModelSupportsAudio,
  set: (value: boolean) => emit('update:providerModelSupportsAudio', value),
});
const providerModelSupportsPdf = computed({
  get: () => props.providerModelSupportsPdf,
  set: (value: boolean) => emit('update:providerModelSupportsPdf', value),
});

const probeModelOptions = computed(() =>
  props.probeModels.map((model) => ({
    value: model,
    label: model,
  })),
);

const activeEditorProvider = computed(() =>
  props.settings?.providers.find((provider) => provider.id === props.activeEditorId) ?? null,
);
</script>
