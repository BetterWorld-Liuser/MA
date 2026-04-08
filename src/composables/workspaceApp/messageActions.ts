import { nextTick, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { BackendWorkspaceSnapshot, ChatMessage, LiveTurn } from '@/data/mock';
import {
  augmentComposerMessage,
  extractBase64Payload,
  humanizeError,
  type ComposerPayload,
  type RunWorkspaceAction,
  type WorkspaceSnapshotState,
} from './types';

type ChatPaneHandle = { focusComposer: () => void };

type MessageActionsOptions = {
  workspaceState: WorkspaceSnapshotState;
  liveTurns: Ref<Record<number, LiveTurn>>;
  sendingTaskId: Ref<number | null>;
  cancellingTaskId: Ref<number | null>;
  errorMessage: Ref<string>;
  chatPaneRef: Ref<ChatPaneHandle | null>;
  clearTaskActivity: (taskId: number) => void;
  upsertLiveTurn: (taskId: number, turn: LiveTurn) => void;
  archiveFailedTurn: (taskId: number, turn: LiveTurn) => void;
  clearLiveTurn: (taskId: number) => void;
  clearArchivedFailedTurns: (taskId: number) => void;
  clearArchivedIntermediateTurns: (taskId: number) => void;
  openConfirmDialog: (options: {
    title: string;
    description: string;
    body: string;
    confirmLabel: string;
    action: () => Promise<void>;
  }) => void;
  closeConfirmDialog: () => void;
  runWorkspaceAction: RunWorkspaceAction;
};

export function createMessageActions({
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
}: MessageActionsOptions) {
  const {
    snapshot,
    workspace,
    optimisticTaskId,
    optimisticActiveTaskId,
    optimisticDeletedTaskIds,
    activeTaskIdNumber,
    queueLocalComposerMessage,
    clearLocalComposerMessages,
    clearTaskComposerState,
  } = workspaceState;

  async function refreshWorkspace(activeTaskId?: number | null) {
    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('load_workspace_snapshot', {
        activeTaskId: activeTaskId ?? undefined,
      });
    });
  }

  async function createTask(busy: Ref<boolean>) {
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

  async function selectTask(taskId: string, busy: Ref<boolean>) {
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

  async function deleteTask(taskId: string, busy: Ref<boolean>) {
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

  return {
    refreshWorkspace,
    createTask,
    selectTask,
    deleteTask,
    sendMessage,
    cancelCurrentTurn,
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
