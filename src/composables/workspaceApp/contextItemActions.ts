import { nextTick, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { BackendMemoryDetailView, BackendWorkspaceSnapshot } from '@/data/mock';
import type { RunWorkspaceAction, WorkspaceSnapshotState } from './types';

type DialogHandle = {
  focusIdField: () => void;
  focusContentField: () => void;
};

type SaveMemoryPayload = {
  id: string;
  memoryType: string;
  topic: string;
  title: string;
  content: string;
  tags: string[];
  scope?: string;
  level?: string;
};

type ContextItemActionsOptions = {
  workspaceState: WorkspaceSnapshotState;
  busy: Ref<boolean>;
  noteDialogRef: Ref<DialogHandle | null>;
  memoryDialogRef: Ref<DialogHandle | null>;
  openCreateNoteDialog: () => void;
  openEditNoteDialog: (input: { id: string; content: string }) => void;
  openCreateMemoryDialog: () => void;
  openEditMemoryDialog: (memory: BackendMemoryDetailView) => void;
  submitNoteDialog: (
    onSubmit: (id: string, content: string) => Promise<void>,
    focus: { id: () => void; content: () => void },
  ) => Promise<void>;
  submitMemoryDialog: (
    onSubmit: (payload: SaveMemoryPayload) => Promise<void>,
    focus: { id: () => void; content: () => void },
  ) => Promise<void>;
  openConfirmDialog: (options: {
    title: string;
    description: string;
    body: string;
    confirmLabel: string;
    action: () => Promise<void>;
  }) => void;
  closeConfirmDialog: () => void;
  onMemoryMutated?: () => Promise<void>;
  runWorkspaceAction: RunWorkspaceAction;
};

export function createContextItemActions({
  workspaceState,
  busy,
  noteDialogRef,
  memoryDialogRef,
  openCreateNoteDialog,
  openEditNoteDialog,
  openCreateMemoryDialog,
  openEditMemoryDialog,
  submitNoteDialog,
  submitMemoryDialog,
  openConfirmDialog,
  closeConfirmDialog,
  onMemoryMutated,
  runWorkspaceAction,
}: ContextItemActionsOptions) {
  const { snapshot, workspace, activeTaskIdNumber } = workspaceState;

  async function addNote() {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }
    openCreateNoteDialog();
    await nextTick();
    noteDialogRef.value?.focusIdField();
  }

  async function addMemory() {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }
    openCreateMemoryDialog();
    await nextTick();
    memoryDialogRef.value?.focusIdField();
  }

  async function editNote(noteId: string) {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    const existing = workspace.value.notes.find((note) => note.id === noteId);
    openEditNoteDialog({
      id: noteId,
      content: existing?.content ?? '',
    });
    await nextTick();
    noteDialogRef.value?.focusContentField();
  }

  async function editMemory(memoryId: string) {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }
    const memory = await invoke<BackendMemoryDetailView>('get_memory', {
      input: {
        taskId: activeTaskIdNumber.value,
        id: memoryId,
      },
    });
    openEditMemoryDialog(memory);
    await nextTick();
    memoryDialogRef.value?.focusContentField();
  }

  async function handleSubmitNoteDialog() {
    await submitNoteDialog(saveNote, {
      id: () => {
        noteDialogRef.value?.focusIdField();
      },
      content: () => {
        noteDialogRef.value?.focusContentField();
      },
    });
  }

  async function handleSubmitMemoryDialog() {
    await submitMemoryDialog(saveMemory, {
      id: () => {
        memoryDialogRef.value?.focusIdField();
      },
      content: () => {
        memoryDialogRef.value?.focusContentField();
      },
    });
  }

  async function deleteNote(noteId: string) {
    if (!activeTaskIdNumber.value) {
      return;
    }

    openConfirmDialog({
      title: '删除 Note',
      description: '删除后，这条上下文不会再注入到下一轮 AI 视图中。',
      body: `确认删除 Note「${noteId}」吗？`,
      confirmLabel: '删除 Note',
      action: async () => {
        await runWorkspaceAction(async () => {
          snapshot.value = await invoke<BackendWorkspaceSnapshot>('delete_note', {
            input: {
              taskId: activeTaskIdNumber.value,
              noteId,
            },
          });
        });
        closeConfirmDialog();
      },
    });
  }

  async function deleteMemory(memoryId: string) {
    if (!activeTaskIdNumber.value) {
      return;
    }

    openConfirmDialog({
      title: '删除 Memory',
      description: '删除后，这条长期记忆不会再参与召回。',
      body: `确认删除 Memory「${memoryId}」吗？`,
      confirmLabel: '删除 Memory',
      action: async () => {
        await runWorkspaceAction(async () => {
          snapshot.value = await invoke<BackendWorkspaceSnapshot>('delete_memory', {
            input: {
              taskId: activeTaskIdNumber.value,
              id: memoryId,
            },
          });
        });
        await onMemoryMutated?.();
        closeConfirmDialog();
      },
    });
  }

  async function toggleOpenFileLock(path: string, locked: boolean) {
    if (!activeTaskIdNumber.value) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('toggle_open_file_lock', {
        input: {
          taskId: activeTaskIdNumber.value,
          path,
          locked,
        },
      });
    });
  }

  async function closeOpenFile(path: string) {
    if (!activeTaskIdNumber.value) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('close_open_file', {
        input: {
          taskId: activeTaskIdNumber.value,
          path,
        },
      });
    });
  }

  async function openFilesFromComposer(paths: string[]) {
    if (!activeTaskIdNumber.value || !paths.length) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('open_files', {
        input: {
          taskId: activeTaskIdNumber.value,
          paths,
        },
      });
    });
  }

  async function saveNote(noteId: string, content: string) {
    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('upsert_note', {
        input: {
          taskId: activeTaskIdNumber.value,
          noteId,
          content,
        },
      });
    });
  }

  async function saveMemory(payload: SaveMemoryPayload) {
    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('upsert_memory', {
        input: {
          taskId: activeTaskIdNumber.value,
          id: payload.id,
          memoryType: payload.memoryType,
          topic: payload.topic,
          title: payload.title,
          content: payload.content,
          tags: payload.tags,
          scope: payload.scope ?? null,
          level: payload.level ?? null,
        },
      });
    });
    await onMemoryMutated?.();
  }

  return {
    addNote,
    addMemory,
    editNote,
    editMemory,
    handleSubmitNoteDialog,
    handleSubmitMemoryDialog,
    deleteNote,
    deleteMemory,
    toggleOpenFileLock,
    closeOpenFile,
    openFilesFromComposer,
  };
}
