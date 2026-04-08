import { nextTick, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type {
  BackendWorkspaceSnapshot,
  BackendMemoryDetailView,
  ChatMessage,
  LiveTurn,
} from '@/data/mock';
import {
  augmentComposerMessage,
  extractBase64Payload,
  humanizeError,
  type ComposerPayload,
  type RunWorkspaceAction,
  type WorkspaceSnapshotState,
} from './types';

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
  const {
    snapshot,
    workspace,
    workspacePath,
    optimisticTaskId,
    optimisticActiveTaskId,
    optimisticDeletedTaskIds,
    activeTaskIdNumber,
    queueLocalComposerMessage,
    clearLocalComposerMessages,
    clearTaskComposerState,
  } = workspaceState;
  const runWorkspaceAction = createWorkspaceActionRunner({
    busy,
    errorMessage,
    snapshot,
    optimisticTaskId,
  });

  async function refreshWorkspace(activeTaskId?: number | null) {
    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('load_workspace_snapshot', {
        activeTaskId: activeTaskId ?? undefined,
      });
    });
  }

  async function createTask() {
    if (busy.value) {
      return;
    }

    optimisticTaskId.value = `pending-task-${Date.now()}`;
    await nextTick();

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('create_task', {
        input: {},
      });
    });
    optimisticTaskId.value = null;
    await nextTick();
    chatPaneRef.value?.focusComposer();
  }

  async function selectTask(taskId: string) {
    if (!taskId || busy.value) {
      return;
    }

    const numericTaskId = Number(taskId);
    if (Number.isFinite(numericTaskId)) {
      clearTaskActivity(numericTaskId);
    }

    optimisticActiveTaskId.value = taskId;

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('select_task', {
        input: { taskId: numericTaskId },
      });
    });

    optimisticActiveTaskId.value = null;
  }

  async function deleteTask(taskId: string) {
    if (!taskId || busy.value) {
      return;
    }

    const task = workspace.value.tasks.find((item) => item.id === taskId);
    openConfirmDialog({
      title: '删除任务',
      description: '删除后，这个主题窗口及其聊天记录会从当前工作区移除。',
      body: `确认删除「${task?.name ?? taskId}」吗？这个操作目前不能撤销。`,
      confirmLabel: '删除任务',
      action: async () => {
        optimisticDeletedTaskIds.value = new Set([...optimisticDeletedTaskIds.value, taskId]);
        const succeeded = await runWorkspaceAction(async () => {
          snapshot.value = await invoke<BackendWorkspaceSnapshot>('delete_task', {
            input: { taskId: Number(taskId) },
          });
        });
        optimisticDeletedTaskIds.value = new Set(
          [...optimisticDeletedTaskIds.value].filter((id) => id !== taskId),
        );
        if (!succeeded) {
          return;
        }
        const numericTaskId = Number(taskId);
        clearLiveTurn(numericTaskId);
        clearTaskActivity(numericTaskId);
        clearArchivedFailedTurns(numericTaskId);
        clearArchivedIntermediateTurns(numericTaskId);
        clearTaskComposerState(numericTaskId);
        if (sendingTaskId.value === numericTaskId) {
          sendingTaskId.value = null;
        }
        closeConfirmDialog();
      },
    });
  }

  async function sendMessage(payload: ComposerPayload) {
    if (!activeTaskIdNumber.value || sendingTaskId.value !== null) {
      return;
    }

    const taskId = activeTaskIdNumber.value;
    const content = augmentComposerMessage(payload);

    queueLocalComposerMessage(taskId, buildPendingUserMessage(content, payload));
    upsertLiveTurn(taskId, {
      turnId: `pending-${Date.now()}`,
      author: 'March',
      state: 'pending',
      statusLabel: '已发送，正在准备',
      content: '',
      errorMessage: '',
      tools: [],
    });
    sendingTaskId.value = taskId;

    try {
      const openFilePaths = Array.from(new Set([...payload.files, ...payload.skills]));
      if (openFilePaths.length) {
        snapshot.value = await invoke<BackendWorkspaceSnapshot>('open_files', {
          input: {
            taskId,
            paths: openFilePaths,
          },
        });
      }

      const nextSnapshot = await invoke<BackendWorkspaceSnapshot>('send_message', {
        input: {
          taskId,
          contentBlocks: [
            ...(content ? [{ type: 'text', text: content }] : []),
            ...payload.images.map((image) => ({
              type: 'image',
              media_type: image.mediaType,
              data_base64: extractBase64Payload(image.previewUrl),
              source_path: image.sourcePath ?? null,
              name: image.name,
            })),
          ],
        },
      });
      clearLocalComposerMessages(taskId);
      clearLiveTurn(taskId);
      if (snapshot.value?.active_task?.task.id === taskId) {
        snapshot.value = nextSnapshot;
      } else if (snapshot.value) {
        snapshot.value = {
          ...snapshot.value,
          tasks: nextSnapshot.tasks,
        };
      }
      errorMessage.value = '';
    } catch (error) {
      const currentLiveTurn = liveTurns.value[taskId];
      if (currentLiveTurn) {
        const failedTurn: LiveTurn = {
          ...currentLiveTurn,
          state: 'error',
          statusLabel: '本轮执行失败',
          errorMessage: humanizeError(error),
        };
        upsertLiveTurn(taskId, failedTurn);
        archiveFailedTurn(taskId, failedTurn);
        clearLiveTurn(taskId);
      }
      errorMessage.value = humanizeError(error);
    } finally {
      if (sendingTaskId.value === taskId) {
        sendingTaskId.value = null;
      }
      if (cancellingTaskId.value === taskId) {
        cancellingTaskId.value = null;
      }
    }
  }

  async function cancelCurrentTurn() {
    if (!sendingTaskId.value || cancellingTaskId.value === sendingTaskId.value) {
      return;
    }

    const taskId = sendingTaskId.value;
    cancellingTaskId.value = taskId;
    const currentLiveTurn = liveTurns.value[taskId];
    if (currentLiveTurn) {
      upsertLiveTurn(taskId, {
        ...currentLiveTurn,
        statusLabel: '正在中断…',
      });
    }

    try {
      await invoke('cancel_turn', { taskId });
    } catch (error) {
      cancellingTaskId.value = null;
      errorMessage.value = humanizeError(error);
    }
  }

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

  async function saveMemory(payload: {
    id: string;
    memoryType: string;
    topic: string;
    title: string;
    content: string;
    tags: string[];
    scope?: string;
    level?: string;
  }) {
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

  async function setTaskModel(selection: { modelConfigId: number }) {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('set_task_model', {
        input: {
          taskId: activeTaskIdNumber.value,
          modelConfigId: selection.modelConfigId,
        },
      });
    });
  }

  async function setTaskModelSettings(settings: {
    temperature?: number | null;
    topP?: number | null;
    presencePenalty?: number | null;
    frequencyPenalty?: number | null;
    maxOutputTokens?: number | null;
  }) {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('set_task_model_settings', {
        input: {
          taskId: activeTaskIdNumber.value,
          temperature: settings.temperature ?? null,
          topP: settings.topP ?? null,
          presencePenalty: settings.presencePenalty ?? null,
          frequencyPenalty: settings.frequencyPenalty ?? null,
          maxOutputTokens: settings.maxOutputTokens ?? null,
        },
      });
    });
  }

  async function setTaskWorkingDirectory(path?: string | null) {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('set_task_working_directory', {
        input: {
          taskId: activeTaskIdNumber.value,
          path: path ?? null,
        },
      });
    });
  }

  async function refreshSkills() {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    await refreshWorkspace(activeTaskIdNumber.value);
  }

  return {
    workspacePath,
    runWorkspaceAction,
    refreshWorkspace,
    createTask,
    selectTask,
    deleteTask,
    sendMessage,
    cancelCurrentTurn,
    addNote,
    editNote,
    addMemory,
    editMemory,
    handleSubmitNoteDialog,
    handleSubmitMemoryDialog,
    deleteNote,
    deleteMemory,
    toggleOpenFileLock,
    closeOpenFile,
    openFilesFromComposer,
    setTaskModel,
    setTaskModelSettings,
    setTaskWorkingDirectory,
    refreshSkills,
  };
}

function createWorkspaceActionRunner({
  busy,
  errorMessage,
  snapshot,
  optimisticTaskId,
}: {
  busy: Ref<boolean>;
  errorMessage: Ref<string>;
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  optimisticTaskId: Ref<string | null>;
}): RunWorkspaceAction {
  return async (action) => {
    busy.value = true;
    try {
      await action();
      errorMessage.value = '';
      return true;
    } catch (error) {
      optimisticTaskId.value = null;
      if (!snapshot.value) {
        console.warn('Failed to load workspace snapshot from Tauri backend, using mock data.', error);
      }
      errorMessage.value = humanizeError(error);
      return false;
    } finally {
      busy.value = false;
    }
  };
}

function buildPendingUserMessage(content: string, payload: ComposerPayload): ChatMessage {
  return {
    role: 'user',
    author: 'User',
    time: new Date().toLocaleTimeString([], {
      hour: '2-digit',
      minute: '2-digit',
    }),
    timestamp: Date.now(),
    content,
    images: payload.images,
  };
}
