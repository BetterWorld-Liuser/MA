import { ref } from 'vue';

type NoteSeed = {
  id: string;
  content: string;
};

export function useNoteDialog() {
  const noteDialogOpen = ref(false);
  const noteDialogMode = ref<'create' | 'edit'>('create');
  const noteDraftId = ref('');
  const noteDraftContent = ref('');

  function openCreateNoteDialog(defaultId = 'target') {
    noteDialogMode.value = 'create';
    noteDraftId.value = defaultId;
    noteDraftContent.value = '';
    noteDialogOpen.value = true;
  }

  function openEditNoteDialog(note: NoteSeed) {
    noteDialogMode.value = 'edit';
    noteDraftId.value = note.id;
    noteDraftContent.value = note.content;
    noteDialogOpen.value = true;
  }

  function closeNoteDialog() {
    noteDialogOpen.value = false;
    noteDialogMode.value = 'create';
    noteDraftId.value = '';
    noteDraftContent.value = '';
  }

  function handleNoteDialogOpenChange(open: boolean) {
    if (!open) {
      closeNoteDialog();
    }
  }

  async function submitNoteDialog(
    onSave: (noteId: string, content: string) => Promise<void>,
    focusField?: { id: () => void; content: () => void },
  ) {
    const noteId = noteDraftId.value.trim();
    const content = noteDraftContent.value.trim();

    if (!noteId) {
      focusField?.id();
      return;
    }
    if (!content) {
      focusField?.content();
      return;
    }

    await onSave(noteId, content);
    closeNoteDialog();
  }

  return {
    noteDialogOpen,
    noteDialogMode,
    noteDraftId,
    noteDraftContent,
    openCreateNoteDialog,
    openEditNoteDialog,
    closeNoteDialog,
    handleNoteDialogOpenChange,
    submitNoteDialog,
  };
}
