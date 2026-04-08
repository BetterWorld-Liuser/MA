import type { Ref } from 'vue';
import type { BackendMemoryDetailView, LiveTurn } from '@/data/mock';
import type { ComposerPayload, WorkspaceSnapshotState } from './types';
import { createContextItemActions } from './contextItemActions';
import { createMessageActions } from './messageActions';
import { createTaskSettingsActions } from './taskSettingsActions';
import { createWorkspaceActionRunner } from './workspaceActionRunner';

type UseWorkspaceTaskActionsOptions = {
  workspaceState: WorkspaceSnapshotState;
  liveTurns: Ref<Record<number, LiveTurn>>;
  sendingTaskId: Ref<number | null>;
  cancellingTaskId: Ref<number | null>;
  busy: Ref<boolean>;
  errorMessage: Ref<string>;
  chatPaneRef: Ref<{ focusComposer: () => void } | null>;
  clearTaskActivity: (taskId: number) => void;
  upsertLiveTurn: (taskId: number, turn: LiveTurn) => void;
  archiveFailedTurn: (taskId: number, turn: LiveTurn) => void;
  clearLiveTurn: (taskId: number) => void;
  clearArchivedFailedTurns: (taskId: number) => void;
  clearArchivedIntermediateTurns: (taskId: number) => void;
  openCreateNoteDialog: () => void;
  openEditNoteDialog: (input: { id: string; content: string }) => void;
  openConfirmDialog: (options: {
    title: string;
    description: string;
    body: string;
    confirmLabel: string;
    action: () => Promise<void>;
  }) => void;
  closeConfirmDialog: () => void;
  noteDialogRef: Ref<{
    focusIdField: () => void;
    focusContentField: () => void;
  } | null>;
  memoryDialogRef: Ref<{
    focusIdField: () => void;
    focusContentField: () => void;
  } | null>;
  submitNoteDialog: (
    onSubmit: (id: string, content: string) => Promise<void>,
    focus: { id: () => void; content: () => void },
  ) => Promise<void>;
  openCreateMemoryDialog: () => void;
  openEditMemoryDialog: (memory: BackendMemoryDetailView) => void;
  submitMemoryDialog: (
    onSubmit: (payload: {
      id: string;
      memoryType: string;
      topic: string;
      title: string;
      content: string;
      tags: string[];
      scope?: string;
      level?: string;
    }) => Promise<void>,
    focus: { id: () => void; content: () => void },
  ) => Promise<void>;
  onMemoryMutated?: () => Promise<void>;
};

export function useWorkspaceTaskActions({
  workspaceState,
  liveTurns,
  sendingTaskId,
  cancellingTaskId,
  busy,
  errorMessage,
  chatPaneRef,
  clearTaskActivity,
  upsertLiveTurn,
  archiveFailedTurn,
  clearLiveTurn,
  clearArchivedFailedTurns,
  clearArchivedIntermediateTurns,
  openCreateNoteDialog,
  openEditNoteDialog,
  openConfirmDialog,
  closeConfirmDialog,
  noteDialogRef,
  memoryDialogRef,
  submitNoteDialog,
  openCreateMemoryDialog,
  openEditMemoryDialog,
  submitMemoryDialog,
  onMemoryMutated,
}: UseWorkspaceTaskActionsOptions) {
  const runWorkspaceAction = createWorkspaceActionRunner({
    busy,
    errorMessage,
    snapshot: workspaceState.snapshot,
    optimisticTaskId: workspaceState.optimisticTaskId,
  });

  const messageActions = createMessageActions({
    workspaceState,
    liveTurns,
    sendingTaskId,
    cancellingTaskId,
    errorMessage,
    chatPaneRef,
    clearTaskActivity,
    upsertLiveTurn,
    archiveFailedTurn,
    clearLiveTurn,
    clearArchivedFailedTurns,
    clearArchivedIntermediateTurns,
    openConfirmDialog,
    closeConfirmDialog,
    runWorkspaceAction,
  });

  const contextItemActions = createContextItemActions({
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
  });

  const taskSettingsActions = createTaskSettingsActions({
    workspaceState,
    runWorkspaceAction,
  });

  async function createTask() {
    await messageActions.createTask(busy);
  }

  async function selectTask(taskId: string) {
    await messageActions.selectTask(taskId, busy);
  }

  async function deleteTask(taskId: string) {
    await messageActions.deleteTask(taskId, busy);
  }

  async function setTaskModel(selection: { modelConfigId: number }) {
    if (busy.value) {
      return;
    }
    await taskSettingsActions.setTaskModel(selection);
  }

  async function setTaskModelSettings(settings: {
    temperature?: number | null;
    topP?: number | null;
    presencePenalty?: number | null;
    frequencyPenalty?: number | null;
    maxOutputTokens?: number | null;
  }) {
    await taskSettingsActions.setTaskModelSettings(settings, busy);
  }

  async function setTaskWorkingDirectory(path?: string | null) {
    await taskSettingsActions.setTaskWorkingDirectory(path, busy);
  }

  async function refreshSkills() {
    if (!workspaceState.activeTaskIdNumber.value || busy.value) {
      return;
    }

    await messageActions.refreshWorkspace(workspaceState.activeTaskIdNumber.value);
  }

  return {
    workspacePath: workspaceState.workspacePath,
    runWorkspaceAction,
    refreshWorkspace: messageActions.refreshWorkspace,
    createTask,
    selectTask,
    deleteTask,
    sendMessage: (payload: ComposerPayload) => messageActions.sendMessage(payload),
    cancelCurrentTurn: () => messageActions.cancelCurrentTurn(),
    addNote: () => contextItemActions.addNote(),
    editNote: (noteId: string) => contextItemActions.editNote(noteId),
    addMemory: () => contextItemActions.addMemory(),
    editMemory: (memoryId: string) => contextItemActions.editMemory(memoryId),
    handleSubmitNoteDialog: () => contextItemActions.handleSubmitNoteDialog(),
    handleSubmitMemoryDialog: () => contextItemActions.handleSubmitMemoryDialog(),
    deleteNote: (noteId: string) => contextItemActions.deleteNote(noteId),
    deleteMemory: (memoryId: string) => contextItemActions.deleteMemory(memoryId),
    toggleOpenFileLock: (path: string, locked: boolean) => contextItemActions.toggleOpenFileLock(path, locked),
    closeOpenFile: (path: string) => contextItemActions.closeOpenFile(path),
    openFilesFromComposer: (paths: string[]) => contextItemActions.openFilesFromComposer(paths),
    setTaskModel,
    setTaskModelSettings,
    setTaskWorkingDirectory,
    refreshSkills,
  };
}
