import { computed, ref, watch, type Ref } from 'vue';
import {
  mergeTaskRuntimeSnapshot,
  createEmptyWorkspaceView,
  toDebugRoundItem,
  toWorkspaceContextView,
  toWorkspaceView,
  type BackendActiveTask,
  type BackendRuntimeSnapshot,
  type BackendWorkspaceSnapshot,
  type DebugRoundItem,
  type TaskActivityStatus,
  type WorkspaceView,
} from '@/data/mock';
import { debugChat } from '@/lib/chatDebug';
import type { WorkspaceSnapshotState } from './types';

type UseWorkspaceSnapshotStateOptions = {
  taskActivityStatuses: Readonly<{ value: Record<number, TaskActivityStatus> }>;
};

export function useWorkspaceSnapshotState({
  snapshot,
  taskActivityStatuses,
}: UseWorkspaceSnapshotStateOptions & {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
}): WorkspaceSnapshotState {
  const optimisticTaskId = ref<string | null>(null);
  const optimisticActiveTaskId = ref<string | null>(null);
  const optimisticDeletedTaskIds = ref<Set<string>>(new Set());
  // `snapshot.active_task` remains the active-task source of truth. Inactive task
  // context stays warm here as backend-shaped data, and the right panel derives
  // its view on demand to avoid maintaining mirrored source/view caches.
  const taskContextSources = ref<Record<number, BackendActiveTask>>({});
  const taskDebugTraces = ref<Record<number, DebugRoundItem[]>>({});
  const workspacePath = computed(() => snapshot.value?.workspace_path);
  const lastResolvedWorkspace = ref<WorkspaceView>(
    createEmptyWorkspaceView({
      workspacePath: workspacePath.value,
      workingDirectory: workspacePath.value,
    }),
  );

  const workspace = computed<WorkspaceView>(() => {
    const activeTaskId = snapshot.value?.active_task?.task.id ?? (snapshot.value?.tasks[0]?.id ?? null);

    if (!snapshot.value) {
      return lastResolvedWorkspace.value;
    }

    const baseWorkspace = toWorkspaceView(snapshot.value);

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
    };
  });

  watch(
    workspace,
    (nextWorkspace) => {
      lastResolvedWorkspace.value = nextWorkspace;
    },
    { immediate: true },
  );

  watch(
    () => snapshot.value?.active_task,
    (activeTask) => {
      if (!activeTask) {
        return;
      }

      const taskId = activeTask.task.id;
      syncTaskContextSnapshot(taskId, activeTask);
      const nextRounds = activeTask.debug_trace?.rounds.map(toDebugRoundItem) ?? [];
      hydrateTaskDebugTrace(taskId, nextRounds);
    },
    { immediate: true },
  );

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

  const taskListView = computed(() => ({
    tasks: resolvedWorkspace.value.tasks,
    activeTaskId: resolvedWorkspace.value.activeTaskId,
  }));

  const composerView = computed(() => ({
    selectedModel: resolvedWorkspace.value.selectedModel,
    selectedTemperature: resolvedWorkspace.value.selectedTemperature,
    selectedTopP: resolvedWorkspace.value.selectedTopP,
    selectedPresencePenalty: resolvedWorkspace.value.selectedPresencePenalty,
    selectedFrequencyPenalty: resolvedWorkspace.value.selectedFrequencyPenalty,
    selectedMaxOutputTokens: resolvedWorkspace.value.selectedMaxOutputTokens,
    workingDirectory: resolvedWorkspace.value.workingDirectory,
    workspacePath: resolvedWorkspace.value.workspacePath,
  }));

  const contextView = computed(() => {
    const activeTaskId = activeTaskIdNumber.value;
    if (!activeTaskId) {
      return buildEmptyContextView(resolvedWorkspace.value);
    }

    const contextSource = taskContextSources.value[activeTaskId];
    const baseContext = contextSource
      ? toWorkspaceContextView(
          contextSource,
          contextSource.runtime?.working_directory ?? contextSource.task.working_directory,
        )
      : buildEmptyContextView(resolvedWorkspace.value);

    return {
      ...baseContext,
      debugRounds: taskDebugTraces.value[activeTaskId] ?? [],
    };
  });

  function setTaskRuntimeSnapshot(
    taskId: number,
    runtime: BackendRuntimeSnapshot,
  ) {
    const currentSource = taskContextSources.value[taskId];
    if (!currentSource) {
      debugChat('workspace-state', 'set-task-runtime-snapshot:missing-source', {
        taskId,
        workingDirectory: runtime.working_directory,
      });
      return;
    }

    const nextSource = mergeTaskRuntimeSnapshot(currentSource, runtime);
    taskContextSources.value = {
      ...taskContextSources.value,
      [taskId]: nextSource,
    };
  }

  function syncTaskContextSnapshot(taskId: number, activeTask: BackendActiveTask) {
    taskContextSources.value = {
      ...taskContextSources.value,
      [taskId]: activeTask,
    };
  }

  function hydrateTaskDebugTrace(taskId: number, rounds: DebugRoundItem[]) {
    taskDebugTraces.value = {
      ...taskDebugTraces.value,
      [taskId]: rounds.map((round) => ({
        ...round,
        toolCalls: round.toolCalls.map((toolCall) => ({ ...toolCall })),
        toolResults: [...round.toolResults],
      })),
    };
  }

  function appendTaskDebugRound(taskId: number, round: DebugRoundItem) {
    const current = taskDebugTraces.value[taskId] ?? [];
    const last = current[current.length - 1];
    if (last?.iteration === round.iteration) {
      return;
    }

    taskDebugTraces.value = {
      ...taskDebugTraces.value,
      [taskId]: [
        ...current,
        {
          ...round,
          toolCalls: round.toolCalls.map((toolCall) => ({ ...toolCall })),
          toolResults: [...round.toolResults],
        },
      ],
    };
  }

  function clearDeletedTaskOptimism(taskId: number) {
    optimisticDeletedTaskIds.value = new Set(
      [...optimisticDeletedTaskIds.value].filter((id) => id !== String(taskId)),
    );
  }

  return {
    snapshot,
    workspacePath,
    workspace,
    resolvedWorkspace,
    taskListView,
    composerView,
    contextView,
    optimisticTaskId,
    optimisticActiveTaskId,
    optimisticDeletedTaskIds,
    activeTaskIdNumber,
    setTaskRuntimeSnapshot,
    hydrateTaskDebugTrace,
    appendTaskDebugRound,
    clearDeletedTaskOptimism,
    syncTaskContextSnapshot,
  };
}

function buildEmptyContextView(fallbackWorkspace: WorkspaceView) {
  return {
    notes: fallbackWorkspace.notes,
    openFiles: fallbackWorkspace.openFiles,
    workingDirectory: fallbackWorkspace.workingDirectory,
    hints: fallbackWorkspace.hints,
    skills: fallbackWorkspace.skills,
    memories: fallbackWorkspace.memories,
    memoryWarnings: fallbackWorkspace.memoryWarnings,
    contextUsage: fallbackWorkspace.contextUsage,
    debugRounds: [],
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
    notes: [],
    openFiles: [],
    hints: [],
    skills: [],
    memories: [],
    memoryWarnings: [],
    contextUsage: {
      percent: 0,
      current: '0',
      limit: workspace.contextUsage.limit || '128k',
      sections: [],
    },
    timeline: [],
    debugRounds: [],
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
    notes: [],
    openFiles: [],
    hints: [],
    skills: [],
    memories: [],
    memoryWarnings: [],
    contextUsage: {
      percent: 0,
      current: '0',
      limit: workspace.contextUsage.limit || '128k',
      sections: [],
    },
    timeline: [],
    debugRounds: [],
  };
}
