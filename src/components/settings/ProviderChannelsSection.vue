<template>
  <div :class="showEditor ? 'settings-grid' : 'space-y-5'">
    <section class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">已有供应商</h3>
          <p class="settings-section-copy">左侧只保留已配置供应商列表。新增或编辑时，右侧再展开对应表单。</p>
        </div>
        <Button variant="outline" size="sm" @click="emit('startCreate')">新增供应商</Button>
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
                <span class="rounded-full bg-bg-hover px-2 py-0.5 text-[10px] uppercase tracking-[0.12em] text-text-dim">
                  {{ provider.models.length ? `${provider.models.length} 个模型` : '未接入模型' }}
                </span>
              </div>
              <p class="mt-1 text-[11px] uppercase tracking-[0.12em] text-text-dim">{{ providerTypeLabel(provider.providerType) }}</p>
              <p v-if="provider.baseUrl" class="mt-1 truncate font-mono text-[11px] text-text-dim">{{ provider.baseUrl }}</p>
              <p class="mt-2 text-[12px] text-text-muted">Key: {{ provider.apiKeyHint }}</p>
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <Button variant="ghost" size="sm" @click="emit('startEdit', provider)">编辑供应商</Button>
              <Button variant="ghost" size="sm" class="text-[color:var(--destructive-text)] hover:text-[color:var(--destructive-text)]" @click="emit('deleteProvider', provider.id)">
                删除
              </Button>
            </div>
          </div>
        </article>
      </div>

      <div v-else class="settings-empty">
        还没有配置任何供应商。点击右上角“新增供应商”后，右侧会展开创建表单。
      </div>
    </section>

    <section v-if="showEditor" class="settings-panel">
      <div class="settings-panel-header">
        <div>
          <h3 class="settings-section-title">{{ activeEditorId ? '编辑供应商' : '新增供应商' }}</h3>
          <p class="settings-section-copy">右侧只负责当前供应商的接入配置。这里确认的是类型和连通性；模型能力留给“模型”页。</p>
        </div>
        <Button variant="ghost" size="sm" type="button" @click="emit('closeEditor')">收起</Button>
      </div>

      <form class="space-y-4" @submit.prevent="emit('submitProvider')">
        <div class="dialog-field">
          <label class="dialog-label" for="provider-type">类型</label>
          <SettingsSelect v-model="providerType" :options="providerTypeOptions" placeholder="请选择供应商类型" />
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
          <p v-if="baseUrlPreview" class="mt-2 text-[12px] text-text-muted">
            预览：<span class="font-mono text-text">{{ baseUrlPreview }}</span>
          </p>
          <p class="dialog-hint">
            {{ baseUrlHint }}
          </p>
        </div>
        <div class="dialog-field">
          <label class="dialog-label" for="provider-api-key">API Key</label>
          <div class="relative">
            <Input
              id="provider-api-key"
              v-model="providerApiKey"
              :type="showApiKey ? 'text' : 'password'"
              :placeholder="apiKeyPlaceholder"
              spellcheck="false"
              autocomplete="off"
              class="pr-11"
            />
            <Button
              variant="ghost"
              size="icon"
              type="button"
              class="absolute right-1 top-1/2 h-8 w-8 -translate-y-1/2 rounded-lg text-text-dim hover:text-text"
              :title="showApiKey ? '隐藏 API Key' : '显示 API Key'"
              @click="showApiKey = !showApiKey"
            >
              <Icon :icon="showApiKey ? eyeOffIcon : eyeIcon" class="h-4 w-4" />
            </Button>
          </div>
        </div>
        <div class="dialog-field">
          <div class="flex items-center justify-between gap-3">
            <label class="dialog-label" for="provider-probe-model">测试模型</label>
            <Button
              variant="ghost"
              size="sm"
              type="button"
              :disabled="busy || probeModelsLoading"
              @click="emit('requestProbeModelsNow', true)"
            >
              {{ probeModelsLoading ? '读取中…' : '刷新列表' }}
            </Button>
          </div>
          <SettingsCombobox
            v-model="providerProbeModel"
            :options="probeModelOptions"
            :placeholder="probeModelPlaceholder"
            :disabled="busy"
            :empty-text="probeModelSelectPlaceholder"
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
            这里只用它验证当前通道是否可用，不会直接把模型加入模型列表。
          </p>
        </div>
        <div class="rounded-2xl border border-[color:var(--ma-line-soft)] bg-bg-hover/30 px-4 py-3">
          <p class="text-[12px] font-medium text-text">类型与连通性</p>
          <p class="mt-1 text-[11px] leading-5 text-text-dim">供应商页确认的是这条通道能否连通，以及它属于哪类供应商类型；单个模型支持哪些能力，由模型页继续确认。</p>
        </div>
        <div class="flex items-center justify-end gap-2">
          <Button
            variant="outline"
            type="button"
            :disabled="busy || providerTestLoading"
            @click="emit('testProvider')"
          >
            <span
              v-if="providerTestLoading"
              class="mr-2 inline-block h-3.5 w-3.5 animate-spin rounded-full border-2 border-current border-t-transparent align-[-0.125em]"
              aria-hidden="true"
            />
            {{ providerTestLoading ? '测试中…' : '测试连通性' }}
          </Button>
          <Button variant="ghost" type="button" @click="emit('resetForm')">清空</Button>
          <Button type="submit" :disabled="busy">{{ activeEditorId ? '保存供应商' : '创建供应商' }}</Button>
        </div>
        <p v-if="providerTestMessage" class="text-[12px]" :class="providerTestSuccess ? 'text-success' : 'text-error'">
          {{ providerTestMessage }}
        </p>
      </form>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue';
import { Icon } from '@iconify/vue';
import eyeIcon from '@iconify-icons/lucide/eye';
import eyeOffIcon from '@iconify-icons/lucide/eye-off';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import SettingsCombobox from '@/components/SettingsCombobox.vue';
import SettingsSelect from '@/components/SettingsSelect.vue';
import type { ProviderSettingsView } from '@/data/mock';

const props = defineProps<{
  settings: ProviderSettingsView | null;
  busy?: boolean;
  probeModels: string[];
  probeSuggestedModels: string[];
  probeModelsLoading?: boolean;
  providerTestLoading?: boolean;
  providerTestMessage?: string;
  providerTestSuccess?: boolean;
  showEditor?: boolean;
  activeEditorId: number | null;
  providerType: string;
  providerName: string;
  providerBaseUrl: string;
  providerApiKey: string;
  providerProbeModel: string;
  providerTypeOptions: Array<{ value: string; label: string }>;
  providerNamePlaceholder: string;
  baseUrlPlaceholder: string;
  baseUrlPreview: string;
  baseUrlHint: string;
  apiKeyPlaceholder: string;
  probeModelPlaceholder: string;
  probeModelSelectPlaceholder: string;
  providerTypeLabel: (providerTypeValue: string) => string;
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
  submitProvider: [];
  testProvider: [];
  resetForm: [];
  requestProbeModelsNow: [forceRefresh?: boolean];
  closeEditor: [];
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
const showApiKey = ref(false);

const probeModelOptions = computed(() =>
  props.probeModels.map((model) => ({
    value: model,
    label: model,
  })),
);
</script>
