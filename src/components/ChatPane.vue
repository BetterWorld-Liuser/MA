<template>
  <section class="panel flex min-h-0 overflow-visible flex-col">
    <div class="chat-pane-header">
      <div class="chat-pane-meta">
        {{ chat.length ? `${chat.length} messages` : 'No messages yet' }}
      </div>
    </div>

    <ChatMessageList :chat="chat" :live-turn="liveTurn" :task-id="taskId" />

    <div class="shrink-0 p-2" style="border-top: 1px solid rgba(255, 255, 255, 0.08)">
        <div ref="composerRootRef" class="chat-composer-shell">
          <label class="sr-only" for="message-input">Reply</label>
          <input
            ref="imageInputRef"
            class="sr-only"
            type="file"
            accept="image/*"
            multiple
            @change="handleImageFileSelection"
          />

        <div
          class="chat-composer"
          :class="dragActive ? 'chat-composer-dragging' : ''"
          @paste="handlePaste"
          @dragover="handleDragOver"
          @dragleave="handleDragLeave"
          @drop="handleDrop"
        >
          <div v-if="mentions.length" class="chat-composer-chips" aria-label="Referenced context">
            <button
              v-for="chip in mentions"
              :key="`${chip.kind}:${chip.path}`"
              class="mention-chip"
              :class="chip.kind === 'directory' ? 'mention-chip-directory' : ''"
              type="button"
              :disabled="disabled"
              @click="removeMention(chip.path, chip.kind)"
            >
              <span class="mention-chip-kind">
                {{
                  chip.kind === 'directory'
                    ? 'DIR'
                    : chip.kind === 'skill'
                      ? 'SKILL'
                      : 'FILE'
                }}
              </span>
              <span class="mention-chip-label">{{ chip.kind === 'skill' ? chip.label : chip.path }}</span>
              <span class="mention-chip-remove" aria-hidden="true">×</span>
            </button>
          </div>

          <textarea
            id="message-input"
            ref="composerRef"
            v-model="draft"
            class="chat-composer-input"
            placeholder="帮我重构认证逻辑，必要时 @ 角色/文件/目录，或 / 选择技能。"
            :disabled="disabled"
            rows="1"
            @input="handleDraftInput"
            @click="updateMentionQueryFromCursor"
            @keyup="handleComposerKeyup"
            @keydown="onComposerKeydown"
          ></textarea>

          <div v-if="imageAttachments.length" class="composer-image-strip" aria-label="图片附件">
            <button
              v-for="image in imageAttachments"
              :key="image.id"
              class="composer-image-card"
              type="button"
              :disabled="disabled"
              @click="openImagePreview(image)"
            >
              <img class="composer-image-thumb" :src="image.previewUrl" :alt="image.name" />
              <span class="composer-image-name">{{ image.name }}</span>
              <span
                class="composer-image-remove"
                role="button"
                tabindex="-1"
                aria-label="移除图片"
                @click.stop="removeImageAttachment(image.id)"
              >
                ×
              </span>
            </button>
          </div>

          <div v-if="composerNotice" class="composer-inline-notice">
            {{ composerNotice }}
          </div>

          <div class="chat-composer-toolbar">
            <div class="chat-composer-toolbar-group">
              <button class="composer-action" type="button" :disabled="disabled" title="选择文件或目录" @click="togglePlusMenu">
                <span class="composer-action-icon">+</span>
              </button>
              <button
                class="composer-directory-button"
                :class="isCustomWorkingDirectory ? 'composer-directory-button-active' : ''"
                type="button"
                :disabled="disabled"
                :title="workingDirectoryTooltip"
                @click="pickWorkingDirectory"
              >
                <Icon :icon="folderOpenIcon" class="h-3.5 w-3.5 shrink-0" />
                <span class="truncate">{{ workingDirectoryLabel }}</span>
              </button>
              <button
                v-if="isCustomWorkingDirectory"
                class="composer-action"
                type="button"
                :disabled="disabled"
                title="恢复默认工作目录"
                @click="resetWorkingDirectory"
              >
                <Icon :icon="rotateCcwIcon" class="h-3.5 w-3.5" />
              </button>
              <div ref="modelMenuAnchorRef" class="composer-model-anchor">
                <button class="composer-model-button" type="button" :disabled="disabled" title="模型选择器" @click="toggleModelMenu">
                  <span class="truncate">{{ modelButtonLabel }}</span>
                  <span aria-hidden="true">∨</span>
                </button>
              </div>
              <div ref="modelSettingsAnchorRef" class="composer-model-anchor">
                <button
                  class="composer-action"
                  type="button"
                  :disabled="disabled"
                  title="模型参数"
                  @click="toggleModelSettingsMenu"
                >
                  <Icon :icon="slidersHorizontalIcon" class="h-3.5 w-3.5" />
                </button>
              </div>
            </div>

            <div class="chat-composer-toolbar-group">
              <p class="composer-shortcut-hint">
                <span class="font-medium">Enter</span> 发送
                <span class="mx-1 text-text-dim/70">·</span>
                <span class="font-medium">Shift+Enter</span> 换行
              </p>
              <button
                v-if="sending"
                class="composer-stop-button"
                type="button"
                :disabled="cancelling"
                :aria-label="cancelling ? '正在中断生成' : '中断生成'"
                :title="cancelling ? '正在中断生成' : '中断生成'"
                @click="emit('cancelTurn')"
              >
                <Icon :icon="pauseIcon" class="h-3.5 w-3.5" />
                <span>{{ cancelling ? '中断中' : '中断' }}</span>
              </button>
              <button
                v-else
                class="composer-send-button"
                type="button"
                :disabled="disabled || interactionLocked || composerIsEmpty"
                @click="submit"
              >
                ↵发送
              </button>
            </div>
          </div>
        </div>

        <div v-if="searchPanelOpen" class="composer-popover">
          <div class="composer-popover-header">
            <span>{{ searchPanelLabel }}</span>
            <span class="composer-popover-query">{{ activeSearchQuery || '全部' }}</span>
          </div>
          <div v-if="searchLoading" class="composer-popover-empty">正在搜索…</div>
          <div v-else-if="searchResults.length" class="composer-popover-list">
            <button
              v-for="(entry, index) in searchResults"
              :key="entry.kind === 'agent' ? `agent:${entry.name}` : entry.kind === 'skill' ? `skill:${entry.path}` : `${entry.kind}:${entry.path}`"
              class="composer-popover-item"
              :class="index === highlightedResultIndex ? 'composer-popover-item-active' : ''"
              type="button"
              @mousedown.prevent="selectWorkspaceEntry(entry)"
              @mouseenter="highlightedResultIndex = index"
            >
              <span class="composer-popover-item-kind">
                {{
                  entry.kind === 'agent'
                    ? '角色'
                    : entry.kind === 'skill'
                      ? '技能'
                    : entry.kind === 'directory'
                      ? '目录'
                      : '文件'
                }}
              </span>
              <template v-if="entry.kind === 'agent'">
                <div class="composer-popover-item-main">
                  <div class="composer-popover-item-line">
                    <span class="composer-popover-item-path composer-popover-item-path-strong">@{{ entry.name }}</span>
                    <span class="composer-popover-item-meta">{{ entry.displayName }}</span>
                  </div>
                  <span class="composer-popover-item-meta">{{ entry.description }}</span>
                </div>
              </template>
              <template v-else-if="entry.kind === 'skill'">
                <div class="composer-popover-item-main">
                  <div class="composer-popover-item-line">
                    <span class="composer-popover-item-path composer-popover-item-path-strong">{{ entry.name }}</span>
                    <span v-if="entry.description" class="composer-popover-item-meta">{{ entry.description }}</span>
                    <span v-else class="composer-popover-item-meta">可直接加入 Open Files 的技能说明文件</span>
                  </div>
                </div>
              </template>
              <template v-else>
                <span class="composer-popover-item-path">{{ entry.path }}</span>
              </template>
            </button>
          </div>
          <div v-else class="composer-popover-empty">没有匹配结果</div>
        </div>

        <div v-if="plusMenuOpen" class="composer-menu">
          <button class="composer-menu-item" type="button" @mousedown.prevent="openSearchFromMenu('file')">选择文件…</button>
          <button class="composer-menu-item" type="button" @mousedown.prevent="openSearchFromMenu('directory')">选择目录…</button>
          <button class="composer-menu-item" type="button" @mousedown.prevent="openSearchFromMenu('skill')">选择技能…</button>
          <button
            v-if="supportsVision"
            class="composer-menu-item"
            type="button"
            @mousedown.prevent="triggerImagePicker"
          >
            选择图片…
          </button>
        </div>

      </div>
    </div>
    <Teleport to="body">
      <div
        v-if="modelMenuOpen"
        ref="modelMenuPanelRef"
        class="composer-menu-portal composer-menu-model"
        :style="modelMenuStyle"
      >
        <div class="composer-menu-header">
          <span>可用模型</span>
          <span v-if="modelsRefreshing" class="composer-menu-status">刷新中…</span>
        </div>
        <div class="composer-menu-search-shell">
          <input
            ref="modelSearchRef"
            v-model="modelSearchQuery"
            class="app-input composer-menu-search-input"
            type="text"
            placeholder="搜索模型…"
          />
        </div>
        <div class="composer-menu-list">
          <div v-if="modelsLoading" class="composer-menu-empty">正在读取…</div>
          <template v-else-if="filteredProviderGroups.length">
            <div
              v-for="group in filteredProviderGroups"
              :key="group.providerCacheKey"
              class="border-b border-border/60 last:border-b-0"
            >
              <div class="composer-menu-header">
                <span>{{ group.providerName }}</span>
                <span class="composer-menu-status">{{ providerTypeLabel(group.providerType) }}</span>
              </div>
              <button
                v-for="model in group.filteredModels"
                :key="`${group.providerCacheKey}:${model}`"
                class="composer-menu-item composer-menu-item-model"
                :class="isModelActive(group.providerId, model) ? 'composer-menu-item-active' : ''"
                type="button"
                @mousedown.prevent="selectModel(group.providerId, model)"
              >
                <span>{{ model }}</span>
                <span v-if="isModelActive(group.providerId, model)">✓</span>
              </button>
            </div>
          </template>
          <div v-else-if="!providerGroups.length && !modelsLoading" class="composer-menu-empty">当前没有可读模型列表</div>
          <div v-else-if="!modelsLoading" class="composer-menu-empty">没有匹配的模型</div>
        </div>
      </div>
    </Teleport>

    <Teleport to="body">
      <div
        v-if="modelSettingsOpen"
        ref="modelSettingsPanelRef"
        class="composer-menu-portal composer-menu-model"
        :style="modelSettingsStyle"
      >
        <div class="composer-menu-header">
          <span>模型参数</span>
          <span class="composer-menu-status">{{ effectiveSelectedModel || '当前任务' }}</span>
        </div>
        <div class="space-y-3 px-3 py-2">
          <div class="dialog-field">
            <label class="dialog-label" for="composer-temperature">Temperature</label>
            <input
              id="composer-temperature"
              v-model="temperatureDraft"
              class="app-input"
              type="number"
              min="0"
              max="2"
              step="0.1"
              placeholder="留空则使用默认值 0.2"
            />
            <p class="dialog-hint">控制输出发散度。`0` 更稳定，`2` 更开放。</p>
          </div>
          <div class="dialog-field">
            <label class="dialog-label" for="composer-max-output">Max Output Tokens</label>
            <input
              id="composer-max-output"
              v-model="maxOutputTokensDraft"
              class="app-input"
              type="number"
              min="1"
              step="1"
              :placeholder="maxOutputTokensPlaceholder"
            />
            <p class="dialog-hint">留空时跟随模型默认上限与 provider 默认行为。</p>
          </div>
          <div class="space-y-3">
            <div class="dialog-field">
              <label class="dialog-label" for="composer-top-p">Top P</label>
              <input
                id="composer-top-p"
                v-model="topPDraft"
                class="app-input"
                type="number"
                min="0"
                max="1"
                step="0.05"
                placeholder="留空则使用 provider 默认值"
              />
              <p class="dialog-hint">核采样范围。</p>
            </div>
            <div class="dialog-field">
              <label class="dialog-label" for="composer-presence-penalty">Presence Penalty</label>
              <input
                id="composer-presence-penalty"
                v-model="presencePenaltyDraft"
                class="app-input"
                type="number"
                min="-2"
                max="2"
                step="0.1"
                placeholder="默认 0"
              />
              <p class="dialog-hint">提高新话题倾向。</p>
            </div>
            <div class="dialog-field">
              <label class="dialog-label" for="composer-frequency-penalty">Frequency Penalty</label>
              <input
                id="composer-frequency-penalty"
                v-model="frequencyPenaltyDraft"
                class="app-input"
                type="number"
                min="-2"
                max="2"
                step="0.1"
                placeholder="默认 0"
              />
              <p class="dialog-hint">降低重复倾向。</p>
            </div>
          </div>
          <p v-if="modelSettingsError" class="text-[11px] text-error">{{ modelSettingsError }}</p>
        </div>
        <div class="composer-model-settings-footer">
          <div class="composer-model-settings-actions">
            <button class="app-button app-button-secondary composer-model-settings-button" type="button" @click="resetModelSettingsDraft">
              重置
            </button>
            <button class="app-button app-button-primary composer-model-settings-button" type="button" @click="applyModelSettings">
              应用
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <Teleport to="body">
      <div v-if="previewImage" class="composer-image-preview-backdrop" @click="closeImagePreview">
        <div class="composer-image-preview-panel" @click.stop>
          <button class="composer-image-preview-close" type="button" @click="closeImagePreview">关闭</button>
          <img class="composer-image-preview-image" :src="previewImage.previewUrl" :alt="previewImage.name" />
          <p class="composer-image-preview-name">{{ previewImage.name }}</p>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, toRef, watch } from 'vue';
import { Icon } from '@iconify/vue';
import { invoke } from '@tauri-apps/api/core';
import { open as openPathDialog } from '@tauri-apps/plugin-dialog';
import pauseIcon from '@iconify-icons/lucide/pause';
import folderOpenIcon from '@iconify-icons/lucide/folder-open';
import rotateCcwIcon from '@iconify-icons/lucide/rotate-ccw';
import slidersHorizontalIcon from '@iconify-icons/lucide/sliders-horizontal';
import ChatMessageList from '@/components/ChatMessageList.vue';
import { useChatComposer } from '@/composables/useChatComposer';
import type { ComposerImageAttachment } from '@/composables/useChatComposer';
import type { TaskModelSelectorView } from '../data/mock';

type CachedProviderGroup = {
  providerId?: number | null;
  providerName: string;
  providerType: string;
  providerCacheKey: string;
  availableModels: string[];
};

type CachedTaskModelSelector = {
  currentProviderId?: number | null;
  currentModel: string;
  currentTemperature?: number | null;
  currentTopP?: number | null;
  currentPresencePenalty?: number | null;
  currentFrequencyPenalty?: number | null;
  currentMaxOutputTokens?: number | null;
  currentModelDefaultMaxOutputTokens?: number | null;
  providers: CachedProviderGroup[];
};

// 模型列表读取仍然可能依赖 provider 网络请求。
// 这里保留一个前端进程内缓存，让菜单可以先秒开最近一次成功结果，再异步刷新。
const taskModelSelectorCache = new Map<number, CachedTaskModelSelector>();

const props = defineProps<{
  chat: import('../data/mock').ChatMessage[];
  liveTurn?: import('../data/mock').LiveTurn;
  disabled?: boolean;
  sending?: boolean;
  interactionLocked?: boolean;
  cancelling?: boolean;
  taskId?: number | null;
  selectedModel?: string;
  selectedTemperature?: number;
  selectedTopP?: number;
  selectedPresencePenalty?: number;
  selectedFrequencyPenalty?: number;
  selectedMaxOutputTokens?: number;
  workingDirectory?: string;
  workspacePath?: string;
  settingsOpen?: boolean;
}>();

const emit = defineEmits<{
  send: [payload: { content: string; directories: string[]; files: string[]; skills: string[]; images: ComposerImageAttachment[] }];
  setModel: [selection: { providerId?: number | null; model: string }];
  setModelSettings: [settings: {
    temperature?: number | null;
    topP?: number | null;
    presencePenalty?: number | null;
    frequencyPenalty?: number | null;
    maxOutputTokens?: number | null;
  }];
  setWorkingDirectory: [path?: string | null];
  cancelTurn: [];
}>();

const disabledRef = computed(() => !!props.disabled);
const interactionLockedRef = computed(() => !!props.interactionLocked);
const taskIdRef = toRef(props, 'taskId');
const resolvedModelSupportsVision = ref(false);
const supportsVision = computed(() => resolvedModelSupportsVision.value);

const {
  draft,
  mentions,
  imageAttachments,
  composerRef,
  composerRootRef,
  imageInputRef,
  activeSearchQuery,
  searchResults,
  searchLoading,
  highlightedResultIndex,
  searchPanelOpen,
  plusMenuOpen,
  composerIsEmpty,
  searchPanelLabel,
  composerNotice,
  dragActive,
  handleDraftInput,
  handleComposerKeyup,
  handleComposerKeydown,
  updateMentionQueryFromCursor,
  openSearchFromMenu,
  selectWorkspaceEntry,
  removeMention,
  removeImageAttachment,
  togglePlusMenu,
  triggerImagePicker,
  handleImageFileSelection,
  handlePaste,
  handleDrop,
  handleDragOver,
  handleDragLeave,
  handleDocumentPointerDown,
  syncComposerHeight,
  focusComposer,
  resetComposer,
} = useChatComposer({
  disabled: disabledRef,
  sending: interactionLockedRef,
  taskId: taskIdRef,
  supportsVision,
});

const modelMenuAnchorRef = ref<HTMLElement | null>(null);
const modelMenuPanelRef = ref<HTMLElement | null>(null);
const modelSearchRef = ref<HTMLInputElement | null>(null);
const modelMenuOpen = ref(false);
const modelSettingsAnchorRef = ref<HTMLElement | null>(null);
const modelSettingsPanelRef = ref<HTMLElement | null>(null);
const modelSettingsOpen = ref(false);
const providerGroups = ref<CachedProviderGroup[]>([]);
const modelSearchQuery = ref('');
const modelsLoading = ref(false);
const modelsRefreshing = ref(false);
const resolvedCurrentProviderId = ref<number | null>(null);
const resolvedCurrentModel = ref('');
const resolvedCurrentTemperature = ref<number | null>(null);
const resolvedCurrentTopP = ref<number | null>(null);
const resolvedCurrentPresencePenalty = ref<number | null>(null);
const resolvedCurrentFrequencyPenalty = ref<number | null>(null);
const resolvedCurrentMaxOutputTokens = ref<number | null>(null);
const resolvedModelDefaultMaxOutputTokens = ref<number | null>(null);
const modelMenuStyle = ref<Record<string, string>>({});
const modelSettingsStyle = ref<Record<string, string>>({});
const temperatureDraft = ref('');
const topPDraft = ref('');
const presencePenaltyDraft = ref('');
const frequencyPenaltyDraft = ref('');
const maxOutputTokensDraft = ref('');
const modelSettingsError = ref('');
const previewImage = ref<ComposerImageAttachment | null>(null);
let activeModelRequestId = 0;

const effectiveSelectedModel = computed(() => props.selectedModel?.trim() || resolvedCurrentModel.value.trim());
const modelButtonLabel = computed(() => effectiveSelectedModel.value || '选择模型');
const maxOutputTokensPlaceholder = computed(() =>
  resolvedModelDefaultMaxOutputTokens.value
    ? `留空则使用默认值 ${resolvedModelDefaultMaxOutputTokens.value}`
    : '留空则使用模型默认值',
);
const normalizedWorkspacePath = computed(() => normalizePath(props.workspacePath));
const normalizedWorkingDirectory = computed(() => normalizePath(props.workingDirectory));
const isCustomWorkingDirectory = computed(
  () =>
    !!normalizedWorkingDirectory.value
    && !!normalizedWorkspacePath.value
    && normalizedWorkingDirectory.value !== normalizedWorkspacePath.value,
);
const workingDirectoryLabel = computed(() => normalizedWorkingDirectory.value || '工作目录');
const workingDirectoryTooltip = computed(() =>
  normalizedWorkingDirectory.value
    ? `AI 工作目录：${normalizedWorkingDirectory.value}`
    : '设置 AI 工作目录',
);
const filteredProviderGroups = computed(() => {
  const query = modelSearchQuery.value.trim().toLowerCase();
  return providerGroups.value
    .map((group) => ({
      ...group,
      filteredModels: !query
        ? group.availableModels
        : group.availableModels.filter((model) => model.toLowerCase().includes(query)),
    }))
    .filter((group) => group.filteredModels.length > 0);
});

watch(
  draft,
  async () => {
    await nextTick();
    syncComposerHeight();
  },
  { flush: 'post' },
);

watch(
  () => props.taskId,
  (taskId) => {
    resetComposer();
    closeModelMenu();
    restoreModelStateFromCache(taskId);
    seedModelListFromCurrentSelection();
    resetModelSettingsDraft();
    void refreshModels();
  },
);

watch(
  () => props.selectedModel,
  (model) => {
    resolvedCurrentModel.value = model?.trim() ?? '';
    seedModelListFromCurrentSelection();
  },
  { immediate: true },
);

watch(
  () => props.selectedTemperature,
  (temperature) => {
    resolvedCurrentTemperature.value = temperature ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  },
  { immediate: true },
);

watch(
  () => props.selectedTopP,
  (value) => {
    resolvedCurrentTopP.value = value ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  },
  { immediate: true },
);

watch(
  () => props.selectedPresencePenalty,
  (value) => {
    resolvedCurrentPresencePenalty.value = value ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  },
  { immediate: true },
);

watch(
  () => props.selectedFrequencyPenalty,
  (value) => {
    resolvedCurrentFrequencyPenalty.value = value ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  },
  { immediate: true },
);

watch(
  () => props.selectedMaxOutputTokens,
  (value) => {
    resolvedCurrentMaxOutputTokens.value = value ?? null;
    if (!modelSettingsOpen.value) {
      resetModelSettingsDraft();
    }
  },
  { immediate: true },
);

watch(
  () => props.settingsOpen,
  (open) => {
    if (open) {
      closeModelMenu();
      closeModelSettingsMenu();
    }
  },
);

watch([modelMenuOpen, filteredProviderGroups, modelSearchQuery], async ([open]) => {
  if (!open) {
    return;
  }
  await nextTick();
  syncModelMenuPosition();
});

watch(modelSettingsOpen, async (open) => {
  if (!open) {
    return;
  }
  await nextTick();
  syncModelSettingsMenuPosition();
});

onMounted(() => {
  document.addEventListener('mousedown', handleDocumentPointerDown);
  document.addEventListener('mousedown', handleModelMenuPointerDown);
  window.addEventListener('resize', syncFloatingMenus);
  window.addEventListener('scroll', syncFloatingMenus, true);
  restoreModelStateFromCache(props.taskId);
  seedModelListFromCurrentSelection();
  resetModelSettingsDraft();
  void refreshModels();
});

onUnmounted(() => {
  document.removeEventListener('mousedown', handleDocumentPointerDown);
  document.removeEventListener('mousedown', handleModelMenuPointerDown);
  window.removeEventListener('resize', syncFloatingMenus);
  window.removeEventListener('scroll', syncFloatingMenus, true);
});

function onComposerKeydown(event: KeyboardEvent) {
  handleComposerKeydown(event, submit);
}

async function toggleModelMenu() {
  if (!modelMenuOpen.value) {
    primeModelMenu();
    plusMenuOpen.value = false;
    modelMenuOpen.value = true;
    modelSearchQuery.value = '';
    await nextTick();
    syncModelMenuPosition();
    modelSearchRef.value?.focus();
    return;
  }
  closeModelMenu();
}

function primeModelMenu() {
  restoreModelStateFromCache(props.taskId);
  seedModelListFromCurrentSelection();
  void refreshModels();
}

function restoreModelStateFromCache(taskId?: number | null) {
  if (!taskId) {
    resolvedCurrentProviderId.value = null;
    resolvedCurrentModel.value = '';
    resolvedModelSupportsVision.value = false;
    providerGroups.value = [];
    resolvedCurrentTopP.value = null;
    resolvedCurrentPresencePenalty.value = null;
    resolvedCurrentFrequencyPenalty.value = null;
    return;
  }

  const cached = taskModelSelectorCache.get(taskId);
  if (!cached) {
    return;
  }

  resolvedCurrentProviderId.value = cached.currentProviderId ?? null;
  resolvedCurrentModel.value = cached.currentModel;
  providerGroups.value = cached.providers.map((group) => ({
    ...group,
    availableModels: [...group.availableModels],
  }));
  resolvedCurrentTemperature.value = cached.currentTemperature ?? null;
  resolvedCurrentTopP.value = cached.currentTopP ?? null;
  resolvedCurrentPresencePenalty.value = cached.currentPresencePenalty ?? null;
  resolvedCurrentFrequencyPenalty.value = cached.currentFrequencyPenalty ?? null;
  resolvedCurrentMaxOutputTokens.value = cached.currentMaxOutputTokens ?? null;
  resolvedModelDefaultMaxOutputTokens.value = cached.currentModelDefaultMaxOutputTokens ?? null;
}

function seedModelListFromCurrentSelection() {
  const selected = props.selectedModel?.trim();
  if (!selected) {
    return;
  }

  resolvedCurrentModel.value = selected;
  if (resolvedCurrentProviderId.value !== null) {
    const activeGroup = providerGroups.value.find((group) => group.providerId === resolvedCurrentProviderId.value);
    if (activeGroup && !activeGroup.availableModels.includes(selected)) {
      activeGroup.availableModels = [selected, ...activeGroup.availableModels];
    }
    return;
  }

  if (providerGroups.value.length === 1 && !providerGroups.value[0].availableModels.includes(selected)) {
    providerGroups.value[0].availableModels = [selected, ...providerGroups.value[0].availableModels];
  }
}

async function refreshModels() {
  if (!props.taskId) {
    return;
  }

  const requestId = ++activeModelRequestId;
  const hasWarmData = providerGroups.value.some((group) => group.availableModels.length > 0);
  modelsLoading.value = !hasWarmData;
  modelsRefreshing.value = hasWarmData;
  try {
    const response = await invoke<TaskModelSelectorView>('list_provider_models', {
      taskId: props.taskId,
    });
    if (requestId !== activeModelRequestId) {
      return;
    }
    applyProviderModels(response, props.taskId);
  } finally {
    if (requestId === activeModelRequestId) {
      modelsLoading.value = false;
      modelsRefreshing.value = false;
    }
  }
}

function applyProviderModels(response: TaskModelSelectorView, taskId: number) {
  const normalizedProviders = response.providers.map((group) => ({
    providerId: group.providerId ?? null,
    providerName: group.providerName,
    providerType: group.providerType,
    providerCacheKey: group.providerCacheKey,
    availableModels: Array.from(
      new Set(
        [
          ...(response.currentProviderId === group.providerId ? [response.currentModel] : []),
          ...group.availableModels,
        ]
          .map((model) => model.trim())
          .filter(Boolean),
      ),
    ),
  }));

  const cacheEntry: CachedTaskModelSelector = {
    currentProviderId: response.currentProviderId ?? null,
    currentModel: response.currentModel,
    currentTemperature: response.currentTemperature ?? null,
    currentTopP: response.currentTopP ?? null,
    currentPresencePenalty: response.currentPresencePenalty ?? null,
    currentFrequencyPenalty: response.currentFrequencyPenalty ?? null,
    currentMaxOutputTokens: response.currentMaxOutputTokens ?? null,
    currentModelDefaultMaxOutputTokens: response.currentModelCapabilities.maxOutputTokens ?? null,
    providers: normalizedProviders,
  };

  taskModelSelectorCache.set(taskId, cacheEntry);
  resolvedCurrentProviderId.value = cacheEntry.currentProviderId ?? null;
  resolvedCurrentModel.value = cacheEntry.currentModel;
  resolvedCurrentTemperature.value = cacheEntry.currentTemperature ?? null;
  resolvedCurrentTopP.value = cacheEntry.currentTopP ?? null;
  resolvedCurrentPresencePenalty.value = cacheEntry.currentPresencePenalty ?? null;
  resolvedCurrentFrequencyPenalty.value = cacheEntry.currentFrequencyPenalty ?? null;
  resolvedCurrentMaxOutputTokens.value = cacheEntry.currentMaxOutputTokens ?? null;
  resolvedModelDefaultMaxOutputTokens.value = cacheEntry.currentModelDefaultMaxOutputTokens ?? null;
  resolvedModelSupportsVision.value = response.currentModelCapabilities.supportsVision;
  providerGroups.value = cacheEntry.providers.map((group) => ({
    ...group,
    availableModels: [...group.availableModels],
  }));
  resetModelSettingsDraft();
}

function selectModel(providerId: number | null | undefined, model: string) {
  resolvedCurrentProviderId.value = providerId ?? null;
  resolvedCurrentModel.value = model;
  const activeGroup = providerGroups.value.find((group) => group.providerId === (providerId ?? null));
  if (activeGroup && !activeGroup.availableModels.includes(model)) {
    activeGroup.availableModels = [model, ...activeGroup.availableModels];
  }
  emit('setModel', { providerId, model });
  closeModelMenu();
}

function isModelActive(providerId: number | null | undefined, model: string) {
  return (providerId ?? null) === resolvedCurrentProviderId.value && model === effectiveSelectedModel.value;
}

function closeModelMenu() {
  modelSearchQuery.value = '';
  modelMenuOpen.value = false;
}

async function toggleModelSettingsMenu() {
  if (!modelSettingsOpen.value) {
    primeModelMenu();
    closeModelMenu();
    resetModelSettingsDraft();
    modelSettingsOpen.value = true;
    await nextTick();
    syncModelSettingsMenuPosition();
    return;
  }
  closeModelSettingsMenu();
}

function closeModelSettingsMenu() {
  modelSettingsError.value = '';
  modelSettingsOpen.value = false;
}

function resetModelSettingsDraft() {
  temperatureDraft.value = resolvedCurrentTemperature.value == null ? '' : String(resolvedCurrentTemperature.value);
  topPDraft.value = resolvedCurrentTopP.value == null ? '' : String(resolvedCurrentTopP.value);
  presencePenaltyDraft.value = resolvedCurrentPresencePenalty.value == null ? '' : String(resolvedCurrentPresencePenalty.value);
  frequencyPenaltyDraft.value = resolvedCurrentFrequencyPenalty.value == null ? '' : String(resolvedCurrentFrequencyPenalty.value);
  maxOutputTokensDraft.value = resolvedCurrentMaxOutputTokens.value == null ? '' : String(resolvedCurrentMaxOutputTokens.value);
  modelSettingsError.value = '';
}

function applyModelSettings() {
  const parsedTemperature = parseOptionalNumber(temperatureDraft.value);
  const parsedTopP = parseOptionalNumber(topPDraft.value);
  const parsedPresencePenalty = parseOptionalNumber(presencePenaltyDraft.value);
  const parsedFrequencyPenalty = parseOptionalNumber(frequencyPenaltyDraft.value);
  const parsedMaxOutputTokens = parseOptionalInteger(maxOutputTokensDraft.value);

  if (parsedTemperature !== null && (parsedTemperature < 0 || parsedTemperature > 2)) {
    modelSettingsError.value = 'Temperature 需要在 0 到 2 之间。';
    return;
  }

  if (parsedMaxOutputTokens !== null && parsedMaxOutputTokens < 1) {
    modelSettingsError.value = 'Max output tokens 需要大于 0。';
    return;
  }

  if (parsedTopP !== null && (parsedTopP < 0 || parsedTopP > 1)) {
    modelSettingsError.value = 'Top P 需要在 0 到 1 之间。';
    return;
  }

  if (parsedPresencePenalty !== null && (parsedPresencePenalty < -2 || parsedPresencePenalty > 2)) {
    modelSettingsError.value = 'Presence penalty 需要在 -2 到 2 之间。';
    return;
  }

  if (parsedFrequencyPenalty !== null && (parsedFrequencyPenalty < -2 || parsedFrequencyPenalty > 2)) {
    modelSettingsError.value = 'Frequency penalty 需要在 -2 到 2 之间。';
    return;
  }

  modelSettingsError.value = '';
  emit('setModelSettings', {
    temperature: parsedTemperature,
    topP: parsedTopP,
    presencePenalty: parsedPresencePenalty,
    frequencyPenalty: parsedFrequencyPenalty,
    maxOutputTokens: parsedMaxOutputTokens,
  });
  closeModelSettingsMenu();
}

async function pickWorkingDirectory() {
  const selected = await openPathDialog({
    directory: true,
    multiple: false,
    defaultPath: props.workingDirectory || props.workspacePath,
    title: '选择 AI 工作目录',
  });
  if (!selected || Array.isArray(selected)) {
    return;
  }
  emit('setWorkingDirectory', selected);
}

function resetWorkingDirectory() {
  emit('setWorkingDirectory', null);
}

function handleModelMenuPointerDown(event: MouseEvent) {
  if (!modelMenuOpen.value && !modelSettingsOpen.value) {
    return;
  }

  const target = event.target as Node | null;
  if (!target) {
    return;
  }

  const clickedAnchor = modelMenuAnchorRef.value?.contains(target);
  const clickedPanel = modelMenuPanelRef.value?.contains(target);
  const clickedSettingsAnchor = modelSettingsAnchorRef.value?.contains(target);
  const clickedSettingsPanel = modelSettingsPanelRef.value?.contains(target);
  if (!clickedAnchor && !clickedPanel) {
    closeModelMenu();
  }
  if (!clickedSettingsAnchor && !clickedSettingsPanel) {
    closeModelSettingsMenu();
  }
}

function providerTypeLabel(providerType: string) {
  const labels: Record<string, string> = {
    anthropic: 'Anthropic',
    openai: 'OpenAI',
    gemini: 'Gemini',
    openai_compat: 'OpenAI 兼容',
    ollama: 'Ollama',
    env: '环境',
  };
  return labels[providerType] ?? providerType;
}

function syncModelSettingsMenuPosition() {
  if (!modelSettingsOpen.value) {
    return;
  }

  const anchor = modelSettingsAnchorRef.value;
  if (!anchor) {
    return;
  }

  const rect = anchor.getBoundingClientRect();
  const menuWidth = Math.max(320, rect.width + 260);
  const viewportPadding = 12;
  const left = Math.min(
    Math.max(viewportPadding, rect.right - menuWidth),
    window.innerWidth - menuWidth - viewportPadding,
  );

  modelSettingsStyle.value = {
    position: 'fixed',
    left: `${left}px`,
    bottom: `${Math.max(viewportPadding, window.innerHeight - rect.top + 10)}px`,
    width: `${menuWidth}px`,
  };
}

function syncModelMenuPosition() {
  if (!modelMenuOpen.value) {
    return;
  }

  const anchor = modelMenuAnchorRef.value;
  if (!anchor) {
    return;
  }

  const rect = anchor.getBoundingClientRect();
  const menuWidth = Math.max(rect.width, 320);
  const viewportPadding = 12;
  const left = Math.min(
    Math.max(viewportPadding, rect.left),
    window.innerWidth - menuWidth - viewportPadding,
  );
  const maxHeight = Math.min(416, window.innerHeight - 144);

  modelMenuStyle.value = {
    position: 'fixed',
    left: `${left}px`,
    bottom: `${Math.max(viewportPadding, window.innerHeight - rect.top + 10)}px`,
    width: `${menuWidth}px`,
    maxHeight: `${maxHeight}px`,
  };
}

function syncFloatingMenus() {
  syncModelMenuPosition();
  syncModelSettingsMenuPosition();
}

function submit() {
  const content = draft.value.trim();
  if ((!content && mentions.value.length === 0 && imageAttachments.value.length === 0) || props.disabled || props.interactionLocked) {
    return;
  }

  const directories = mentions.value.filter((item) => item.kind === 'directory').map((item) => item.path);
  const files = mentions.value.filter((item) => item.kind === 'file').map((item) => item.path);
  const skills = mentions.value.filter((item) => item.kind === 'skill').map((item) => item.path);
  emit('send', {
    content,
    directories,
    files,
    skills,
    images: imageAttachments.value,
  });
  resetComposer();
  closeModelMenu();
  closeModelSettingsMenu();
}

function openImagePreview(image: ComposerImageAttachment) {
  previewImage.value = image;
}

function closeImagePreview() {
  previewImage.value = null;
}

defineExpose({
  focusComposer,
});

function normalizePath(path?: string) {
  if (!path) {
    return '';
  }

  const normalized = path.replaceAll('\\', '/');
  if (normalized.startsWith('//?/UNC/')) {
    return `//${normalized.slice('//?/UNC/'.length)}`;
  }
  if (normalized.startsWith('//?/')) {
    return normalized.slice('//?/'.length);
  }
  return normalized;
}

function parseOptionalNumber(value: string | number | null | undefined) {
  if (value == null) {
    return null;
  }
  const normalized = typeof value === 'string' ? value.trim() : String(value);
  if (!normalized) {
    return null;
  }
  const parsed = Number(normalized);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseOptionalInteger(value: string | number | null | undefined) {
  if (value == null) {
    return null;
  }
  const normalized = typeof value === 'string' ? value.trim() : String(value);
  if (!normalized) {
    return null;
  }
  const parsed = Number(normalized);
  return Number.isInteger(parsed) ? parsed : null;
}

</script>
