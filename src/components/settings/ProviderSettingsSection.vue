<template>
  <div class="settings-grid">
    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">模型</h3>
          <p class="settings-section-copy">这里展示已经可用的运行模型。模型是日常选择入口，连接通道只负责提供来源。</p>
        </div>
        <Button v-if="activeEditorProvider" variant="outline" size="sm" type="button" @click="emit('startCreateProviderModel')">
          接入到当前通道
        </Button>
      </div>

      <div v-if="allModels.length" class="space-y-3">
        <article
          v-for="model in allModels"
          :key="model.id"
          class="settings-provider-card"
          :class="activeProviderModelId === model.id ? 'settings-provider-card-active' : ''"
        >
          <div class="flex items-start justify-between gap-3">
            <div class="min-w-0">
              <div class="flex items-center gap-2">
                <h4 class="truncate text-[14px] font-medium text-text">{{ model.displayName || model.modelId }}</h4>
                <span
                  v-if="model.id === settings?.defaultModelConfigId"
                  class="settings-default-badge"
                >
                  默认
                </span>
              </div>
              <p class="mt-1 text-[11px] uppercase tracking-[0.12em] text-text-dim">
                {{ model.providerName }} · {{ providerTypeLabel(model.providerType) }}
              </p>
              <p class="mt-2 text-[12px] text-text-muted">{{ formatCapabilitiesSummary(model.capabilities) }}</p>
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <Button variant="ghost" size="sm" type="button" @click="startEditModel(model)">定位到通道</Button>
              <Button variant="ghost" size="sm" type="button" class="text-[#d44a4a] hover:text-[#d44a4a]" @click="emit('deleteProviderModel', model.id)">
                删除
              </Button>
            </div>
          </div>
        </article>
      </div>

      <div v-else class="settings-empty">
        还没有激活任何模型。先在右侧新增连接通道，再为它添加或扫描模型。
      </div>
    </section>

    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">连接通道</h3>
          <p class="settings-section-copy">连接通道只保存凭据和端点；模型能力与默认选择都从模型实体出发。</p>
        </div>
        <Button variant="outline" size="sm" @click="emit('startCreate')">新增通道</Button>
      </div>

      <div v-if="settings?.providers.length" class="mb-6 space-y-3">
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
                <span class="rounded-full bg-bg-hover px-2 py-0.5 text-[10px] uppercase tracking-[0.12em] text-text-dim">
                  {{ provider.models.length ? `${provider.models.length} 个模型` : '未接入模型' }}
                </span>
              </div>
              <p class="mt-1 text-[11px] uppercase tracking-[0.12em] text-text-dim">{{ providerTypeLabel(provider.providerType) }}</p>
              <p v-if="provider.baseUrl" class="mt-1 truncate font-mono text-[11px] text-text-dim">{{ provider.baseUrl }}</p>
              <p class="mt-2 text-[12px] text-text-muted">Key: {{ provider.apiKeyHint }}</p>
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <Button variant="ghost" size="sm" @click="emit('startEdit', provider)">管理模型</Button>
              <Button variant="ghost" size="sm" class="text-[#d44a4a] hover:text-[#d44a4a]" @click="emit('deleteProvider', provider.id)">
                删除
              </Button>
            </div>
          </div>
        </article>
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
            <h3 class="settings-section-title">通道接入</h3>
            <p class="settings-section-copy">当前正在管理 {{ activeEditorProvider.name }}。先选择模型 ID，再手动确认能力，最后保存到左侧模型列表。</p>
          </div>
          <Button variant="outline" size="sm" type="button" @click="emit('startCreateProviderModel')">
            添加模型
          </Button>
        </div>

        <div class="mb-4 rounded-2xl border border-[color:var(--ma-line-soft)] bg-bg-hover/40 px-4 py-3 text-[12px] text-text-muted">
          {{
            activeEditorProvider.models.length
              ? `这个通道当前已接入 ${activeEditorProvider.models.length} 个模型。左侧“模型”列表展示全局入口；这里负责把模型接到该通道上。`
              : '这个通道还没有接入模型。可以先扫描模型列表，再把需要的模型写入本地设置库。'
          }}
        </div>

        <form class="mt-4 space-y-4" @submit.prevent="emit('submitProviderModel')">
          <div class="grid gap-4 md:grid-cols-2">
            <div class="dialog-field">
              <label class="dialog-label" for="provider-model-id">模型 ID</label>
              <template v-if="providerModelIdOptions.length">
                <SettingsSelect
                  v-model="providerModelId"
                  :options="providerModelIdOptions"
                  placeholder="从已缓存或已配置模型中选择"
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

          <div class="rounded-2xl border border-[color:var(--ma-line-soft)] bg-bg-hover/30 px-4 py-3">
            <p class="text-[12px] font-medium text-text">能力确认</p>
            <p class="mt-1 text-[11px] leading-5 text-text-dim">这里直接维护工具、多模态和 server-side tools 配置，不再额外做自动能力探测。</p>
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
                    @change="emit('serverToolToggle', tool.capability, ($event.target as HTMLInputElement | null)?.checked ?? false)"
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
  serverToolToggle: [capability: string, enabled: boolean];
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
  set: (value: string | number) => emit('update:providerModelContextWindow', String(value)),
});
const providerModelMaxOutputTokens = computed({
  get: () => props.providerModelMaxOutputTokens,
  set: (value: string | number) => emit('update:providerModelMaxOutputTokens', String(value)),
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

const allModels = computed(() =>
  (props.settings?.providers ?? []).flatMap((provider) =>
    provider.models.map((model) => ({
      ...model,
      providerName: provider.name,
      providerType: provider.providerType,
      providerId: provider.id,
    })),
  ),
);

function startEditModel(model: (typeof allModels.value)[number]) {
  const provider = props.settings?.providers.find((entry) => entry.id === model.providerId);
  if (!provider) {
    return;
  }
  emit('startEdit', provider);
  emit('startEditProviderModel', model);
}
</script>
