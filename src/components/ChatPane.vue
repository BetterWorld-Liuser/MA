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

        <div class="chat-composer">
          <div v-if="mentions.length" class="chat-composer-chips" aria-label="Referenced paths">
            <button
              v-for="chip in mentions"
              :key="`${chip.kind}:${chip.path}`"
              class="mention-chip"
              :class="chip.kind === 'directory' ? 'mention-chip-directory' : ''"
              type="button"
              :disabled="disabled || interactionLocked"
              @click="removeMention(chip.path, chip.kind)"
            >
              <span class="mention-chip-kind">{{ chip.kind === 'directory' ? 'DIR' : 'FILE' }}</span>
              <span class="mention-chip-label">{{ chip.path }}</span>
              <span class="mention-chip-remove" aria-hidden="true">×</span>
            </button>
          </div>

          <textarea
            id="message-input"
            ref="composerRef"
            v-model="draft"
            class="chat-composer-input"
            placeholder="帮我重构认证逻辑，必要时 @ 文件或目录。"
            :disabled="disabled || interactionLocked"
            rows="1"
            @input="handleDraftInput"
            @click="updateMentionQueryFromCursor"
            @keyup="handleComposerKeyup"
            @keydown="onComposerKeydown"
          ></textarea>

          <div class="chat-composer-toolbar">
            <div class="chat-composer-toolbar-group">
              <button class="composer-action" type="button" :disabled="disabled || interactionLocked" title="选择文件或目录" @click="togglePlusMenu">
                <span class="composer-action-icon">+</span>
              </button>
              <div ref="modelMenuAnchorRef" class="composer-model-anchor">
                <button class="composer-model-button" type="button" :disabled="disabled || interactionLocked" title="模型选择器" @click="toggleModelMenu">
                  <span class="truncate">{{ modelButtonLabel }}</span>
                  <span aria-hidden="true">∨</span>
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
              :key="`${entry.kind}:${entry.path}`"
              class="composer-popover-item"
              :class="index === highlightedResultIndex ? 'composer-popover-item-active' : ''"
              type="button"
              @mousedown.prevent="selectWorkspaceEntry(entry)"
              @mouseenter="highlightedResultIndex = index"
            >
              <span class="composer-popover-item-kind">{{ entry.kind === 'directory' ? '目录' : '文件' }}</span>
              <span class="composer-popover-item-path">{{ entry.path }}</span>
            </button>
          </div>
          <div v-else class="composer-popover-empty">没有匹配结果</div>
        </div>

        <div v-if="plusMenuOpen" class="composer-menu">
          <button class="composer-menu-item" type="button" @mousedown.prevent="openSearchFromMenu('file')">选择文件…</button>
          <button class="composer-menu-item" type="button" @mousedown.prevent="openSearchFromMenu('directory')">选择目录…</button>
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
          <button
            v-for="model in filteredAvailableModels"
            :key="model"
            class="composer-menu-item composer-menu-item-model"
            :class="model === effectiveSelectedModel ? 'composer-menu-item-active' : ''"
            type="button"
            @mousedown.prevent="selectModel(model)"
          >
            <span>{{ model }}</span>
            <span v-if="model === effectiveSelectedModel">✓</span>
          </button>
          <div v-if="!availableModels.length && !modelsLoading" class="composer-menu-empty">当前没有可读模型列表</div>
          <div v-else-if="!filteredAvailableModels.length && !modelsLoading" class="composer-menu-empty">没有匹配的模型</div>
        </div>
      </div>
    </Teleport>
  </section>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, toRef, watch } from 'vue';
import { Icon } from '@iconify/vue';
import { invoke } from '@tauri-apps/api/core';
import pauseIcon from '@iconify-icons/lucide/pause';
import ChatMessageList from '@/components/ChatMessageList.vue';
import { useChatComposer } from '@/composables/useChatComposer';
import type { ProviderModelsView } from '../data/mock';

type CachedModelList = {
  providerKey: string;
  currentModel: string;
  availableModels: string[];
};

// 模型列表读取仍然可能依赖 provider 网络请求。
// 这里保留一个前端进程内缓存，让菜单可以先秒开最近一次成功结果，再异步刷新。
const providerModelCache = new Map<string, CachedModelList>();
const taskProviderCacheKey = new Map<number, string>();

const props = defineProps<{
  chat: import('../data/mock').ChatMessage[];
  liveTurn?: import('../data/mock').LiveTurn;
  disabled?: boolean;
  sending?: boolean;
  interactionLocked?: boolean;
  cancelling?: boolean;
  taskId?: number | null;
  selectedModel?: string;
}>();

const emit = defineEmits<{
  send: [payload: { content: string; directories: string[] }];
  openFiles: [paths: string[]];
  setModel: [model: string];
  cancelTurn: [];
}>();

const disabledRef = computed(() => !!props.disabled);
const interactionLockedRef = computed(() => !!props.interactionLocked);
const taskIdRef = toRef(props, 'taskId');

const {
  draft,
  mentions,
  composerRef,
  composerRootRef,
  activeSearchQuery,
  searchResults,
  searchLoading,
  highlightedResultIndex,
  searchPanelOpen,
  plusMenuOpen,
  composerIsEmpty,
  searchPanelLabel,
  handleDraftInput,
  handleComposerKeyup,
  handleComposerKeydown,
  updateMentionQueryFromCursor,
  openSearchFromMenu,
  selectWorkspaceEntry,
  removeMention,
  togglePlusMenu,
  handleDocumentPointerDown,
  syncComposerHeight,
  focusComposer,
  resetComposer,
} = useChatComposer({
  disabled: disabledRef,
  sending: interactionLockedRef,
  taskId: taskIdRef,
  onOpenFiles: (paths) => emit('openFiles', paths),
});

const modelMenuAnchorRef = ref<HTMLElement | null>(null);
const modelMenuPanelRef = ref<HTMLElement | null>(null);
const modelSearchRef = ref<HTMLInputElement | null>(null);
const modelMenuOpen = ref(false);
const availableModels = ref<string[]>([]);
const modelSearchQuery = ref('');
const modelsLoading = ref(false);
const modelsRefreshing = ref(false);
const resolvedCurrentModel = ref('');
const modelMenuStyle = ref<Record<string, string>>({});
let activeModelRequestId = 0;

const effectiveSelectedModel = computed(() => props.selectedModel?.trim() || resolvedCurrentModel.value.trim());
const modelButtonLabel = computed(() => effectiveSelectedModel.value || '选择模型');
const filteredAvailableModels = computed(() => {
  const query = modelSearchQuery.value.trim().toLowerCase();
  if (!query) {
    return availableModels.value;
  }
  return availableModels.value.filter((model) => model.toLowerCase().includes(query));
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
    restoreModelStateFromCache(taskId);
    seedModelListFromCurrentSelection();
    void refreshModels();
  },
);

watch(
  () => props.selectedModel,
  (model) => {
    if (model?.trim()) {
      resolvedCurrentModel.value = model.trim();
      seedModelListFromCurrentSelection();
    }
  },
  { immediate: true },
);

watch([modelMenuOpen, filteredAvailableModels, modelSearchQuery], async ([open]) => {
  if (!open) {
    return;
  }
  await nextTick();
  syncModelMenuPosition();
});

onMounted(() => {
  document.addEventListener('mousedown', handleDocumentPointerDown);
  window.addEventListener('resize', syncModelMenuPosition);
  window.addEventListener('scroll', syncModelMenuPosition, true);
  restoreModelStateFromCache(props.taskId);
  seedModelListFromCurrentSelection();
  void refreshModels();
});

onUnmounted(() => {
  document.removeEventListener('mousedown', handleDocumentPointerDown);
  window.removeEventListener('resize', syncModelMenuPosition);
  window.removeEventListener('scroll', syncModelMenuPosition, true);
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
  } else {
    modelSearchQuery.value = '';
  }
  modelMenuOpen.value = false;
}

function primeModelMenu() {
  restoreModelStateFromCache(props.taskId);
  seedModelListFromCurrentSelection();
  void refreshModels();
}

function restoreModelStateFromCache(taskId?: number | null) {
  if (!taskId) {
    resolvedCurrentModel.value = '';
    availableModels.value = [];
    return;
  }

  const providerKey = taskProviderCacheKey.get(taskId);
  const cached =
    (providerKey ? providerModelCache.get(providerKey) : undefined) ??
    (providerModelCache.size === 1 ? Array.from(providerModelCache.values())[0] : undefined);

  if (!cached) {
    return;
  }

  resolvedCurrentModel.value = cached.currentModel;
  availableModels.value = [...cached.availableModels];
}

function seedModelListFromCurrentSelection() {
  const selected = props.selectedModel?.trim();
  if (!selected) {
    return;
  }

  resolvedCurrentModel.value = selected;
  if (!availableModels.value.includes(selected)) {
    availableModels.value = [selected, ...availableModels.value];
  }
}

async function refreshModels() {
  if (!props.taskId) {
    return;
  }

  const requestId = ++activeModelRequestId;
  const hasWarmData = availableModels.value.length > 0;
  modelsLoading.value = !hasWarmData;
  modelsRefreshing.value = hasWarmData;
  try {
    const response = await invoke<ProviderModelsView>('list_provider_models', {
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

function applyProviderModels(response: ProviderModelsView, taskId: number) {
  const normalizedModels = Array.from(
    new Set(
      [response.current_model, ...response.available_models]
        .map((model) => model.trim())
        .filter(Boolean),
    ),
  );

  const cacheEntry: CachedModelList = {
    providerKey: response.provider_cache_key,
    currentModel: response.current_model,
    availableModels: normalizedModels,
  };

  providerModelCache.set(response.provider_cache_key, cacheEntry);
  taskProviderCacheKey.set(taskId, response.provider_cache_key);
  resolvedCurrentModel.value = cacheEntry.currentModel;
  availableModels.value = [...cacheEntry.availableModels];
}

function selectModel(model: string) {
  resolvedCurrentModel.value = model;
  if (!availableModels.value.includes(model)) {
    availableModels.value = [model, ...availableModels.value];
  }
  emit('setModel', model);
  modelMenuOpen.value = false;
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

function submit() {
  const content = draft.value.trim();
  if ((!content && mentions.value.length === 0) || props.disabled || props.interactionLocked) {
    return;
  }

  const directories = mentions.value.filter((item) => item.kind === 'directory').map((item) => item.path);
  emit('send', {
    content,
    directories,
  });
  resetComposer();
}

defineExpose({
  focusComposer,
});
</script>
