import { computed, ref, watch, type Ref } from 'vue';
import { toChatMessages, type BackendWorkspaceSnapshot, type ChatMessage, type LiveTurn } from '@/data/mock';
import { debugChat } from '@/lib/chatDebug';
import type { WorkspaceChatView } from './types';

type UseTaskChatStateOptions = {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  activeTaskIdNumber: Readonly<Ref<number | null>>;
  liveTurns: Readonly<{ value: Record<number, LiveTurn> }>;
  archivedFailedTurns: Readonly<{ value: Record<number, Array<{ message: ChatMessage }>> }>;
  archivedIntermediateTurns: Readonly<{ value: Record<number, Array<{ message: ChatMessage }>> }>;
};

export function useTaskChatState({
  snapshot,
  activeTaskIdNumber,
  liveTurns,
  archivedFailedTurns,
  archivedIntermediateTurns,
}: UseTaskChatStateOptions) {
  const taskChats = ref<Record<number, ChatMessage[]>>({});
  const hydratedTaskIds = ref<Set<number>>(new Set());

  watch(
    () => snapshot.value?.active_task,
    (activeTask) => {
      if (!activeTask) {
        debugChat('task-chat', 'active-task:empty');
        return;
      }

      const taskId = activeTask.task.id;
      if (hydratedTaskIds.value.has(taskId)) {
        debugChat('task-chat', 'hydrate:skip-already-hydrated', {
          taskId,
          historyLength: activeTask.history.length,
        });
        return;
      }

      debugChat('task-chat', 'hydrate:from-snapshot', {
        taskId,
        historyLength: activeTask.history.length,
      });
      hydrateTaskChat(taskId, toChatMessages(activeTask.history));
      hydratedTaskIds.value = new Set([...hydratedTaskIds.value, taskId]);
    },
    { immediate: true },
  );

  const chatView = computed<WorkspaceChatView>(() => ({
    chat: mergeArchivedChatMessages(
      activeTaskIdNumber.value,
      taskChats.value,
      archivedIntermediateTurns.value,
      archivedFailedTurns.value,
    ),
    liveTurn: activeTaskIdNumber.value ? liveTurns.value[activeTaskIdNumber.value] : undefined,
  }));

  function hydrateTaskChat(taskId: number, messages: ChatMessage[]) {
    debugChat('task-chat', 'hydrate:apply', {
      taskId,
      messageCount: messages.length,
    });
    taskChats.value = {
      ...taskChats.value,
      [taskId]: messages.map(cloneChatMessage),
    };
  }

  function appendTaskChatMessage(taskId: number, message: ChatMessage) {
    const current = taskChats.value[taskId] ?? [];
    if (message.id && current.some((entry) => entry.id === message.id)) {
      debugChat('task-chat', 'append:skip-duplicate-id', {
        taskId,
        messageId: message.id,
      });
      return;
    }

    const last = current[current.length - 1];
    const sameAsLast =
      !!last
      && last.role === message.role
      && last.content === message.content
      && last.timestamp === message.timestamp;

    if (sameAsLast) {
      debugChat('task-chat', 'append:skip-same-as-last', {
        taskId,
        role: message.role,
        timestamp: message.timestamp ?? null,
      });
      return;
    }

    debugChat('task-chat', 'append:message', {
      taskId,
      role: message.role,
      messageId: message.id ?? null,
      nextCount: current.length + 1,
      contentLength: message.content.length,
    });
    taskChats.value = {
      ...taskChats.value,
      [taskId]: [...current, cloneChatMessage(message)],
    };
  }

  function clearTaskChat(taskId: number) {
    if (!(taskId in taskChats.value)) {
      debugChat('task-chat', 'clear:skip-missing', {
        taskId,
      });
      return;
    }

    debugChat('task-chat', 'clear', {
      taskId,
      previousCount: taskChats.value[taskId]?.length ?? 0,
    });
    const nextTaskChats = { ...taskChats.value };
    delete nextTaskChats[taskId];
    taskChats.value = nextTaskChats;

    if (!hydratedTaskIds.value.has(taskId)) {
      return;
    }

    const nextHydrated = new Set(hydratedTaskIds.value);
    nextHydrated.delete(taskId);
    hydratedTaskIds.value = nextHydrated;
  }

  function markTaskChatNeedsHydration(taskId: number) {
    if (!hydratedTaskIds.value.has(taskId)) {
      debugChat('task-chat', 'mark-needs-hydration:skip-not-hydrated', {
        taskId,
      });
      return;
    }

    debugChat('task-chat', 'mark-needs-hydration', {
      taskId,
    });
    const nextHydrated = new Set(hydratedTaskIds.value);
    nextHydrated.delete(taskId);
    hydratedTaskIds.value = nextHydrated;
  }

  return {
    chatView,
    appendTaskChatMessage,
    hydrateTaskChat,
    clearTaskChat,
    markTaskChatNeedsHydration,
  };
}

function cloneChatMessage(message: ChatMessage): ChatMessage {
  return {
    ...message,
    images: message.images ? [...message.images] : undefined,
    tools: message.tools ? [...message.tools] : undefined,
  };
}

function mergeArchivedChatMessages(
  taskId: number | null,
  taskChats: Record<number, ChatMessage[]>,
  archivedIntermediateTurns: Record<number, Array<{ message: ChatMessage }>>,
  archivedFailedTurns: Record<number, Array<{ message: ChatMessage }>>,
) {
  if (!taskId) {
    return [];
  }

  return [
    ...(taskChats[taskId] ?? []),
    ...(archivedIntermediateTurns[taskId] ?? []).map((entry) => entry.message),
    ...(archivedFailedTurns[taskId] ?? []).map((entry) => entry.message),
  ].sort(
    (left, right) => (left.timestamp ?? Number.MAX_SAFE_INTEGER) - (right.timestamp ?? Number.MAX_SAFE_INTEGER),
  );
}
