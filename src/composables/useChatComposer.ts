import { computed, ref, type Ref } from 'vue';
import type { ComposerImageAttachment, MentionItem } from './chatComposerShared';
import { useComposerImageAttachments } from './useComposerImageAttachments';
import { useComposerMentions } from './useComposerMentions';

export type { ComposerImageAttachment, MentionItem } from './chatComposerShared';

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
  const composerRef = ref<HTMLTextAreaElement | null>(null);
  const composerRootRef = ref<HTMLElement | null>(null);
  const imageInputRef = ref<HTMLInputElement | null>(null);
  const plusMenuOpen = ref(false);
  const composerNotice = ref('');
  const composerMaxHeight = 160;
  let composerNoticeTimer: ReturnType<typeof setTimeout> | null = null;

  const {
    imageAttachments,
    dragActive,
    removeImageAttachment,
    clearImageAttachments,
    attachWorkspaceImage,
    handleImageFileSelection,
    handlePaste,
    handleDrop,
    handleDragOver,
    handleDragLeave: handleImageDragLeave,
  } = useComposerImageAttachments({
    taskId,
    supportsVision,
    showComposerNotice,
  });

  const {
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
  } = useComposerMentions({
    disabled,
    taskId,
    draft,
    composerRef,
    plusMenuOpen,
    attachWorkspaceImage,
    showComposerNotice,
  });

  const composerIsEmpty = computed(() =>
    !draft.value.trim() && mentions.value.length === 0 && imageAttachments.value.length === 0,
  );

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

    if (event.key === 'Enter' && !event.shiftKey && !sending.value) {
      event.preventDefault();
      submit();
    }
  }

  function togglePlusMenu() {
    plusMenuOpen.value = !plusMenuOpen.value;
    if (plusMenuOpen.value) {
      closeSearchPanel();
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

  function handleDragLeave(event: DragEvent) {
    handleImageDragLeave(event, composerRootRef);
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
    clearMentions();
    clearImageAttachments();
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

function isModifierOnlyKey(key: string) {
  return key === 'Shift' || key === 'Control' || key === 'Alt' || key === 'Meta';
}
