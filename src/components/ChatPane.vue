<template>
  <section class="panel flex min-h-0 overflow-visible flex-col">
    <div class="panel-header flex items-center gap-3">
      <div class="text-[12px] text-text-dim">
        {{ chat.length ? `${chat.length} messages` : 'No messages yet' }}
      </div>
    </div>

    <div ref="scrollContainer" class="min-h-0 flex-1 overflow-y-auto px-3 py-3">
      <div v-if="!chat.length" class="empty-state">
        <p class="text-sm text-text">No messages yet.</p>
        <p class="mt-1 text-xs text-text-dim">Start a task from here and March will persist the conversation into the active task.</p>
      </div>

      <article
        v-for="message in chat"
        :key="`${message.role}-${message.time}-${message.author}`"
        class="chat-row"
        :class="message.role === 'assistant' ? 'chat-row-assistant' : 'chat-row-user'"
      >
        <span class="message-avatar shrink-0">{{ message.author.slice(0, 1) }}</span>

        <div class="message-stack" :class="message.role === 'assistant' ? 'items-start' : 'items-end'">
          <div class="message-meta" :class="message.role === 'assistant' ? 'justify-start' : 'justify-end'">
            <span class="text-[13px] font-semibold text-text">{{ message.author }}</span>
            <time class="font-mono text-[11px] text-text-dim">{{ message.time }}</time>
          </div>

          <div
            class="message-bubble"
            :class="message.role === 'assistant' ? 'message-bubble-assistant' : 'message-bubble-user'"
          >
            <MarkdownRender
              v-if="message.role === 'assistant'"
              custom-id="ma-chat-message"
              :content="message.content"
              :final="true"
              :max-live-nodes="0"
              :render-batch-size="16"
              :render-batch-delay="8"
            />
            <p v-else class="whitespace-pre-wrap text-text">{{ message.content }}</p>

            <details v-if="message.tools?.length" class="message-tools">
              <summary class="cursor-pointer list-none text-[11px] uppercase tracking-[0.18em] text-text-muted">
                Tool summaries
              </summary>
              <ul class="mt-2 space-y-1">
                <li v-for="tool in message.tools" :key="`${tool.label}-${tool.summary}`" class="text-xs text-text-muted">
                  <span class="text-text">{{ tool.label }}</span>
                  <span class="text-text-dim"> - {{ tool.summary }}</span>
                </li>
              </ul>
            </details>
          </div>
        </div>
      </article>

      <article v-if="liveTurn" class="chat-row chat-row-assistant">
        <span class="message-avatar shrink-0">M</span>

        <div class="message-stack items-start">
          <div class="message-meta justify-start">
            <span class="text-[13px] font-semibold text-text">March</span>
            <time class="font-mono text-[11px] text-text-dim">...</time>
          </div>

          <div class="message-bubble message-bubble-assistant opacity-90">
            <div class="live-status-row">
              <span class="live-status-dots" aria-hidden="true">
                <span></span>
                <span></span>
                <span></span>
              </span>
              <span class="live-status-label">{{ liveTurn.statusLabel }}</span>
            </div>
            <MarkdownRender
              v-if="liveTurn.content"
              custom-id="ma-chat-streaming"
              :content="liveTurn.content"
              :final="liveTurn.state !== 'streaming'"
              :max-live-nodes="0"
              :render-batch-size="16"
              :render-batch-delay="8"
            />
            <p v-else class="mt-2 text-[13px] text-text-dim">
              {{ liveTurn.state === 'error' ? '这轮没有成功完成。' : 'March 正在处理这一轮请求。' }}
            </p>

            <div v-if="liveTurn.tools.length" class="live-tools" aria-label="Live tool summaries">
              <div v-for="tool in liveTurn.tools" :key="tool.id" class="live-tool-item">
                <span class="live-tool-state" :class="`live-tool-state-${tool.state}`"></span>
                <span class="live-tool-text">{{ tool.summary || tool.label }}</span>
              </div>
            </div>
          </div>
        </div>
      </article>

      <div ref="bottomAnchor" aria-hidden="true"></div>
    </div>

    <div class="shrink-0 p-3" style="border-top: 1px solid rgba(255, 255, 255, 0.08)">
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
              :disabled="disabled || sending"
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
            :disabled="disabled || sending"
            rows="1"
            @input="handleDraftInput"
            @click="updateMentionQueryFromCursor"
            @keyup="handleComposerKeyup"
            @keydown="handleComposerKeydown"
          ></textarea>

          <div class="chat-composer-toolbar">
            <div class="chat-composer-toolbar-group">
              <button class="composer-action" type="button" :disabled="disabled || sending" title="选择文件或目录" @click="togglePlusMenu">
                <span class="composer-action-icon">+</span>
              </button>
              <div ref="modelMenuAnchorRef" class="composer-model-anchor">
                <button class="composer-model-button" type="button" :disabled="disabled || sending" title="模型选择器" @click="toggleModelMenu">
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
                class="composer-send-button composer-send-button-paused"
                type="button"
                disabled
                aria-label="Pause generation"
                title="Pause generation"
              >
                <Icon :icon="pauseIcon" class="h-3.5 w-3.5" />
              </button>
              <button
                v-else
                class="composer-send-button"
                type="button"
                :disabled="disabled || composerIsEmpty"
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
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue';
import { Icon } from '@iconify/vue';
import { invoke } from '@tauri-apps/api/core';
import pauseIcon from '@iconify-icons/lucide/pause';
import MarkdownRender from 'markstream-vue';
import type { ChatMessage, LiveTurn, ProviderModelsView, WorkspaceEntryView } from '../data/mock';

type MentionKind = 'file' | 'directory';
type MentionItem = {
  path: string;
  kind: MentionKind;
};

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
  chat: ChatMessage[];
  liveTurn?: LiveTurn;
  disabled?: boolean;
  sending?: boolean;
  taskId?: number | null;
  selectedModel?: string;
}>();

const emit = defineEmits<{
  send: [payload: { content: string; directories: string[] }];
  openFiles: [paths: string[]];
  setModel: [model: string];
}>();

const draft = ref('');
const mentions = ref<MentionItem[]>([]);
const scrollContainer = ref<HTMLElement | null>(null);
const bottomAnchor = ref<HTMLElement | null>(null);
const composerRef = ref<HTMLTextAreaElement | null>(null);
const composerRootRef = ref<HTMLElement | null>(null);
const modelMenuAnchorRef = ref<HTMLElement | null>(null);
const modelMenuPanelRef = ref<HTMLElement | null>(null);
const modelSearchRef = ref<HTMLInputElement | null>(null);
const composerMaxHeight = 160;
const activeSearchQuery = ref('');
const searchResults = ref<WorkspaceEntryView[]>([]);
const searchLoading = ref(false);
const highlightedResultIndex = ref(0);
const searchPanelOpen = ref(false);
const searchMode = ref<'smart' | 'file' | 'directory'>('smart');
const mentionQueryRange = ref<{ start: number; end: number } | null>(null);
const plusMenuOpen = ref(false);
const modelMenuOpen = ref(false);
const availableModels = ref<string[]>([]);
const modelSearchQuery = ref('');
const modelsLoading = ref(false);
const modelsRefreshing = ref(false);
const resolvedCurrentModel = ref('');
const modelMenuStyle = ref<Record<string, string>>({});
const lastSearchQuery = ref('');
const lastSearchMode = ref<'smart' | 'file' | 'directory' | null>(null);
let activeModelRequestId = 0;

const composerIsEmpty = computed(() => !draft.value.trim() && mentions.value.length === 0);
const effectiveSelectedModel = computed(() => props.selectedModel?.trim() || resolvedCurrentModel.value.trim());
const modelButtonLabel = computed(() => effectiveSelectedModel.value || '选择模型');
const filteredAvailableModels = computed(() => {
  const query = modelSearchQuery.value.trim().toLowerCase();
  if (!query) {
    return availableModels.value;
  }
  return availableModels.value.filter((model) => model.toLowerCase().includes(query));
});
const searchPanelLabel = computed(() => {
  if (searchMode.value === 'file') {
    return '选择文件';
  }
  if (searchMode.value === 'directory') {
    return '选择目录';
  }
  return '@ 引用';
});

function scrollToBottom(behavior: ScrollBehavior = 'smooth') {
  if (bottomAnchor.value) {
    bottomAnchor.value.scrollIntoView({ behavior, block: 'end' });
    return;
  }

  if (scrollContainer.value) {
    scrollContainer.value.scrollTo({
      top: scrollContainer.value.scrollHeight,
      behavior,
    });
  }
}

watch(
  () => props.chat.length,
  async () => {
    await nextTick();
    scrollToBottom('smooth');
  },
);

watch(
  () => props.liveTurn,
  async (turn, previousTurn) => {
    if (!turn) {
      return;
    }

    await nextTick();
    scrollToBottom(previousTurn ? 'auto' : 'smooth');
  },
  { deep: true },
);

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
    mentions.value = [];
    draft.value = '';
    closeAllMenus();
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

function handleDraftInput() {
  syncComposerHeight();
  updateMentionQueryFromCursor();
}

function handleComposerKeydown(event: KeyboardEvent) {
  if (searchPanelOpen.value && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
    event.preventDefault();
    if (!searchResults.value.length) {
      return;
    }
    const delta = event.key === 'ArrowDown' ? 1 : -1;
    highlightedResultIndex.value =
      (highlightedResultIndex.value + delta + searchResults.value.length) % searchResults.value.length;
    return;
  }

  if (searchPanelOpen.value && event.key === 'Enter' && !event.shiftKey) {
    const entry = searchResults.value[highlightedResultIndex.value];
    if (entry) {
      event.preventDefault();
      void selectWorkspaceEntry(entry);
      return;
    }
  }

  if (event.key === 'Escape') {
    closeAllMenus();
    return;
  }

  if (event.key === 'Enter' && !event.shiftKey) {
    event.preventDefault();
    submit();
  }
}

function handleComposerKeyup(event: KeyboardEvent) {
  if (isModifierOnlyKey(event.key)) {
    return;
  }
  void updateMentionQueryFromCursor();
}

async function updateMentionQueryFromCursor() {
  const textarea = composerRef.value;
  if (!textarea) {
    return;
  }

  if (searchMode.value !== 'smart') {
    return;
  }

  const cursor = textarea.selectionStart ?? draft.value.length;
  const prefix = draft.value.slice(0, cursor);
  const match = prefix.match(/(^|\s)@([^\s@]*)$/);
  if (!match || typeof match.index !== 'number') {
    searchPanelOpen.value = false;
    mentionQueryRange.value = null;
    activeSearchQuery.value = '';
    lastSearchQuery.value = '';
    lastSearchMode.value = null;
    return;
  }

  const query = match[2] ?? '';
  const atIndex = match.index + match[1].length;
  mentionQueryRange.value = { start: atIndex, end: cursor };
  activeSearchQuery.value = query;
  await loadSearchResults(query, 'smart');
}

async function loadSearchResults(query: string, mode: 'smart' | 'file' | 'directory') {
  if (props.disabled || !props.taskId) {
    return;
  }

  if (
    searchPanelOpen.value &&
    lastSearchQuery.value === query &&
    lastSearchMode.value === mode &&
    searchResults.value.length > 0
  ) {
    return;
  }

  searchMode.value = mode;
  searchPanelOpen.value = true;
  plusMenuOpen.value = false;
  modelMenuOpen.value = false;
  searchLoading.value = true;
  try {
    searchResults.value = await invoke<WorkspaceEntryView[]>('search_workspace_entries', {
      input: {
        query,
        kind: mode === 'smart' ? undefined : mode,
        limit: 12,
      },
    });
    lastSearchQuery.value = query;
    lastSearchMode.value = mode;
    highlightedResultIndex.value = 0;
  } finally {
    searchLoading.value = false;
  }
}

function openSearchFromMenu(mode: 'file' | 'directory') {
  activeSearchQuery.value = '';
  mentionQueryRange.value = null;
  void loadSearchResults('', mode);
}

async function selectWorkspaceEntry(entry: WorkspaceEntryView) {
  if (mentions.value.some((item) => item.path === entry.path && item.kind === entry.kind)) {
    closeSearchPanel();
    return;
  }

  mentions.value = [
    ...mentions.value,
    {
      path: entry.path,
      kind: entry.kind,
    },
  ];

  if (entry.kind === 'file') {
    emit('openFiles', [entry.path]);
  }

  if (mentionQueryRange.value) {
    const { start, end } = mentionQueryRange.value;
    draft.value = `${draft.value.slice(0, start)}${draft.value.slice(end)}`.replace(/\s{2,}/g, ' ').trimStart();
    await nextTick();
    composerRef.value?.focus();
    const nextCursor = start;
    composerRef.value?.setSelectionRange(nextCursor, nextCursor);
  }

  closeSearchPanel();
}

function removeMention(path: string, kind: MentionKind) {
  mentions.value = mentions.value.filter((item) => !(item.path === path && item.kind === kind));
}

async function toggleModelMenu() {
  if (!modelMenuOpen.value) {
    primeModelMenu();
    plusMenuOpen.value = false;
    searchPanelOpen.value = false;
    modelSearchQuery.value = '';
    modelMenuOpen.value = true;
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

function togglePlusMenu() {
  plusMenuOpen.value = !plusMenuOpen.value;
  if (plusMenuOpen.value) {
    modelMenuOpen.value = false;
    searchPanelOpen.value = false;
  }
}

function closeSearchPanel() {
  searchPanelOpen.value = false;
  mentionQueryRange.value = null;
  lastSearchQuery.value = '';
  lastSearchMode.value = null;
}

function closeAllMenus() {
  plusMenuOpen.value = false;
  modelMenuOpen.value = false;
  modelSearchQuery.value = '';
  closeSearchPanel();
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

function handleDocumentPointerDown(event: MouseEvent) {
  if (!(event.target instanceof Node)) {
    return;
  }

  const root = composerRootRef.value;
  const modelMenu = modelMenuPanelRef.value;
  const modelAnchor = modelMenuAnchorRef.value;
  const clickedInsideModelMenu = !!modelMenu?.contains(event.target);
  const clickedOnModelAnchor = !!modelAnchor?.contains(event.target);

  if (modelMenuOpen.value) {
    if (!clickedInsideModelMenu && !clickedOnModelAnchor) {
      modelMenuOpen.value = false;
      modelSearchQuery.value = '';
    }
  }

  if (clickedInsideModelMenu) {
    return;
  }

  if (!root || root.contains(event.target)) {
    return;
  }
  closeAllMenus();
}

function submit() {
  const content = draft.value.trim();
  if (( !content && mentions.value.length === 0) || props.disabled || props.sending) {
    return;
  }

  const directories = mentions.value.filter((item) => item.kind === 'directory').map((item) => item.path);
  emit('send', {
    content,
    directories,
  });
  draft.value = '';
  mentions.value = [];
  closeAllMenus();
  syncComposerHeight(true);
}

function syncComposerHeight(reset = false) {
  if (!composerRef.value) {
    return;
  }

  if (reset) {
    composerRef.value.style.height = 'auto';
    composerRef.value.style.overflowY = 'hidden';
    return;
  }

  composerRef.value.style.height = 'auto';
  const nextHeight = Math.min(composerRef.value.scrollHeight, composerMaxHeight);
  composerRef.value.style.height = `${nextHeight}px`;
  composerRef.value.style.overflowY = composerRef.value.scrollHeight > composerMaxHeight ? 'auto' : 'hidden';
}

function focusComposer() {
  composerRef.value?.focus();
}

function isModifierOnlyKey(key: string) {
  return key === 'Shift' || key === 'Control' || key === 'Alt' || key === 'Meta';
}

defineExpose({
  focusComposer,
});
</script>
