import { computed, ref, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { WorkspaceEntryView } from '@/data/mock';

export type MentionKind = 'file' | 'directory';

export type MentionItem = {
  path: string;
  kind: MentionKind;
};

export function useChatComposer(options: {
  disabled: Ref<boolean>;
  sending: Ref<boolean>;
  taskId: Ref<number | null | undefined>;
  onOpenFiles: (paths: string[]) => void;
}) {
  const { disabled, sending, taskId, onOpenFiles } = options;

  const draft = ref('');
  const mentions = ref<MentionItem[]>([]);
  const composerRef = ref<HTMLTextAreaElement | null>(null);
  const composerRootRef = ref<HTMLElement | null>(null);
  const composerMaxHeight = 160;
  const activeSearchQuery = ref('');
  const searchResults = ref<WorkspaceEntryView[]>([]);
  const searchLoading = ref(false);
  const highlightedResultIndex = ref(0);
  const searchPanelOpen = ref(false);
  const searchMode = ref<'smart' | 'file' | 'directory'>('smart');
  const mentionQueryRange = ref<{ start: number; end: number } | null>(null);
  const plusMenuOpen = ref(false);
  const lastSearchQuery = ref('');
  const lastSearchMode = ref<'smart' | 'file' | 'directory' | null>(null);

  const composerIsEmpty = computed(() => !draft.value.trim() && mentions.value.length === 0);
  const searchPanelLabel = computed(() => {
    if (searchMode.value === 'file') {
      return '选择文件';
    }
    if (searchMode.value === 'directory') {
      return '选择目录';
    }
    return '@ 引用';
  });

  function handleDraftInput() {
    syncComposerHeight();
    void updateMentionQueryFromCursor();
  }

  function handleComposerKeyup(event: KeyboardEvent) {
    if (isModifierOnlyKey(event.key)) {
      return;
    }
    void updateMentionQueryFromCursor();
  }

  function handleComposerKeydown(event: KeyboardEvent, submit: () => void) {
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
    if (disabled.value || !taskId.value) {
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
      onOpenFiles([entry.path]);
    }

    if (mentionQueryRange.value) {
      const { start, end } = mentionQueryRange.value;
      draft.value = `${draft.value.slice(0, start)}${draft.value.slice(end)}`.replace(/\s{2,}/g, ' ').trimStart();
      composerRef.value?.focus();
      const nextCursor = start;
      composerRef.value?.setSelectionRange(nextCursor, nextCursor);
    }

    closeSearchPanel();
  }

  function removeMention(path: string, kind: MentionKind) {
    mentions.value = mentions.value.filter((item) => !(item.path === path && item.kind === kind));
  }

  function togglePlusMenu() {
    plusMenuOpen.value = !plusMenuOpen.value;
    if (plusMenuOpen.value) {
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
    closeSearchPanel();
  }

  function handleDocumentPointerDown(event: MouseEvent) {
    if (!(event.target instanceof Node)) {
      return;
    }

    const root = composerRootRef.value;
    if (!root || root.contains(event.target)) {
      return;
    }
    closeAllMenus();
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

  function resetComposer() {
    draft.value = '';
    mentions.value = [];
    closeAllMenus();
    syncComposerHeight(true);
  }

  function isModifierOnlyKey(key: string) {
    return key === 'Shift' || key === 'Control' || key === 'Alt' || key === 'Meta';
  }

  return {
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
    closeAllMenus,
    handleDocumentPointerDown,
    syncComposerHeight,
    focusComposer,
    resetComposer,
  };
}
