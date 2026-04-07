import { computed, ref, type Ref } from 'vue';
import {
  mockWorkspace,
  toWorkspaceView,
  type BackendWorkspaceSnapshot,
  type ChatMessage,
  type LiveTurn,
  type TaskActivityStatus,
  type WorkspaceView,
} from '@/data/mock';
import type { WorkspaceSnapshotState } from './types';

type UseWorkspaceSnapshotStateOptions = {
  liveTurns: Readonly<{ value: Record<number, LiveTurn> }>;
  archivedFailedTurns: Readonly<{ value: Record<number, Array<{ message: ChatMessage }>> }>;
  archivedIntermediateTurns: Readonly<{ value: Record<number, Array<{ message: ChatMessage }>> }>;
  taskActivityStatuses: Readonly<{ value: Record<number, TaskActivityStatus> }>;
};

export function useWorkspaceSnapshotState({
  snapshot,
  liveTurns,
  archivedFailedTurns,
  archivedIntermediateTurns,
  taskActivityStatuses,
}: UseWorkspaceSnapshotStateOptions & {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
}): WorkspaceSnapshotState {
  const optimisticTaskId = ref<string | null>(null);
  const optimisticActiveTaskId = ref<string | null>(null);
  const optimisticDeletedTaskIds = ref<Set<string>>(new Set());
  const localComposerMessages = ref<Record<number, ChatMessage[]>>({});
  const workspacePath = computed(() => snapshot.value?.workspace_path);

  const workspace = computed<WorkspaceView>(() => {
    const activeTaskId = snapshot.value?.active_task?.task.id ?? (snapshot.value?.tasks[0]?.id ?? null);

    if (!snapshot.value) {
      return mockWorkspace;
    }

    const baseWorkspace = toWorkspaceView(snapshot.value);
    const intermediateMessages = activeTaskId
      ? (archivedIntermediateTurns.value[activeTaskId] ?? []).map((entry) => entry.message)
      : [];
    const archivedMessages = activeTaskId
      ? (archivedFailedTurns.value[activeTaskId] ?? []).map((entry) => entry.message)
      : [];
    const mergedChat = [...baseWorkspace.chat, ...intermediateMessages, ...archivedMessages].sort(
      (left, right) => (left.timestamp ?? Number.MAX_SAFE_INTEGER) - (right.timestamp ?? Number.MAX_SAFE_INTEGER),
    );

    return {
      ...baseWorkspace,
      tasks: baseWorkspace.tasks.map((task) => {
        const taskId = Number(task.id);
        const activityStatus = Number.isFinite(taskId) ? taskActivityStatuses.value[taskId] : undefined;
        return {
          ...task,
          activityStatus: task.id === String(activeTaskId) ? undefined : activityStatus,
        };
      }),
      chat: mergeChatWithComposerMessages(localComposerMessages.value, activeTaskId ?? undefined, mergedChat),
      liveTurn: activeTaskId ? liveTurns.value[activeTaskId] : undefined,
    };
  });

  const resolvedWorkspace = computed<WorkspaceView>(() => {
    const baseWorkspace = workspace.value;
    let nextWorkspace = baseWorkspace;

    if (optimisticTaskId.value) {
      const optimisticTask = {
        id: optimisticTaskId.value,
        name: '默认任务',
        status: 'active' as const,
        updatedAt: '刚刚',
      };

      nextWorkspace = buildEmptyTaskWorkspace(baseWorkspace, {
        title: optimisticTask.name,
        tasks: [
          optimisticTask,
          ...baseWorkspace.tasks
            .filter((task) => task.id !== optimisticTask.id)
            .map((task) => ({
              ...task,
              status: 'idle' as const,
            })),
        ],
        activeTaskId: optimisticTask.id,
        workingDirectory: baseWorkspace.workspacePath ?? baseWorkspace.workingDirectory,
      });
    }

    if (!optimisticDeletedTaskIds.value.size) {
      return applyOptimisticActiveTask(nextWorkspace, optimisticActiveTaskId.value);
    }

    const visibleTasks = nextWorkspace.tasks.filter((task) => !optimisticDeletedTaskIds.value.has(task.id));
    const activeTaskVisible =
      nextWorkspace.activeTaskId && !optimisticDeletedTaskIds.value.has(nextWorkspace.activeTaskId);
    const fallbackActiveTaskId = activeTaskVisible ? nextWorkspace.activeTaskId : (visibleTasks[0]?.id ?? '');

    if (activeTaskVisible) {
      return applyOptimisticActiveTask(
        {
          ...nextWorkspace,
          tasks: visibleTasks,
        },
        optimisticActiveTaskId.value,
      );
    }

    const fallbackTaskName = visibleTasks.find((task) => task.id === fallbackActiveTaskId)?.name ?? 'March';

    return applyOptimisticActiveTask(
      buildEmptyTaskWorkspace(nextWorkspace, {
        title: fallbackTaskName,
        tasks: visibleTasks,
        activeTaskId: fallbackActiveTaskId,
        workingDirectory: nextWorkspace.workspacePath ?? nextWorkspace.workingDirectory,
      }),
      optimisticActiveTaskId.value,
    );
  });

  const activeTaskIdNumber = computed(() => {
    const raw = resolvedWorkspace.value.activeTaskId;
    if (!raw || raw === optimisticTaskId.value) {
      return null;
    }
    const parsed = Number(raw);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
  });

  function queueLocalComposerMessage(taskId: number, message: ChatMessage) {
    localComposerMessages.value = {
      ...localComposerMessages.value,
      [taskId]: [...(localComposerMessages.value[taskId] ?? []), message],
    };
  }

  function clearLocalComposerMessages(taskId: number) {
    if (!(taskId in localComposerMessages.value)) {
      return;
    }

    const nextMessages = { ...localComposerMessages.value };
    delete nextMessages[taskId];
    localComposerMessages.value = nextMessages;
  }

  function clearTaskComposerState(taskId: number) {
    clearLocalComposerMessages(taskId);
    optimisticDeletedTaskIds.value = new Set(
      [...optimisticDeletedTaskIds.value].filter((id) => id !== String(taskId)),
    );
  }

  return {
    snapshot,
    workspacePath,
    workspace,
    resolvedWorkspace,
    optimisticTaskId,
    optimisticActiveTaskId,
    optimisticDeletedTaskIds,
    localComposerMessages,
    activeTaskIdNumber,
    queueLocalComposerMessage,
    clearLocalComposerMessages,
    clearTaskComposerState,
  };
}

function buildEmptyTaskWorkspace(
  workspace: WorkspaceView,
  overrides: {
    title: string;
    tasks: WorkspaceView['tasks'];
    activeTaskId: string;
    workingDirectory: string | undefined;
  },
): WorkspaceView {
  return {
    ...workspace,
    title: overrides.title,
    tasks: overrides.tasks,
    activeTaskId: overrides.activeTaskId,
    selectedModel: undefined,
    selectedTemperature: undefined,
    selectedTopP: undefined,
    selectedPresencePenalty: undefined,
    selectedFrequencyPenalty: undefined,
    selectedMaxOutputTokens: undefined,
    workingDirectory: overrides.workingDirectory,
    chat: [],
    notes: [],
    openFiles: [],
    hints: [],
    skills: [],
    contextUsage: {
      percent: 0,
      current: '0',
      limit: workspace.contextUsage.limit,
      sections: [],
    },
    debugRounds: [],
    liveTurn: undefined,
  };
}

function applyOptimisticActiveTask(workspace: WorkspaceView, optimisticTaskId: string | null): WorkspaceView {
  if (!optimisticTaskId || optimisticTaskId === workspace.activeTaskId) {
    return workspace;
  }
  const targetTask = workspace.tasks.find((task) => task.id === optimisticTaskId);
  if (!targetTask) {
    return workspace;
  }
  return {
    ...workspace,
    activeTaskId: optimisticTaskId,
    title: targetTask.name,
    selectedModel: undefined,
    selectedTemperature: undefined,
    selectedTopP: undefined,
    selectedPresencePenalty: undefined,
    selectedFrequencyPenalty: undefined,
    selectedMaxOutputTokens: undefined,
    chat: [],
    notes: [],
    openFiles: [],
    hints: [],
    skills: [],
    contextUsage: {
      percent: 0,
      current: '0',
      limit: workspace.contextUsage.limit,
      sections: [],
    },
    debugRounds: [],
    liveTurn: undefined,
  };
}

function mergeChatWithComposerMessages(
  localComposerMessages: Record<number, ChatMessage[]>,
  taskId: number | undefined,
  chat: ChatMessage[],
) {
  if (!taskId) {
    return chat;
  }

  const localMessages = localComposerMessages[taskId] ?? [];
  if (!localMessages.length) {
    return chat;
  }

  const merged = chat.map((message) => ({
    ...message,
    images: message.images ? [...message.images] : undefined,
  }));
  const usedIndices = new Set<number>();
  const unmatched: ChatMessage[] = [];

  for (const localMessage of localMessages) {
    const matchIndex = merged.findIndex((message, index) =>
      !usedIndices.has(index)
      && message.role === localMessage.role
      && message.content === localMessage.content
      && Math.abs((message.timestamp ?? 0) - (localMessage.timestamp ?? 0)) < 120000,
    );

    if (matchIndex >= 0) {
      usedIndices.add(matchIndex);
      if (localMessage.images?.length) {
        merged[matchIndex] = {
          ...merged[matchIndex],
          images: localMessage.images,
        };
      }
    } else {
      unmatched.push(localMessage);
    }
  }

  return [...merged, ...unmatched].sort(
    (left, right) => (left.timestamp ?? Number.MAX_SAFE_INTEGER) - (right.timestamp ?? Number.MAX_SAFE_INTEGER),
  );
}
