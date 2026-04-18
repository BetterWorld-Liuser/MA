<template>
  <div :class="showEditor ? 'settings-grid' : 'space-y-5'">
    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">已有模型</h3>
          <p class="settings-section-copy">左侧只展示已经接入的运行模型。新建或编辑时，右侧再展开模型能力面板。</p>
        </div>
        <Button v-if="providerOptions.length" variant="outline" size="sm" type="button" @click="emit('startCreateProviderModel')">
          新建模型
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
              <Button
                v-if="model.id !== settings?.defaultModelConfigId"
                variant="ghost"
                size="icon-sm"
                type="button"
                title="设为默认模型"
                aria-label="设为默认模型"
                @click="emit('saveDefaultModel', model.id)"
              >
                <Icon :icon="starIcon" class="h-4 w-4" />
              </Button>
              <Button variant="ghost" size="sm" type="button" @click="startEditModel(model)">编辑能力</Button>
              <Button variant="ghost" size="sm" type="button" class="text-[color:var(--destructive-text)] hover:text-[color:var(--destructive-text)]" @click="emit('deleteProviderModel', model.id)">
                删除
              </Button>
            </div>
          </div>
        </article>
      </div>

      <div v-else class="settings-empty">
        还没有接入任何模型。先到“供应商”页配置来源通道，再点击右上角“新建模型”展开右侧面板。
      </div>
    </section>

    <section v-if="showEditor" class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">{{ activeProviderModelId ? '编辑模型能力' : '新建模型' }}</h3>
          <p class="settings-section-copy">先选择供应商，再选择模型 ID，最后确认能力。模型挂在哪个供应商下面，在这里直接决定。</p>
        </div>
        <Button variant="ghost" size="sm" type="button" @click="emit('closeEditor')">收起</Button>
      </div>

      <form class="space-y-4" @submit.prevent="emit('submitProviderModel')">
        <div class="grid gap-4 md:grid-cols-2">
          <div class="dialog-field">
            <label class="dialog-label">来源供应商</label>
            <SettingsSelect
              v-model="selectedProviderIdString"
              :options="providerOptions"
              placeholder="先选择供应商"
            />
            <p class="dialog-hint">协议跟着所选供应商走，这里不单独编辑。</p>
          </div>
          <div class="dialog-field">
            <label class="dialog-label" for="provider-model-id">模型 ID</label>
            <SettingsCombobox
              id="provider-model-id"
              v-model="providerModelId"
              :options="providerModelIdOptions"
              placeholder="gpt-4o-mini / qwen2.5-coder:32b"
              empty-text="没有匹配的缓存模型，继续输入即可新建"
            />
          </div>
          <div class="dialog-field">
            <label class="dialog-label" for="provider-model-context-window">上下文窗口</label>
            <Input id="provider-model-context-window" v-model="providerModelContextWindow" type="number" min="1" />
          </div>
          <div class="dialog-field">
            <label class="dialog-label" for="provider-model-max-output">最大输出</label>
            <Input id="provider-model-max-output" v-model="providerModelMaxOutputTokens" type="number" min="1" />
          </div>
          <div class="dialog-field md:col-span-2">
            <label class="dialog-label" for="provider-model-display-name">显示名称</label>
            <Input id="provider-model-display-name" v-model="providerModelDisplayName" placeholder="可选，留空则界面显示 model_id" />
          </div>
        </div>

        <div class="dialog-field">
          <div class="rounded-2xl border border-[color:var(--ma-line-soft)] bg-bg-hover/30 px-4 py-3">
            <p class="text-[12px] font-medium text-text">能力确认</p>
            <p class="mt-1 text-[11px] leading-5 text-text-dim">
              这里直接维护模型能力画像。供应商页只负责类型与连通性，单个模型支持哪些能力由你在这里确认并保存。
            </p>
          </div>
        </div>

        <div class="dialog-field">
          <label class="dialog-label">能力</label>
          <div class="flex flex-wrap gap-2">
            <button
              v-for="capability in capabilityOptions"
              :key="capability.key"
              type="button"
              class="inline-flex items-center gap-1.5 rounded-full border px-3 py-1.5 text-[12px] transition"
              :class="capability.enabled
                ? 'border-[color:var(--selection-accent-border)] bg-accent-dim text-accent'
                : 'border-[color:var(--ma-line-soft)] bg-bg-hover/35 text-text-dim hover:bg-bg-hover hover:text-text'"
              :aria-pressed="capability.enabled"
              @click="capability.toggle()"
            >
              <Icon :icon="capability.icon" class="h-3.5 w-3.5 shrink-0" />
              <span>{{ capability.label }}</span>
            </button>
          </div>
        </div>

        <div class="dialog-field">
          <div class="flex items-center justify-between gap-3">
            <label class="dialog-label">Server-side Tools</label>
          </div>
          <div class="space-y-3 rounded-2xl border border-[color:var(--ma-line-soft)] bg-bg-hover/20 px-4 py-3">
            <div class="flex flex-wrap gap-2">
              <button
                v-for="tool in serverToolOptions"
                :key="tool.capability"
                type="button"
                class="inline-flex items-center gap-1.5 rounded-full border px-3 py-1.5 text-[12px] transition"
                :class="tool.enabled
                  ? 'border-[color:var(--selection-accent-border)] bg-accent-dim text-accent'
                  : 'border-[color:var(--ma-line-soft)] bg-bg-hover/35 text-text-dim hover:bg-bg-hover hover:text-text'"
                :aria-pressed="tool.enabled"
                @click="emitServerToolToggle(tool.capability, !tool.enabled)"
              >
                <Icon :icon="tool.icon" class="h-3.5 w-3.5 shrink-0" />
                <span>{{ tool.label }}</span>
              </button>
            </div>

            <div v-if="enabledServerToolOptions.length" class="space-y-3">
              <div
                v-for="tool in enabledServerToolOptions"
                :key="tool.capability"
                class="grid gap-3 rounded-2xl border border-[color:var(--ma-line-soft)] bg-bg-hover/20 px-3 py-3 md:grid-cols-[minmax(0,1fr)_220px]"
              >
                <div class="flex items-center gap-2 text-[12px] text-text">
                  <Icon :icon="tool.icon" class="h-3.5 w-3.5 shrink-0 text-accent" />
                  <span>{{ tool.label }}</span>
                </div>
                <SettingsSelect
                  :model-value="providerModelServerTools[tool.capability] ?? ''"
                  :options="serverToolFormatOptions(tool.capability)"
                  placeholder="选择格式"
                  @update:model-value="emit('setServerToolFormat', tool.capability, $event)"
                />
              </div>
            </div>
          </div>
          <p class="dialog-hint">
            这些工具由 provider 侧执行；这里保存的是该模型允许注入哪些 server-side tool 定义。
          </p>
        </div>

        <div class="flex items-center justify-end gap-2">
          <Button variant="ghost" type="button" @click="emit('resetProviderModelForm')">清空</Button>
          <Button type="submit" :disabled="busy || !providerModelId.trim()">
            {{ activeProviderModelId ? '保存模型' : '添加模型' }}
          </Button>
        </div>
      </form>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue';
import { Icon } from '@iconify/vue';
import eyeIcon from '@iconify-icons/lucide/eye';
import fileTextIcon from '@iconify-icons/lucide/file-text';
import folderSearchIcon from '@iconify-icons/lucide/folder-search-2';
import globeIcon from '@iconify-icons/lucide/globe';
import musicIcon from '@iconify-icons/lucide/music-4';
import starIcon from '@iconify-icons/lucide/star';
import terminalSquareIcon from '@iconify-icons/lucide/terminal-square';
import wrenchIcon from '@iconify-icons/lucide/wrench';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { ProviderSettingsView } from '@/data/mock';
import SettingsCombobox from '@/components/SettingsCombobox.vue';
import SettingsSelect from '@/components/SettingsSelect.vue';

const props = defineProps<{
  settings: ProviderSettingsView | null;
  busy?: boolean;
  showEditor?: boolean;
  activeEditorId: number | null;
  activeProviderModelId: number | null;
  providerOptions: Array<{ value: string; label: string }>;
  selectedProviderIdString: string;
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
  serverToolDefinitions: ReadonlyArray<{ capability: string; label: string; formats: readonly string[] }>;
  serverToolFormatOptions: (capability: string) => Array<{ value: string; label: string }>;
  isServerToolEnabled: (capability: string) => boolean;
  providerTypeLabel: (providerTypeValue: string) => string;
  formatCapabilitiesSummary: (capabilities: ProviderSettingsView['providers'][number]['models'][number]['capabilities']) => string;
}>();

const emit = defineEmits<{
  startCreateProviderModel: [];
  saveDefaultModel: [modelConfigId: number];
  startEdit: [provider: ProviderSettingsView['providers'][number]];
  startEditProviderModel: [model: NonNullable<ProviderSettingsView['providers'][number]['models'][number]>];
  deleteProviderModel: [providerModelId: number];
  'update:selectedProviderIdString': [value: string];
  'update:providerModelId': [value: string];
  'update:providerModelDisplayName': [value: string];
  'update:providerModelContextWindow': [value: string];
  'update:providerModelMaxOutputTokens': [value: string];
  'update:providerModelSupportsToolUse': [value: boolean];
  'update:providerModelSupportsVision': [value: boolean];
  'update:providerModelSupportsAudio': [value: boolean];
  'update:providerModelSupportsPdf': [value: boolean];
  submitProviderModel: [];
  resetProviderModelForm: [];
  closeEditor: [];
  serverToolToggle: [capability: string, enabled: boolean];
  setServerToolFormat: [capability: string, format: string];
}>();

const selectedProviderIdString = computed({
  get: () => props.selectedProviderIdString,
  set: (value: string) => emit('update:selectedProviderIdString', value),
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

const selectedProvider = computed(() => {
  const providerId = Number(props.selectedProviderIdString);
  if (!Number.isFinite(providerId) || providerId <= 0) {
    return null;
  }
  return props.settings?.providers.find((provider) => provider.id === providerId) ?? null;
});

const capabilityOptions = computed(() => [
  {
    key: 'vision',
    label: '视觉',
    icon: eyeIcon,
    enabled: providerModelSupportsVision.value,
    toggle: () => {
      providerModelSupportsVision.value = !providerModelSupportsVision.value;
    },
  },
  {
    key: 'tool_use',
    label: '工具',
    icon: wrenchIcon,
    enabled: providerModelSupportsToolUse.value,
    toggle: () => {
      providerModelSupportsToolUse.value = !providerModelSupportsToolUse.value;
    },
  },
  {
    key: 'audio',
    label: '音频',
    icon: musicIcon,
    enabled: providerModelSupportsAudio.value,
    toggle: () => {
      providerModelSupportsAudio.value = !providerModelSupportsAudio.value;
    },
  },
  {
    key: 'pdf',
    label: 'PDF',
    icon: fileTextIcon,
    enabled: providerModelSupportsPdf.value,
    toggle: () => {
      providerModelSupportsPdf.value = !providerModelSupportsPdf.value;
    },
  },
]);

const serverToolOptions = computed(() =>
  props.serverToolDefinitions.map((tool) => ({
    capability: tool.capability,
    label: tool.label,
    enabled: props.isServerToolEnabled(tool.capability),
    icon: serverToolIcon(tool.capability),
  })),
);

const enabledServerToolOptions = computed(() =>
  serverToolOptions.value.filter((tool) => tool.enabled),
);

function startEditModel(model: (typeof allModels.value)[number]) {
  const provider = props.settings?.providers.find((entry) => entry.id === model.providerId);
  if (!provider) {
    return;
  }
  emit('startEdit', provider);
  emit('startEditProviderModel', model);
}

function emitServerToolToggle(capability: string, enabled: boolean) {
  emit('serverToolToggle', capability, enabled);
}

function serverToolIcon(capability: string) {
  if (capability === 'web_search') {
    return globeIcon;
  }
  if (capability === 'code_execution') {
    return terminalSquareIcon;
  }
  if (capability === 'file_search') {
    return folderSearchIcon;
  }
  return wrenchIcon;
}
</script>
