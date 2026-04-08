import { computed, ref, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { MentionTargetView, SearchSkillView, WorkspaceEntryView } from '@/data/mock';
import {
  appendToken,
  isImagePath,
  type ComposerSearchResult,
  type MentionItem,
  type SearchMode,
} from './chatComposerShared';

type UseComposerMentionsOptions = {
  disabled: Ref<boolean>;
  taskId: Ref<number | null | undefined>;
  draft: Ref<string>;
  composerRef: Ref<HTMLTextAreaElement | null>;
  plusMenuOpen: Ref<boolean>;
  attachWorkspaceImage: (path: string) => Promise<boolean>;
  showComposerNotice: (message: string) => void;
};

export function useComposerMentions({
  disabled,
  taskId,
  draft,
  composerRef,
  plusMenuOpen,
  attachWorkspaceImage,
}: UseComposerMentionsOptions) {
  const mentions = ref<MentionItem[]>([]);
  const activeSearchQuery = ref('');
  const searchResults = ref<ComposerSearchResult[]>([]);
  const searchLoading = ref(false);
  const highlightedResultIndex = ref(0);
  const searchPanelOpen = ref(false);
  const searchMode = ref<SearchMode>('smart');
  const searchQueryRange = ref<{ start: number; end: number } | null>(null);
  const lastSearchQuery = ref('');
  const lastSearchMode = ref<SearchMode | null>(null);

  const searchPanelLabel = computed(() => {
    if (searchMode.value === 'file') {
      return '选择文件';
    }
    if (searchMode.value === 'directory') {
      return '选择目录';
    }
    if (searchMode.value === 'skill') {
      return '选择技能';
    }
    return '@ 引用';
  });

  async function updateMentionQueryFromCursor() {
    const textarea = composerRef.value;
    if (!textarea) {
      return;
    }

    if (searchMode.value === 'file' || searchMode.value === 'directory') {
      return;
    }

    const cursor = textarea.selectionStart ?? draft.value.length;
    const prefix = draft.value.slice(0, cursor);

    const mentionMatch = prefix.match(/(^|\s)@([^\s@]*)$/);
    if (mentionMatch && typeof mentionMatch.index === 'number') {
      const query = mentionMatch[2] ?? '';
      const atIndex = mentionMatch.index + mentionMatch[1].length;
      searchQueryRange.value = { start: atIndex, end: cursor };
      activeSearchQuery.value = query;
      await loadSearchResults(query, 'smart');
      return;
    }

    const slashMatch = prefix.match(/(^|\s)\/([^\s/]*)$/);
    if (slashMatch && typeof slashMatch.index === 'number') {
      const query = slashMatch[2] ?? '';
      const slashIndex = slashMatch.index + slashMatch[1].length;
      searchQueryRange.value = { start: slashIndex, end: cursor };
      activeSearchQuery.value = query;
      await loadSearchResults(query, 'skill');
      return;
    }

    closeSearchPanel();
    activeSearchQuery.value = '';
  }

  async function loadSearchResults(query: string, mode: SearchMode) {
    if (disabled.value || !taskId.value) {
      return;
    }

    if (
      searchPanelOpen.value
      && lastSearchQuery.value === query
      && lastSearchMode.value === mode
      && searchResults.value.length > 0
    ) {
      return;
    }

    searchMode.value = mode;
    searchPanelOpen.value = true;
    plusMenuOpen.value = false;
    searchLoading.value = true;
    try {
      if (mode === 'smart') {
        searchResults.value = await invoke<MentionTargetView[]>('search_mentions', {
          input: {
            taskId: taskId.value,
            query,
            limit: 12,
          },
        });
      } else if (mode === 'skill') {
        searchResults.value = await invoke<SearchSkillView[]>('search_skills', {
          input: {
            taskId: taskId.value,
            query,
            limit: 12,
          },
        });
      } else {
        const entries = await invoke<WorkspaceEntryView[]>('search_workspace_entries', {
          input: {
            taskId: taskId.value,
            query,
            kind: mode,
            limit: 12,
          },
        });
        searchResults.value = entries;
      }
      lastSearchQuery.value = query;
      lastSearchMode.value = mode;
      highlightedResultIndex.value = 0;
    } finally {
      searchLoading.value = false;
    }
  }

  function openSearchFromMenu(mode: 'file' | 'directory' | 'skill') {
    activeSearchQuery.value = '';
    searchQueryRange.value = null;
    void loadSearchResults('', mode);
  }

  async function selectWorkspaceEntry(entry: ComposerSearchResult) {
    if (entry.kind === 'skill') {
      if (mentions.value.some((item) => item.path === entry.path && item.kind === 'skill')) {
        closeSearchPanel();
        return;
      }

      mentions.value = [
        ...mentions.value,
        {
          path: entry.path,
          kind: 'skill',
          label: entry.name,
          description: entry.description,
        },
      ];
      removeSearchQueryFromDraft();
      closeSearchPanel();
      return;
    }

    if (entry.kind === 'agent') {
      insertAgentMention(entry.name);
      closeSearchPanel();
      return;
    }

    if (entry.kind === 'file' && isImagePath(entry.path)) {
      const attached = await attachWorkspaceImage(entry.path);
      if (attached) {
        removeSearchQueryFromDraft();
      }
      closeSearchPanel();
      return;
    }

    if (mentions.value.some((item) => item.path === entry.path && item.kind === entry.kind)) {
      closeSearchPanel();
      return;
    }

    mentions.value = [
      ...mentions.value,
      {
        path: entry.path,
        kind: entry.kind,
        label: entry.path,
      },
    ];
    removeSearchQueryFromDraft();
    closeSearchPanel();
  }

  function removeMention(path: string, kind: MentionItem['kind']) {
    mentions.value = mentions.value.filter((item) => !(item.path === path && item.kind === kind));
  }

  function clearMentions() {
    mentions.value = [];
    activeSearchQuery.value = '';
    closeSearchPanel();
  }

  function closeSearchPanel() {
    searchPanelOpen.value = false;
    searchQueryRange.value = null;
    lastSearchQuery.value = '';
    lastSearchMode.value = null;
    searchMode.value = 'smart';
  }

  function removeSearchQueryFromDraft() {
    if (!searchQueryRange.value) {
      return;
    }

    const { start, end } = searchQueryRange.value;
    draft.value = `${draft.value.slice(0, start)}${draft.value.slice(end)}`.replace(/\s{2,}/g, ' ').trimStart();
    composerRef.value?.focus();
    composerRef.value?.setSelectionRange(start, start);
    searchQueryRange.value = null;
  }

  function insertAgentMention(name: string) {
    const mention = `@${name}`;
    if (!searchQueryRange.value) {
      draft.value = appendToken(draft.value, mention);
      return;
    }

    const { start, end } = searchQueryRange.value;
    const prefix = draft.value.slice(0, start);
    const suffix = draft.value.slice(end).replace(/^\s*/, '');
    draft.value = `${prefix}${mention} ${suffix}`.trimEnd();
    const cursor = prefix.length + mention.length + 1;
    composerRef.value?.focus();
    composerRef.value?.setSelectionRange(cursor, cursor);
    searchQueryRange.value = null;
  }

  return {
    mentions,
    activeSearchQuery,
    searchResults,
    searchLoading,
    highlightedResultIndex,
    searchPanelOpen,
    searchPanelLabel,
    updateMentionQueryFromCursor,
    openSearchFromMenu,
    selectWorkspaceEntry,
    removeMention,
    clearMentions,
    closeSearchPanel,
  };
}
