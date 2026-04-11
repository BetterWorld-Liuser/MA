import { nextTick, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { BackendWorkspaceSnapshot } from '@/data/mock';
import { toDebugRoundItem } from '@/data/mock';
import { debugChat, summarizeSnapshot } from '@/lib/chatDebug';
import {
  augmentComposerMessage,
  extractBase64Payload,
  humanizeError,
  type ComposerPayload,
  type RunWorkspaceAction,
  type TaskChatState,
  type WorkspaceSnapshotState,
} from './types';

type ChatPaneHandle = { focusComposer: () => void };

type MessageActionsOptions = {
  workspaceState: WorkspaceSnapshotState;
  taskChatState: TaskChatState;
  sendingTaskId: Ref<number | null>;
  cancellingTaskId: Ref<number | null>;
  errorMessage: Ref<string>;
  chatPaneRef: Ref<ChatPaneHandle | null>;
  clearTaskActivity: (taskId: number) => void;
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
  taskChatState,
  sendingTaskId,
  cancellingTaskId,
  errorMessage,
  chatPaneRef,
  clearTaskActivity,
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
    hydrateTaskDebugTrace,
    clearDeletedTaskOptimism,
    syncTaskContextSnapshot,
  } = workspaceState;
  const { optimisticAppendUserMessage, clearTaskTimeline } = taskChatState;

  function applyCompletedTaskSnapshot(nextSnapshot: BackendWorkspaceSnapshot, taskId: number) {
    debugChat('message-actions', 'apply-completed-snapshot:start', {
      taskId,
      next: summarizeSnapshot(nextSnapshot),
      current: summarizeSnapshot(snapshot.value),
    });
    const currentSnapshot = snapshot.value;
    const currentActiveTask = currentSnapshot?.active_task;
    const nextActiveTask = nextSnapshot.active_task;

    if (
      !currentSnapshot
      || !currentActiveTask
      || !nextActiveTask
      || currentActiveTask.task.id !== taskId
      || nextActiveTask.task.id !== taskId
    ) {
      if (currentSnapshot) {
        snapshot.value = {
          ...currentSnapshot,
          tasks: nextSnapshot.tasks,
        };
        debugChat('message-actions', 'apply-completed-snapshot:tasks-only', summarizeSnapshot(snapshot.value));
      }
      return;
    }

    if (nextActiveTask.debug_trace) {
      hydrateTaskDebugTrace(taskId, nextActiveTask.debug_trace.rounds.map(toDebugRoundItem));
    }
    syncTaskContextSnapshot(taskId, nextActiveTask);

    snapshot.value = {
      ...currentSnapshot,
      tasks: nextSnapshot.tasks,
      active_task: {
        ...currentActiveTask,
        ...nextActiveTask,
        runtime: nextActiveTask.runtime ?? currentActiveTask.runtime,
        debug_trace: nextActiveTask.debug_trace ?? currentActiveTask.debug_trace,
        timeline: mergeCompletedTaskTimeline(currentActiveTask.timeline, nextActiveTask.timeline),
      },
    };
    debugChat('message-actions', 'apply-completed-snapshot:merged', summarizeSnapshot(snapshot.value));
  }

  function finalizeSuccessfulSend(taskId: number, nextSnapshot: BackendWorkspaceSnapshot) {
    applyCompletedTaskSnapshot(nextSnapshot, taskId);
    // Let terminal progress events own the live-turn lifecycle so a fast invoke
    // response does not clear the bubble before the final assistant event lands.
    debugChat('message-actions', 'send:finalize-success', {
      taskId,
      snapshot: summarizeSnapshot(snapshot.value),
    });
    errorMessage.value = '';
  }

  async function syncReferencedFiles(taskId: number, payload: ComposerPayload) {
    const openFilePaths = Array.from(new Set([...payload.files, ...payload.skills]));
    if (!openFilePaths.length) {
      debugChat('message-actions', 'sync-referenced-files:skip-empty', {
        taskId,
      });
      return;
    }

    debugChat('message-actions', 'sync-referenced-files:start', {
      taskId,
      paths: openFilePaths,
    });
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('open_files', {
      input: {
        taskId,
        paths: openFilePaths,
      },
    });
    debugChat('message-actions', 'sync-referenced-files:done', summarizeSnapshot(snapshot.value));
  }

  function beginPendingTurn(taskId: number, payload: ComposerPayload, content: string) {
    debugChat('message-actions', 'send:begin-pending-turn', {
      taskId,
      contentLength: content.length,
      directories: payload.directories.length,
      files: payload.files.length,
      skills: payload.skills.length,
      images: payload.images.length,
    });
    optimisticAppendUserMessage(taskId, buildPendingUserMessage(content, payload));
    sendingTaskId.value = taskId;
  }

  function finalizeFailedSend(taskId: number, error: unknown) {
    debugChat('message-actions', 'send:finalize-failed', {
      taskId,
      error: humanizeError(error),
    });
    errorMessage.value = humanizeError(error);
  }

  async function requestAssistantReply(taskId: number, payload: ComposerPayload, content: string) {
    const requestId = `task-${taskId}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
    debugChat('message-actions', 'send:invoke-start', {
      taskId,
      requestId,
      contentLength: content.length,
      images: payload.images.length,
    });
    return invoke<BackendWorkspaceSnapshot>('send_message', {
      input: {
        taskId,
        requestId,
        mentions: payload.mentions,
        replies: payload.replies,
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
  }

  async function refreshWorkspace(activeTaskId?: number | null) {
    debugChat('message-actions', 'refresh-workspace:start', {
      activeTaskId: activeTaskId ?? null,
    });
    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('load_workspace_snapshot', {
        activeTaskId: activeTaskId ?? undefined,
      });
    });
    debugChat('message-actions', 'refresh-workspace:done', summarizeSnapshot(snapshot.value));
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
        clearTaskActivity(numericTaskId);
        clearTaskTimeline(numericTaskId);
        clearDeletedTaskOptimism(numericTaskId);
        if (sendingTaskId.value === numericTaskId) {
          sendingTaskId.value = null;
        }
        closeConfirmDialog();
      },
    });
  }

  async function sendMessage(payload: ComposerPayload) {
    if (!activeTaskIdNumber.value || sendingTaskId.value !== null) {
      debugChat('message-actions', 'send:skip', {
        activeTaskId: activeTaskIdNumber.value,
        sendingTaskId: sendingTaskId.value,
      });
      return;
    }

    const taskId = activeTaskIdNumber.value;
    const content = augmentComposerMessage(payload);
    beginPendingTurn(taskId, payload, content);

    try {
      await syncReferencedFiles(taskId, payload);
      const nextSnapshot = await requestAssistantReply(taskId, payload, content);
      debugChat('message-actions', 'send:invoke-done', {
        taskId,
        nextSnapshot: summarizeSnapshot(nextSnapshot),
      });
      finalizeSuccessfulSend(taskId, nextSnapshot);
    } catch (error) {
      finalizeFailedSend(taskId, error);
    } finally {
      debugChat('message-actions', 'send:finally', {
        taskId,
        sendingTaskId: sendingTaskId.value,
        cancellingTaskId: cancellingTaskId.value,
      });
      if (sendingTaskId.value === taskId) {
        sendingTaskId.value = null;
      }
      if (cancellingTaskId.value === taskId) {
        cancellingTaskId.value = null;
      }
    }
  }

  async function cancelCurrentTurn(turnId?: string) {
    if (!sendingTaskId.value || cancellingTaskId.value === sendingTaskId.value) {
      return;
    }

    const taskId = sendingTaskId.value;
    cancellingTaskId.value = taskId;

    try {
      if (turnId) {
        await invoke('cancel_turn', { turnId });
      } else {
        await invoke('cancel_task', { taskId });
      }
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

function buildPendingUserMessage(content: string, payload: ComposerPayload) {
  return {
    id: `pending-user:${Date.now()}`,
    ts: Date.now(),
    content,
    mentions: payload.mentions,
    replies: payload.replies,
    images: payload.images,
  };
}

function mergeCompletedTaskTimeline(
  currentTimeline: NonNullable<BackendWorkspaceSnapshot['active_task']>['timeline'],
  nextTimeline: NonNullable<BackendWorkspaceSnapshot['active_task']>['timeline'],
) {
  return nextTimeline.length >= currentTimeline.length ? nextTimeline : currentTimeline;
}
