import { computed, ref, watch, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import {
  type BackendTaskHistoryView,
  toDebugRoundItem,
  toTaskTimelineEntries,
  type BackendAgentProgressEvent,
  type BackendRuntimeSnapshot,
  type BackendWorkspaceSnapshot,
  type ChatImageAttachment,
  type DebugRoundItem,
  type ReplyRef,
  type TaskActivityStatus,
  type TaskTimelineEntry,
} from '@/data/mock';
import { applyAgentEventToTimeline, appendUserMessage } from '@/composables/chatRuntime/chatEventReducer';
import { debugChat } from '@/lib/chatDebug';
import type { WorkspaceChatView } from './types';

type UseTaskTimelineStateOptions = {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  activeTaskIdNumber: Readonly<Ref<number | null>>;
  taskActivityStatuses: Ref<Record<number, TaskActivityStatus>>;
  setTaskRuntimeSnapshot: (taskId: number, runtime: BackendRuntimeSnapshot) => void;
  syncTaskContextSnapshot: (taskId: number, task: NonNullable<BackendWorkspaceSnapshot['active_task']>) => void;
  appendTaskDebugRound: (taskId: number, round: DebugRoundItem) => void;
};

export function useTaskTimelineState({
  snapshot,
  activeTaskIdNumber,
  taskActivityStatuses,
  setTaskRuntimeSnapshot,
  syncTaskContextSnapshot,
  appendTaskDebugRound,
}: UseTaskTimelineStateOptions) {
  const taskTimelines = ref<Record<number, TaskTimelineEntry[]>>({});
  const taskLastSeqs = ref<Record<number, number>>({});
  const hydratedTaskIds = ref<Set<number>>(new Set());

  async function loadTaskHistory(taskId: number) {
    const history = await invoke<BackendTaskHistoryView>('get_task_history', { taskId });
    hydrateTaskTimeline(taskId, toTaskTimelineEntries(history.timeline), history.last_seq);
    hydratedTaskIds.value = new Set([...hydratedTaskIds.value, taskId]);
  }

  watch(
    () => snapshot.value?.active_task,
    async (activeTask) => {
      if (!activeTask) {
        return;
      }

      const taskId = activeTask.task.id;
      syncTaskContextSnapshot(taskId, activeTask);
      if (hydratedTaskIds.value.has(taskId)) {
        // Keep the local seq cursor intact for hydrated tasks. The active-task
        // snapshot is persisted state and can lag behind the buffered replay
        // events; bumping the cursor here would make subscribe_task skip the
        // missed turn_finished/message_finished events we need to replay.
        return;
      }

      await loadTaskHistory(taskId);
    },
    { immediate: true },
  );

  const chatView = computed<WorkspaceChatView>(() => ({
    timeline: activeTaskIdNumber.value ? taskTimelines.value[activeTaskIdNumber.value] ?? [] : [],
  }));

  function hydrateTaskTimeline(taskId: number, timeline: TaskTimelineEntry[], lastSeq = 0) {
    taskTimelines.value = {
      ...taskTimelines.value,
      [taskId]: timeline.map(cloneTimelineEntry),
    };
    taskLastSeqs.value = {
      ...taskLastSeqs.value,
      [taskId]: lastSeq,
    };
  }

  function optimisticAppendUserMessage(
    taskId: number,
    input: {
      id: string;
      content: string;
      ts?: number;
      mentions?: string[];
      replies?: ReplyRef[];
      images?: ChatImageAttachment[];
    },
  ) {
    taskTimelines.value = {
      ...taskTimelines.value,
      [taskId]: appendUserMessage(taskTimelines.value[taskId] ?? [], input),
    };
  }

  function applyAgentProgress(event: BackendAgentProgressEvent) {
    const currentLastSeq = taskLastSeqs.value[event.task_id] ?? 0;
    if (event.seq <= currentLastSeq) {
      debugChat('task-timeline', 'event:skip-stale', {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        lastSeq: currentLastSeq,
      });
      return;
    }

    debugChat('task-timeline', 'event:apply', {
      kind: event.kind,
      taskId: event.task_id,
      turnId: 'turn_id' in event ? event.turn_id : null,
    });

    if ('runtime' in event) {
      setTaskRuntimeSnapshot(event.task_id, event.runtime);
    }
    if ('task' in event) {
      syncTaskContextSnapshot(event.task_id, event.task);
    }
    if (event.kind === 'round_complete') {
      appendTaskDebugRound(event.task_id, toDebugRoundItem(event.debug_round));
    }

    taskTimelines.value = {
      ...taskTimelines.value,
      [event.task_id]: applyAgentEventToTimeline(taskTimelines.value[event.task_id] ?? [], event),
    };
    taskLastSeqs.value = {
      ...taskLastSeqs.value,
      [event.task_id]: event.seq,
    };

    switch (event.kind) {
      case 'user_message_appended':
        break;
      case 'turn_started':
      case 'message_started':
      case 'tool_started':
      case 'tool_finished':
      case 'assistant_stream_delta':
      case 'message_finished':
        taskActivityStatuses.value[event.task_id] = 'working';
        break;
      case 'turn_finished':
      case 'round_complete':
        taskActivityStatuses.value[event.task_id] = 'review';
        break;
    }
  }

  function clearTaskTimeline(taskId: number) {
    if (!(taskId in taskTimelines.value)) {
      return;
    }

    const nextTimelines = { ...taskTimelines.value };
    delete nextTimelines[taskId];
    taskTimelines.value = nextTimelines;

    const nextHydrated = new Set(hydratedTaskIds.value);
    nextHydrated.delete(taskId);
    hydratedTaskIds.value = nextHydrated;

    const nextLastSeqs = { ...taskLastSeqs.value };
    delete nextLastSeqs[taskId];
    taskLastSeqs.value = nextLastSeqs;
  }

  function markTaskTimelineNeedsHydration(taskId: number) {
    const nextHydrated = new Set(hydratedTaskIds.value);
    nextHydrated.delete(taskId);
    hydratedTaskIds.value = nextHydrated;
  }

  function clearTaskActivity(taskId: number) {
    delete taskActivityStatuses.value[taskId];
  }

  return {
    chatView,
    taskActivityStatuses,
    hydrateTaskTimeline,
    optimisticAppendUserMessage,
    applyAgentProgress,
    clearTaskTimeline,
    markTaskTimelineNeedsHydration,
    clearTaskActivity,
    loadTaskHistory,
    getTaskLastSeq: (taskId: number) => taskLastSeqs.value[taskId] ?? 0,
  };
}

function cloneTimelineEntry(entry: TaskTimelineEntry): TaskTimelineEntry {
  if (entry.kind === 'user_message') {
    return {
      ...entry,
      mentions: [...entry.mentions],
      replies: entry.replies.map((reply) => ({ ...reply })),
      images: entry.images ? [...entry.images] : undefined,
    };
  }

  return {
    ...entry,
    trigger: { ...entry.trigger },
    messages: entry.messages.map((message) => ({
      ...message,
      timeline: message.timeline.map((timelineEntry) => ({ ...timelineEntry })),
    })),
  };
}
