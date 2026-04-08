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

        <MemorySettingsSection
          v-else-if="activeSection === 'memory'"
          :memories="memories"
          :loading="memoriesLoading"
          @create-memory="emit('createMemory')"
          @edit-memory="emit('editMemory', $event)"
          @delete-memory="emit('deleteMemory', $event)"
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
import { useSettingsAgentForm } from '@/composables/settings/useSettingsAgentForm';
import { useSettingsModelForm } from '@/composables/settings/useSettingsModelForm';
import { useSettingsProviderForm } from '@/composables/settings/useSettingsProviderForm';
import { providerTypeLabel } from '@/composables/settings/settingsShared';
import type { BackendMemoryDetailView, ProviderSettingsView } from '@/data/mock';
import AgentSettingsSection from '@/components/settings/AgentSettingsSection.vue';
import AppearanceSettingsSection from '@/components/settings/AppearanceSettingsSection.vue';
import MemorySettingsSection from '@/components/settings/MemorySettingsSection.vue';
import ModelSettingsSection from '@/components/settings/ModelSettingsSection.vue';
import ProviderChannelsSection from '@/components/settings/ProviderChannelsSection.vue';
import { Button } from '@/components/ui/button';

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
  memories: BackendMemoryDetailView[];
  memoriesLoading?: boolean;
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
  createMemory: [];
  editMemory: [memoryId: string];
  deleteMemory: [memoryId: string];
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

const activeSection = ref<'appearance' | 'models' | 'providers' | 'agents' | 'memory'>('appearance');

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
  {
    value: 'memory' as const,
    label: '记忆',
    description: '长期知识与偏好管理',
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

const settingsRef = computed(() => props.settings);
const probeModelsRef = computed(() => props.probeModels);
const probeModelsLoadingRef = computed(() => props.probeModelsLoading);

// 页面层只保留 section 切换和事件编排，三类编辑状态下沉到独立 composable。
const {
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
  startCreate: openCreateProviderEditor,
  startEdit: openEditProviderEditor,
  hydrateProviderContext,
  closeProviderEditor,
  resetForm,
  submitProvider,
  testProvider,
} = useSettingsProviderForm({
  settings: settingsRef,
  probeModels: probeModelsRef,
  probeModelsLoading: probeModelsLoadingRef,
  onSaveProvider: (input) => emit('saveProvider', input),
  onTestProvider: (input) => emit('testProvider', input),
  onRequestProbeModels: (input) => emit('requestProbeModels', input),
});

const {
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
} = useSettingsModelForm({
  settings: settingsRef,
  probeModels: probeModelsRef,
  activeEditorId,
  providerType,
  applyProviderEditorState,
  requestProbeModelsIfNeeded,
  onSaveProviderModel: (input) => emit('saveProviderModel', input),
});

const {
  activeAgentName,
  agentName,
  agentDisplayName,
  agentDescription,
  agentAvatarColor,
  agentProviderIdString,
  agentModelId,
  agentSystemPrompt,
  editingBuiltInMarch,
  resolvedAgentName,
  agentProviderOptions,
  agentModelOptions,
  startCreateAgent: resetAgentDraft,
  startEditAgent: openAgentEditor,
  resetAgentForm,
  submitAgent,
  formatAgentBinding,
  formatAgentSource,
} = useSettingsAgentForm({
  settings: settingsRef,
  onSaveAgent: (input) => emit('saveAgent', input),
});

function startCreate() {
  activeSection.value = 'providers';
  openCreateProviderEditor();
  closeModelEditor();
  clearProviderModelDraft();
}

function startEdit(provider: ProviderSettingsView['providers'][number]) {
  activeSection.value = 'providers';
  openEditProviderEditor(provider);
  closeModelEditor();
  clearProviderModelDraft();
}

function startCreateAgent() {
  activeSection.value = 'agents';
  resetAgentDraft();
}

function startEditAgent(agent: ProviderSettingsView['agents'][number]) {
  activeSection.value = 'agents';
  openAgentEditor(agent);
}

function submitDefaultModel(modelConfigId: number) {
  if (!Number.isFinite(modelConfigId) || modelConfigId <= 0) {
    return;
  }
  emit('saveDefaultModel', {
    modelConfigId,
  });
}
</script>
