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
        <ChatPaneModelMenu
          :model-items="modelItems"
          :filtered-model-items="filteredModelItems"
          :model-search-query="modelSearchQuery"
          :models-loading="modelsLoading"
          :models-refreshing="modelsRefreshing"
          :provider-type-label="providerTypeLabel"
          :is-model-active="isModelActive"
          @update:model-search-query="modelSearchQuery = $event"
          @select-model="selectModel($event.modelConfigId, $event.model)"
        />
      </div>
    </Teleport>

    <Teleport to="body">
      <div
        v-if="modelSettingsOpen"
        ref="modelSettingsPanelRef"
        class="composer-menu-portal composer-menu-model"
        :style="modelSettingsStyle"
      >
        <ChatPaneModelSettingsMenu
          :effective-selected-model="effectiveSelectedModel"
          :temperature-draft="temperatureDraft"
          :top-p-draft="topPDraft"
          :presence-penalty-draft="presencePenaltyDraft"
          :frequency-penalty-draft="frequencyPenaltyDraft"
          :max-output-tokens-draft="maxOutputTokensDraft"
          :max-output-tokens-placeholder="maxOutputTokensPlaceholder"
          :model-settings-error="modelSettingsError"
          @update:temperature-draft="temperatureDraft = $event"
          @update:top-p-draft="topPDraft = $event"
          @update:presence-penalty-draft="presencePenaltyDraft = $event"
          @update:frequency-penalty-draft="frequencyPenaltyDraft = $event"
          @update:max-output-tokens-draft="maxOutputTokensDraft = $event"
          @reset="resetModelSettingsDraft"
          @apply="applyModelSettings"
        />
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
import pauseIcon from '@iconify-icons/lucide/pause';
import folderOpenIcon from '@iconify-icons/lucide/folder-open';
import rotateCcwIcon from '@iconify-icons/lucide/rotate-ccw';
import slidersHorizontalIcon from '@iconify-icons/lucide/sliders-horizontal';
import ChatMessageList from '@/components/ChatMessageList.vue';
import ChatPaneModelMenu from '@/components/chat/ChatPaneModelMenu.vue';
import ChatPaneModelSettingsMenu from '@/components/chat/ChatPaneModelSettingsMenu.vue';
import { useChatComposer } from '@/composables/useChatComposer';
import { useTaskModelSelector } from '@/composables/useTaskModelSelector';
import type { ComposerImageAttachment } from '@/composables/useChatComposer';

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
  setModel: [selection: { modelConfigId: number }];
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
  supportsVision: computed(() => supportsVision.value),
});

const {
  modelMenuAnchorRef,
  modelMenuPanelRef,
  modelMenuOpen,
  modelSettingsAnchorRef,
  modelSettingsPanelRef,
  modelSettingsOpen,
  providerGroups,
  modelItems,
  modelSearchQuery,
  modelsLoading,
  modelsRefreshing,
  modelMenuStyle,
  modelSettingsStyle,
  supportsVision,
  effectiveSelectedModel,
  modelButtonLabel,
  maxOutputTokensPlaceholder,
  isCustomWorkingDirectory,
  workingDirectoryLabel,
  workingDirectoryTooltip,
  filteredModelItems,
  temperatureDraft,
  topPDraft,
  presencePenaltyDraft,
  frequencyPenaltyDraft,
  maxOutputTokensDraft,
  modelSettingsError,
  toggleModelMenu,
  toggleModelSettingsMenu,
  pickWorkingDirectory,
  resetWorkingDirectory,
  selectModel,
  isModelActive,
  applyModelSettings,
  resetModelSettingsDraft,
  providerTypeLabel,
  closeAllMenus,
} = useTaskModelSelector({
  taskId: taskIdRef,
  disabled: disabledRef,
  settingsOpen: toRef(props, 'settingsOpen'),
  selectedModel: toRef(props, 'selectedModel'),
  selectedTemperature: toRef(props, 'selectedTemperature'),
  selectedTopP: toRef(props, 'selectedTopP'),
  selectedPresencePenalty: toRef(props, 'selectedPresencePenalty'),
  selectedFrequencyPenalty: toRef(props, 'selectedFrequencyPenalty'),
  selectedMaxOutputTokens: toRef(props, 'selectedMaxOutputTokens'),
  workingDirectory: toRef(props, 'workingDirectory'),
  workspacePath: toRef(props, 'workspacePath'),
  plusMenuOpen,
  closeComposerMenus: () => {
    plusMenuOpen.value = false;
  },
  emitSetModel: (selection) => emit('setModel', selection),
  emitSetModelSettings: (settings) => emit('setModelSettings', settings),
  emitSetWorkingDirectory: (path) => emit('setWorkingDirectory', path),
});

const previewImage = ref<ComposerImageAttachment | null>(null);

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
  () => {
    resetComposer();
  },
);

onMounted(() => {
  document.addEventListener('mousedown', handleDocumentPointerDown);
});

onUnmounted(() => {
  document.removeEventListener('mousedown', handleDocumentPointerDown);
});

function onComposerKeydown(event: KeyboardEvent) {
  handleComposerKeydown(event, submit);
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
  closeAllMenus();
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
</script>
