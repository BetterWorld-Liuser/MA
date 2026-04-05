import { computed, ref, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  ChatImageAttachment,
  MentionTargetView,
  WorkspaceEntryView,
  WorkspaceImageView,
} from '@/data/mock';

export type MentionKind = 'file' | 'directory';

export type MentionItem = {
  path: string;
  kind: MentionKind;
};

export type ComposerImageAttachment = ChatImageAttachment;

type SearchMode = 'smart' | 'file' | 'directory';

const IMAGE_EXTENSIONS = new Set(['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg']);

export function useChatComposer(options: {
  disabled: Ref<boolean>;
  sending: Ref<boolean>;
  taskId: Ref<number | null | undefined>;
  supportsVision: Ref<boolean>;
}) {
  const {
    disabled,
    sending,
    taskId,
    supportsVision,
  } = options;

  const draft = ref('');
  const mentions = ref<MentionItem[]>([]);
  const imageAttachments = ref<ComposerImageAttachment[]>([]);
  const composerRef = ref<HTMLTextAreaElement | null>(null);
  const composerRootRef = ref<HTMLElement | null>(null);
  const imageInputRef = ref<HTMLInputElement | null>(null);
  const composerMaxHeight = 160;
  const activeSearchQuery = ref('');
  const searchResults = ref<MentionTargetView[]>([]);
  const searchLoading = ref(false);
  const highlightedResultIndex = ref(0);
  const searchPanelOpen = ref(false);
  const searchMode = ref<SearchMode>('smart');
  const mentionQueryRange = ref<{ start: number; end: number } | null>(null);
  const plusMenuOpen = ref(false);
  const lastSearchQuery = ref('');
  const lastSearchMode = ref<SearchMode | null>(null);
  const composerNotice = ref('');
  const dragActive = ref(false);
  let composerNoticeTimer: ReturnType<typeof setTimeout> | null = null;

  const composerIsEmpty = computed(() =>
    !draft.value.trim() && mentions.value.length === 0 && imageAttachments.value.length === 0,
  );
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
    if (!textarea || searchMode.value !== 'smart') {
      return;
    }

    const cursor = textarea.selectionStart ?? draft.value.length;
    const prefix = draft.value.slice(0, cursor);
    const match = prefix.match(/(^|\s)@([^\s@]*)$/);
    if (!match || typeof match.index !== 'number') {
      closeSearchPanel();
      activeSearchQuery.value = '';
      return;
    }

    const query = match[2] ?? '';
    const atIndex = match.index + match[1].length;
    mentionQueryRange.value = { start: atIndex, end: cursor };
    activeSearchQuery.value = query;
    await loadSearchResults(query, 'smart');
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

  function openSearchFromMenu(mode: 'file' | 'directory') {
    activeSearchQuery.value = '';
    mentionQueryRange.value = null;
    void loadSearchResults('', mode);
  }

  async function selectWorkspaceEntry(entry: MentionTargetView) {
    if (entry.kind === 'agent') {
      insertAgentMention(entry.name);
      closeSearchPanel();
      return;
    }

    if (entry.kind === 'file' && isImagePath(entry.path)) {
      if (!supportsVision.value) {
        showComposerNotice('当前模型不支持图片输入');
        removeMentionQueryFromDraft();
        closeSearchPanel();
        return;
      }

      if (!taskId.value) {
        return;
      }

      const image = await invoke<WorkspaceImageView>('load_workspace_image', {
        input: {
          taskId: taskId.value,
          path: entry.path,
        },
      });
      addImageAttachment({
        id: `workspace:${image.path}`,
        name: image.name,
        previewUrl: image.dataUrl,
        mediaType: image.mediaType,
        sourcePath: image.path,
      });
      removeMentionQueryFromDraft();
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
      },
    ];
    removeMentionQueryFromDraft();
    closeSearchPanel();
  }

  function removeMention(path: string, kind: MentionKind) {
    mentions.value = mentions.value.filter((item) => !(item.path === path && item.kind === kind));
  }

  function removeImageAttachment(id: string) {
    imageAttachments.value = imageAttachments.value.filter((item) => item.id !== id);
  }

  function togglePlusMenu() {
    plusMenuOpen.value = !plusMenuOpen.value;
    if (plusMenuOpen.value) {
      searchPanelOpen.value = false;
    }
  }

  function triggerImagePicker() {
    plusMenuOpen.value = false;
    if (!supportsVision.value) {
      showComposerNotice('当前模型不支持图片输入');
      return;
    }
    imageInputRef.value?.click();
  }

  async function handleImageFileSelection(event: Event) {
    const target = event.target as HTMLInputElement | null;
    const files = Array.from(target?.files ?? []);
    await attachImageFiles(files);
    if (target) {
      target.value = '';
    }
  }

  async function handlePaste(event: ClipboardEvent) {
    const files = extractImageFiles(event.clipboardData?.items);
    if (!files.length) {
      return;
    }

    event.preventDefault();
    await attachImageFiles(files);
  }

  async function handleDrop(event: DragEvent) {
    dragActive.value = false;
    const files = Array.from(event.dataTransfer?.files ?? []).filter((file) => file.type.startsWith('image/'));
    if (!files.length) {
      return;
    }

    event.preventDefault();
    await attachImageFiles(files);
  }

  function handleDragOver(event: DragEvent) {
    if (!hasImageFile(event.dataTransfer?.items)) {
      dragActive.value = false;
      return;
    }

    event.preventDefault();
    dragActive.value = true;
  }

  function handleDragLeave(event: DragEvent) {
    if (!composerRootRef.value?.contains(event.relatedTarget as Node | null)) {
      dragActive.value = false;
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
    imageAttachments.value = [];
    dragActive.value = false;
    composerNotice.value = '';
    closeAllMenus();
    syncComposerHeight(true);
  }

  function showComposerNotice(message: string) {
    composerNotice.value = message;
    if (composerNoticeTimer) {
      clearTimeout(composerNoticeTimer);
    }
    composerNoticeTimer = setTimeout(() => {
      composerNotice.value = '';
      composerNoticeTimer = null;
    }, 2200);
  }

  async function attachImageFiles(files: File[]) {
    if (!files.length) {
      return;
    }
    if (!supportsVision.value) {
      showComposerNotice('当前模型不支持图片输入');
      return;
    }

    const imageFiles = files.filter((file) => file.type.startsWith('image/') || isImagePath(file.name));
    const attachments = await Promise.all(imageFiles.map(fileToImageAttachment));
    attachments.forEach(addImageAttachment);
  }

  function addImageAttachment(attachment: ComposerImageAttachment) {
    if (imageAttachments.value.some((item) => item.id === attachment.id)) {
      return;
    }
    imageAttachments.value = [...imageAttachments.value, attachment];
  }

  function removeMentionQueryFromDraft() {
    if (!mentionQueryRange.value) {
      return;
    }

    const { start, end } = mentionQueryRange.value;
    draft.value = `${draft.value.slice(0, start)}${draft.value.slice(end)}`.replace(/\s{2,}/g, ' ').trimStart();
    composerRef.value?.focus();
    composerRef.value?.setSelectionRange(start, start);
    mentionQueryRange.value = null;
  }

  function insertAgentMention(name: string) {
    const mention = `@${name}`;
    if (!mentionQueryRange.value) {
      draft.value = appendToken(draft.value, mention);
      return;
    }

    const { start, end } = mentionQueryRange.value;
    const prefix = draft.value.slice(0, start);
    const suffix = draft.value.slice(end).replace(/^\s*/, '');
    draft.value = `${prefix}${mention} ${suffix}`.trimEnd();
    const cursor = prefix.length + mention.length + 1;
    composerRef.value?.focus();
    composerRef.value?.setSelectionRange(cursor, cursor);
    mentionQueryRange.value = null;
  }

  async function fileToImageAttachment(file: File): Promise<ComposerImageAttachment> {
    const previewUrl = await readFileAsDataUrl(file);
    return {
      id: `upload:${file.name}:${file.size}:${file.lastModified}`,
      name: file.name,
      previewUrl,
      mediaType: file.type || inferImageMediaType(file.name),
    };
  }

  function extractImageFiles(items?: DataTransferItemList | null) {
    if (!items) {
      return [];
    }
    return Array.from(items)
      .filter((item) => item.kind === 'file' && item.type.startsWith('image/'))
      .map((item) => item.getAsFile())
      .filter((file): file is File => !!file);
  }

  function hasImageFile(items?: DataTransferItemList | null) {
    if (!items) {
      return false;
    }
    return Array.from(items).some((item) => item.kind === 'file' && item.type.startsWith('image/'));
  }

  function isModifierOnlyKey(key: string) {
    return key === 'Shift' || key === 'Control' || key === 'Alt' || key === 'Meta';
  }

  return {
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
    closeAllMenus,
    handleDocumentPointerDown,
    syncComposerHeight,
    focusComposer,
    resetComposer,
  };
}

function isImagePath(path: string) {
  const normalized = path.trim().toLowerCase();
  const extension = normalized.split('.').pop();
  return !!extension && IMAGE_EXTENSIONS.has(extension);
}

function inferImageMediaType(name: string) {
  const extension = name.trim().toLowerCase().split('.').pop();
  switch (extension) {
    case 'png':
      return 'image/png';
    case 'jpg':
    case 'jpeg':
      return 'image/jpeg';
    case 'gif':
      return 'image/gif';
    case 'webp':
      return 'image/webp';
    case 'bmp':
      return 'image/bmp';
    case 'svg':
      return 'image/svg+xml';
    default:
      return 'image/png';
  }
}

function readFileAsDataUrl(file: File) {
  return new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === 'string') {
        resolve(reader.result);
        return;
      }
      reject(new Error(`failed to read image file ${file.name}`));
    };
    reader.onerror = () => reject(reader.error ?? new Error(`failed to read image file ${file.name}`));
    reader.readAsDataURL(file);
  });
}

function appendToken(content: string, token: string) {
  const trimmedEnd = content.replace(/\s+$/, '');
  return trimmedEnd ? `${trimmedEnd} ${token} ` : `${token} `;
}
