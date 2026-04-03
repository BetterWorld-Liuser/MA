<template>
  <section class="settings-shell">
    <header class="settings-header">
      <div>
        <p class="text-[11px] uppercase tracking-[0.18em] text-text-dim">Settings</p>
        <h2 class="mt-1 text-[22px] font-semibold tracking-[-0.02em] text-text">Provider 配置</h2>
        <p class="mt-2 max-w-[720px] text-[13px] leading-6 text-text-muted">
          这里管理 March 的全局 provider。当前版本会把默认 provider 用作运行时入口，任务级模型选择会覆盖默认模型。
        </p>
      </div>
      <Button variant="ghost" size="icon" class="rounded-xl border border-white/8" @click="$emit('close')">
        <Icon :icon="xIcon" class="h-4 w-4" />
      </Button>
    </header>

    <div class="settings-grid">
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
                <p class="mt-1 truncate font-mono text-[11px] text-text-dim">{{ provider.baseUrl }}</p>
                <p class="mt-2 text-[12px] text-text-muted">Key: {{ provider.apiKeyHint }}</p>
              </div>
              <div class="flex shrink-0 items-center gap-1">
                <Button variant="ghost" size="sm" @click="startEdit(provider)">编辑</Button>
                <Button variant="ghost" size="sm" class="text-[#ffb2b2] hover:text-[#ffb2b2]" @click="$emit('delete-provider', provider.id)">
                  删除
                </Button>
              </div>
            </div>
          </article>
        </div>

        <div v-else class="settings-empty">
          还没有配置 provider。先新增一个 base URL 和 API key，后面模型选择器就能接上它。
        </div>
      </section>

      <section class="settings-panel">
        <div class="settings-panel-header">
          <div>
            <h3 class="settings-section-title">{{ activeEditorId ? '编辑 Provider' : '新增 Provider' }}</h3>
            <p class="settings-section-copy">目前先支持 OpenAI-compatible 接口。</p>
          </div>
        </div>

        <form class="space-y-4" @submit.prevent="submitProvider">
          <div class="dialog-field">
            <label class="dialog-label" for="provider-name">名称</label>
            <Input id="provider-name" v-model="providerName" placeholder="OpenRouter / Local vLLM" />
          </div>
          <div class="dialog-field">
            <label class="dialog-label" for="provider-base-url">Base URL</label>
            <Input id="provider-base-url" v-model="providerBaseUrl" placeholder="https://api.openai.com/v1" />
          </div>
          <div class="dialog-field">
            <label class="dialog-label" for="provider-api-key">API Key</label>
            <Input
              id="provider-api-key"
              v-model="providerApiKey"
              type="password"
              :placeholder="activeEditorId ? '留空则保持当前 API key' : 'sk-...'"
            />
          </div>
          <div class="flex items-center justify-end gap-2">
            <Button variant="ghost" type="button" @click="resetForm">清空</Button>
            <Button type="submit" :disabled="busy">{{ activeEditorId ? '保存修改' : '创建 Provider' }}</Button>
          </div>
        </form>

        <div class="settings-divider"></div>

        <div class="settings-panel-header">
          <div>
            <h3 class="settings-section-title">默认模型</h3>
            <p class="settings-section-copy">默认 provider 会驱动聊天运行时和模型列表读取。</p>
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
          </div>
          <div class="flex items-center justify-end">
            <Button :disabled="busy || !defaultProviderIdLocal || !defaultModelLocal.trim()" @click="submitDefaultProvider">
              保存默认模型
            </Button>
          </div>
        </div>
      </section>
    </div>
  </section>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import xIcon from '@iconify-icons/lucide/x';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import type { ProviderSettingsView } from '@/data/mock';
import SettingsSelect from './SettingsSelect.vue';

const props = defineProps<{
  settings: ProviderSettingsView | null;
  busy?: boolean;
  modelsLoading?: boolean;
  availableModels: string[];
}>();

const emit = defineEmits<{
  close: [];
  saveProvider: [input: { id?: number; name: string; baseUrl: string; apiKey: string }];
  deleteProvider: [providerId: number];
  saveDefaultProvider: [input: { providerId: number; model: string }];
  requestModels: [providerId: number];
}>();

const activeEditorId = ref<number | null>(null);
const providerName = ref('');
const providerBaseUrl = ref('');
const providerApiKey = ref('');
const defaultProviderIdString = ref('');
const defaultModelLocal = ref('');

const defaultProviderIdLocal = computed(() => {
  const parsed = Number(defaultProviderIdString.value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
});

const providerOptions = computed(() =>
  (props.settings?.providers ?? []).map((provider) => ({
    value: String(provider.id),
    label: provider.name,
  })),
);

const modelOptions = computed(() =>
  props.availableModels.map((model) => ({
    value: model,
    label: model,
  })),
);

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

function startCreate() {
  activeEditorId.value = null;
  providerName.value = '';
  providerBaseUrl.value = '';
  providerApiKey.value = '';
}

function startEdit(provider: ProviderSettingsView['providers'][number]) {
  activeEditorId.value = provider.id;
  providerName.value = provider.name;
  providerBaseUrl.value = provider.baseUrl;
  providerApiKey.value = '';
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
  emit('save-provider', {
    id: activeEditorId.value ?? undefined,
    name: providerName.value,
    baseUrl: providerBaseUrl.value,
    apiKey: providerApiKey.value,
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
  emit('save-default-provider', {
    providerId: defaultProviderIdLocal.value,
    model: defaultModelLocal.value,
  });
}
</script>
