import { ref, watch, type Ref } from 'vue';
import type {
  BackendActiveTask,
  BackendAgentProgressEvent,
  BackendRuntimeSnapshot,
  BackendWorkspaceSnapshot,
  ChatMessage,
  DebugRoundItem,
  LiveTurn,
  TaskActivityStatus,
} from '@/data/mock';
import { debugChat, summarizeAgentEvent, summarizeLiveTurn, summarizeSnapshot } from '@/lib/chatDebug';
import { toChatMessage, toDebugRoundItem } from '@/data/mock';

type UseLiveTurnsOptions = {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  sendingTaskId: Ref<number | null>;
  errorMessage: Ref<string>;
  workspacePath: Ref<string | undefined>;
  appendTaskChatMessage: (taskId: number, message: ChatMessage) => void;
  appendTaskDebugRound: (taskId: number, round: DebugRoundItem) => void;
  setTaskRuntimeSnapshot: (
    taskId: number,
    runtime: BackendRuntimeSnapshot,
  ) => void;
  syncTaskContextSnapshot: (taskId: number, task: BackendActiveTask) => void;
};

type ArchivedFailedTurn = {
  turnId: string;
  createdAt: number;
  message: ChatMessage;
};

type ArchivedIntermediateTurn = {
  id: string;
  turnId: string;
  createdAt: number;
  message: ChatMessage;
};

export function useLiveTurns({
  snapshot,
  sendingTaskId,
  errorMessage,
  workspacePath,
  appendTaskChatMessage,
  appendTaskDebugRound,
  setTaskRuntimeSnapshot,
  syncTaskContextSnapshot,
}: UseLiveTurnsOptions) {
  const liveTurns = ref<Record<number, LiveTurn>>({});
  const archivedFailedTurns = ref<Record<number, ArchivedFailedTurn[]>>({});
  const archivedIntermediateTurns = ref<Record<number, ArchivedIntermediateTurn[]>>({});
  const taskActivityStatuses = ref<Record<number, TaskActivityStatus>>({});
  const closedLiveTurnIds = ref<Record<number, string>>({});
  const previewLogCounts: Record<string, number> = {};

  watch(
    workspacePath,
    () => {
      archivedFailedTurns.value = loadArchivedFailedTurns(workspacePath.value);
      archivedIntermediateTurns.value = loadArchivedIntermediateTurns(workspacePath.value);
    },
    { immediate: true },
  );

  function applyAgentProgress(event: BackendAgentProgressEvent) {
    logAgentProgress(event);

    switch (event.kind) {
      case 'turn_started':
        if (shouldIgnoreClosedLiveTurnEvent(event.task_id, event.turn_id)) {
          debugChat('live-turns', 'event:ignored-closed-turn', summarizeAgentEvent(event));
          return;
        }
        setTaskActivity(event.task_id, 'working');
        upsertLiveTurn(event.task_id, {
          turnId: event.turn_id,
          author: formatAgentAuthor(event.agent_display_name || event.agent),
          state: 'pending',
          statusLabel: '正在整理上下文',
          content: '',
          errorMessage: '',
          tools: [],
        });
        return;
      case 'status':
        withMutableLiveTurn(event, (liveTurn) => ({
          ...liveTurn,
          author: formatAgentAuthor(event.agent_display_name || event.agent),
          state: liveTurn.content ? liveTurn.state : 'running',
          statusLabel: event.label,
        }));
        return;
      case 'tool_started':
        withMutableLiveTurn(event, (liveTurn) => ({
          ...liveTurn,
          tools: [
            ...liveTurn.tools,
            {
              id: event.tool_call_id,
              label: event.tool_name,
              summary: event.summary,
              state: 'running',
            },
          ],
        }));
        return;
      case 'tool_finished':
        withMutableLiveTurn(event, (liveTurn) => ({
          ...liveTurn,
          tools: liveTurn.tools.map((tool) =>
            tool.id === event.tool_call_id
              ? {
                  ...tool,
                  state: event.status,
                  summary: event.summary || tool.summary,
                  preview: event.preview ?? undefined,
                }
              : tool,
          ),
        }));
        return;
      case 'assistant_text_preview':
        withMutableLiveTurn(event, (liveTurn) => ({
          ...liveTurn,
          author: formatAgentAuthor(event.agent_display_name || event.agent),
          state: 'streaming',
          statusLabel: '正在生成回复',
          content: event.message,
          errorMessage: '',
        }));
        return;
      case 'assistant_message_checkpoint':
        withMutableLiveTurn(event, (liveTurn) => {
          if (event.checkpoint_type !== 'intermediate') {
            return liveTurn;
          }

          archiveIntermediateTurn(event.task_id, {
            ...liveTurn,
            author: formatAgentAuthor(event.agent_display_name || event.agent),
            content: event.content,
          }, event.message_id);
          return {
            ...liveTurn,
            author: formatAgentAuthor(event.agent_display_name || event.agent),
            state: 'running',
            statusLabel: '正在继续处理',
            content: '',
            errorMessage: '',
            transitionKey: (liveTurn.transitionKey ?? 0) + 1,
          };
        });
        return;
      case 'final_assistant_message':
        sealLiveTurn(event.task_id, event.turn_id);
        setTaskActivity(event.task_id, 'review');
        appendTaskChatMessage(event.task_id, toChatMessage(event.assistant_message));
        syncTaskContextSnapshot(event.task_id, event.task);
        mergeActiveTaskSnapshot(event.task_id, event.task, event.assistant_message);
        clearLiveTurn(event.task_id);
        return;
      case 'round_complete':
        setTaskActivity(event.task_id, 'review');
        appendTaskDebugRound(event.task_id, toDebugRoundItem(event.debug_round));
        syncTaskContextSnapshot(event.task_id, event.task);
        mergeActiveTaskSnapshot(event.task_id, event.task);
        return;
      case 'turn_failed':
      {
        clearTaskActivity(event.task_id);
        const liveTurn = ensureLiveTurn(event.task_id, event.turn_id);
        const failedTurn: LiveTurn = {
          ...liveTurn,
          state: 'error',
          statusLabel: '本轮执行失败',
          errorMessage: event.message,
        };
        upsertLiveTurn(event.task_id, failedTurn);
        archiveFailedTurn(event.task_id, failedTurn);
        sealLiveTurn(event.task_id, event.turn_id);
        if (sendingTaskId.value === event.task_id) {
          sendingTaskId.value = null;
        }
        if (snapshot.value?.active_task?.task.id === event.task_id) {
          errorMessage.value = event.message;
        }
        return;
      }
      case 'turn_cancelled':
      {
        clearTaskActivity(event.task_id);
        syncTaskContextSnapshot(event.task_id, event.task);
        mergeActiveTaskSnapshot(event.task_id, event.task);
        const liveTurn = ensureLiveTurn(event.task_id, event.turn_id);
        upsertLiveTurn(event.task_id, {
          ...liveTurn,
          state: 'cancelled',
          statusLabel: '本轮已中断',
          errorMessage: '',
        });
        sealLiveTurn(event.task_id, event.turn_id);
        if (sendingTaskId.value === event.task_id) {
          sendingTaskId.value = null;
        }
        errorMessage.value = '';
        return;
      }
    }
  }

  function ensureLiveTurn(taskId: number, turnId: string): LiveTurn {
    const existing = liveTurns.value[taskId];
    if (existing?.turnId === turnId) {
      debugChat('live-turns', 'ensure-live-turn:reuse-existing', {
        taskId,
        turnId,
      });
      return existing;
    }

    debugChat('live-turns', 'ensure-live-turn:create', {
      taskId,
      turnId,
    });
    const createdTurn: LiveTurn = {
      turnId,
      author: 'March',
      state: 'running',
      statusLabel: '正在处理',
      content: '',
      errorMessage: '',
      tools: [],
      transitionKey: 0,
    };
    upsertLiveTurn(taskId, createdTurn);
    return createdTurn;
  }

  function withMutableLiveTurn(
    event: Extract<
      BackendAgentProgressEvent,
      { kind: 'status' | 'tool_started' | 'tool_finished' | 'assistant_text_preview' | 'assistant_message_checkpoint' }
    >,
    patch: (liveTurn: LiveTurn) => LiveTurn,
  ) {
    if (shouldIgnoreClosedLiveTurnEvent(event.task_id, event.turn_id)) {
      debugChat('live-turns', 'event:ignored-closed-turn', summarizeAgentEvent(event));
      return;
    }

    setTaskActivity(event.task_id, 'working');
    setTaskRuntimeSnapshot(event.task_id, event.runtime);
    const liveTurn = ensureLiveTurn(event.task_id, event.turn_id);
    upsertLiveTurn(event.task_id, patch(liveTurn));
  }

  function shouldIgnoreClosedLiveTurnEvent(taskId: number, turnId: string) {
    return closedLiveTurnIds.value[taskId] === turnId;
  }

  function sealLiveTurn(taskId: number, turnId: string) {
    if (closedLiveTurnIds.value[taskId] === turnId) {
      debugChat('live-turns', 'seal-live-turn:skip-duplicate', {
        taskId,
        turnId,
      });
      return;
    }

    // The invoke response and the progress event stream are delivered through
    // different channels. A status event from an already-finished turn can land
    // after the final assistant message and would otherwise recreate the live
    // bubble as "正在调用模型". Once a turn has closed its live bubble, later
    // live-turn mutations for the same turn id must be ignored.
    closedLiveTurnIds.value = {
      ...closedLiveTurnIds.value,
      [taskId]: turnId,
    };
    debugChat('live-turns', 'seal-live-turn', {
      taskId,
      turnId,
    });
  }

  function upsertLiveTurn(taskId: number, turn: LiveTurn) {
    liveTurns.value = {
      ...liveTurns.value,
      [taskId]: turn,
    };
    debugChat('live-turns', 'upsert-live-turn', {
      taskId,
      ...summarizeLiveTurn(turn),
    });
  }

  function archiveFailedTurn(taskId: number, turn: LiveTurn) {
    const errorDetail = turn.errorMessage?.trim() || '这轮没有成功完成。';
    const content = turn.content.trim()
      ? `${turn.content.trim()}\n\n[本轮执行失败]\n${errorDetail}`
      : `本轮执行失败\n\n${errorDetail}`;
    const message: ChatMessage = {
      id: `failed:${turn.turnId}:${Date.now()}`,
      role: 'assistant',
      author: turn.author,
      time: formatArchivedTurnTime(Date.now()),
      timestamp: Date.now(),
      content,
      tools: turn.tools.map((tool) => ({
        label: tool.label,
        summary: tool.summary || tool.preview || tool.label,
      })),
      variant: 'failed',
    };

    const nextTaskEntries = [
      ...(archivedFailedTurns.value[taskId] ?? []).filter((entry) => entry.turnId !== turn.turnId),
      {
        turnId: turn.turnId,
        createdAt: Date.now(),
        message,
      },
    ].sort((left, right) => left.createdAt - right.createdAt);

    archivedFailedTurns.value = {
      ...archivedFailedTurns.value,
      [taskId]: nextTaskEntries,
    };
    persistArchivedFailedTurns(workspacePath.value, archivedFailedTurns.value);
  }

  function setTaskActivity(taskId: number, status: TaskActivityStatus) {
    taskActivityStatuses.value = {
      ...taskActivityStatuses.value,
      [taskId]: status,
    };
  }

  function mergeActiveTaskSnapshot(
    taskId: number,
    nextTask: NonNullable<BackendWorkspaceSnapshot['active_task']>,
    finalAssistantMessage?: BackendWorkspaceSnapshot['active_task'] extends infer ActiveTask
      ? ActiveTask extends { history: Array<infer Turn> }
        ? Turn
        : never
      : never,
  ) {
    if (!snapshot.value?.active_task || snapshot.value.active_task.task.id !== taskId) {
      debugChat('live-turns', 'merge-task-snapshot:skip-inactive-task', {
        taskId,
        activeTaskId: snapshot.value?.active_task?.task.id ?? null,
      });
      return;
    }

    const currentTask = snapshot.value.active_task;
    const mergedHistory = mergeHistoryTurns(
      currentTask.history,
      nextTask.history,
      finalAssistantMessage ?? null,
    );
    snapshot.value = {
      ...snapshot.value,
      active_task: {
        ...currentTask,
        ...nextTask,
        runtime: nextTask.runtime ?? currentTask.runtime,
        debug_trace: nextTask.debug_trace ?? currentTask.debug_trace,
        history: mergedHistory,
      },
    };
    debugChat('live-turns', 'merge-task-snapshot:applied', summarizeSnapshot(snapshot.value));
  }

  function clearTaskActivity(taskId: number) {
    if (!(taskId in taskActivityStatuses.value)) {
      return;
    }

    const nextStatuses = { ...taskActivityStatuses.value };
    delete nextStatuses[taskId];
    taskActivityStatuses.value = nextStatuses;
  }

  function archiveIntermediateTurn(taskId: number, turn: LiveTurn, messageId?: string) {
    const content = turn.content.trim();
    if (!content) {
      return;
    }

    const createdAt = Date.now();
    const id = messageId || `intermediate:${turn.turnId}:${createdAt}`;
    const message: ChatMessage = {
      id,
      role: 'assistant',
      author: turn.author,
      time: formatArchivedTurnTime(createdAt),
      timestamp: createdAt,
      content,
      tools: turn.tools.map((tool) => ({
        label: tool.label,
        summary: tool.summary || tool.preview || tool.label,
      })),
      variant: 'intermediate',
    };

    const nextTaskEntries = [
      ...(archivedIntermediateTurns.value[taskId] ?? []).filter((entry) => entry.id !== id),
      {
        id,
        turnId: turn.turnId,
        createdAt,
        message,
      },
    ];

    archivedIntermediateTurns.value = {
      ...archivedIntermediateTurns.value,
      [taskId]: nextTaskEntries,
    };
    persistArchivedIntermediateTurns(workspacePath.value, archivedIntermediateTurns.value);
  }

  function clearLiveTurn(taskId: number) {
    if (!(taskId in liveTurns.value)) {
      debugChat('live-turns', 'clear-live-turn:skip-missing', {
        taskId,
      });
      return;
    }

    debugChat('live-turns', 'clear-live-turn', {
      taskId,
      ...summarizeLiveTurn(liveTurns.value[taskId]),
    });
    const nextTurns = { ...liveTurns.value };
    delete nextTurns[taskId];
    liveTurns.value = nextTurns;
  }

  function clearArchivedFailedTurns(taskId: number) {
    if (!(taskId in archivedFailedTurns.value)) {
      return;
    }

    const nextTurns = { ...archivedFailedTurns.value };
    delete nextTurns[taskId];
    archivedFailedTurns.value = nextTurns;
    persistArchivedFailedTurns(workspacePath.value, archivedFailedTurns.value);
  }

  function clearArchivedIntermediateTurns(taskId: number) {
    if (!(taskId in archivedIntermediateTurns.value)) {
      return;
    }

    const nextTurns = { ...archivedIntermediateTurns.value };
    delete nextTurns[taskId];
    archivedIntermediateTurns.value = nextTurns;
    persistArchivedIntermediateTurns(workspacePath.value, archivedIntermediateTurns.value);
  }

  return {
    liveTurns,
    archivedFailedTurns,
    archivedIntermediateTurns,
    taskActivityStatuses,
    applyAgentProgress,
    upsertLiveTurn,
    archiveFailedTurn,
    archiveIntermediateTurn,
    clearLiveTurn,
    clearTaskActivity,
    clearArchivedFailedTurns,
    clearArchivedIntermediateTurns,
  };

  function logAgentProgress(event: BackendAgentProgressEvent) {
    if (event.kind !== 'assistant_text_preview') {
      debugChat('live-turns', 'event:apply', summarizeAgentEvent(event));
      return;
    }

    const key = `${event.task_id}:${event.turn_id}`;
    const nextCount = (previewLogCounts[key] ?? 0) + 1;
    previewLogCounts[key] = nextCount;
    if (nextCount === 1 || nextCount % 20 === 0) {
      debugChat('live-turns', 'event:apply-preview', {
        ...summarizeAgentEvent(event),
        previewCount: nextCount,
      });
    }
  }
}

function storageKey(workspacePath?: string) {
  return workspacePath ? `ma:archived-failed-turns:${workspacePath}` : '';
}

function intermediateStorageKey(workspacePath?: string) {
  return workspacePath ? `ma:archived-intermediate-turns:${workspacePath}` : '';
}

function mergeHistoryTurns(
  currentHistory: NonNullable<BackendWorkspaceSnapshot['active_task']>['history'],
  nextHistory: NonNullable<BackendWorkspaceSnapshot['active_task']>['history'],
  finalAssistantMessage: NonNullable<BackendWorkspaceSnapshot['active_task']>['history'][number] | null,
) {
  const merged = [...nextHistory];
  const hasFinalAssistantMessage = finalAssistantMessage
    ? merged.some((turn) =>
      turn.role === finalAssistantMessage.role
      && turn.timestamp === finalAssistantMessage.timestamp
      && turn.content === finalAssistantMessage.content)
    : false;

  if (finalAssistantMessage && !hasFinalAssistantMessage) {
    merged.push(finalAssistantMessage);
  }

  return merged.length >= currentHistory.length ? merged : currentHistory;
}

function loadArchivedFailedTurns(workspacePath?: string) {
  const key = storageKey(workspacePath);
  if (!key || typeof window === 'undefined') {
    return {};
  }

  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) {
      return {};
    }
    return JSON.parse(raw) as Record<number, ArchivedFailedTurn[]>;
  } catch {
    return {};
  }
}

function persistArchivedFailedTurns(
  workspacePath: string | undefined,
  records: Record<number, ArchivedFailedTurn[]>,
) {
  const key = storageKey(workspacePath);
  if (!key || typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(key, JSON.stringify(records));
  } catch {
    // Ignore local cache persistence failures; this layer is user-view only.
  }
}

function loadArchivedIntermediateTurns(workspacePath?: string) {
  const key = intermediateStorageKey(workspacePath);
  if (!key || typeof window === 'undefined') {
    return {};
  }

  try {
    const raw = window.localStorage.getItem(key);
    if (!raw) {
      return {};
    }
    return JSON.parse(raw) as Record<number, ArchivedIntermediateTurn[]>;
  } catch {
    return {};
  }
}

function persistArchivedIntermediateTurns(
  workspacePath: string | undefined,
  records: Record<number, ArchivedIntermediateTurn[]>,
) {
  const key = intermediateStorageKey(workspacePath);
  if (!key || typeof window === 'undefined') {
    return;
  }

  try {
    window.localStorage.setItem(key, JSON.stringify(records));
  } catch {
    // Ignore local cache persistence failures; this layer is user-view only.
  }
}

function formatArchivedTurnTime(timestamp: number) {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

function formatAgentAuthor(agent?: string) {
  const normalized = agent?.trim();
  if (!normalized) {
    return 'March';
  }
  if (normalized.toLowerCase() === 'march') {
    return 'March';
  }
  return normalized;
}
