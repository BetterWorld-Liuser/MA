import { ref, watch, type Ref } from 'vue';
import type {
  BackendAgentProgressEvent,
  BackendWorkspaceSnapshot,
  ChatMessage,
  LiveTurn,
  TaskActivityStatus,
} from '@/data/mock';

type UseLiveTurnsOptions = {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  sendingTaskId: Ref<number | null>;
  errorMessage: Ref<string>;
  workspacePath: Ref<string | undefined>;
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

export function useLiveTurns({ snapshot, sendingTaskId, errorMessage, workspacePath }: UseLiveTurnsOptions) {
  const liveTurns = ref<Record<number, LiveTurn>>({});
  const archivedFailedTurns = ref<Record<number, ArchivedFailedTurn[]>>({});
  const archivedIntermediateTurns = ref<Record<number, ArchivedIntermediateTurn[]>>({});
  const taskActivityStatuses = ref<Record<number, TaskActivityStatus>>({});

  watch(
    workspacePath,
    () => {
      archivedFailedTurns.value = loadArchivedFailedTurns(workspacePath.value);
      archivedIntermediateTurns.value = loadArchivedIntermediateTurns(workspacePath.value);
    },
    { immediate: true },
  );

  function applyAgentProgress(event: BackendAgentProgressEvent) {
    switch (event.kind) {
      case 'turn_started':
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
        setTaskActivity(event.task_id, 'working');
        mergeActiveTaskRuntime(event.task_id, event.runtime);
        ensureLiveTurn(event.task_id, event.turn_id);
        if (!liveTurns.value[event.task_id]) {
          return;
        }
        upsertLiveTurn(event.task_id, {
          ...liveTurns.value[event.task_id],
          author: formatAgentAuthor(event.agent_display_name || event.agent),
          state: liveTurns.value[event.task_id].content ? liveTurns.value[event.task_id].state : 'running',
          statusLabel: event.label,
        });
        return;
      case 'tool_started':
        setTaskActivity(event.task_id, 'working');
        mergeActiveTaskRuntime(event.task_id, event.runtime);
        ensureLiveTurn(event.task_id, event.turn_id);
        if (!liveTurns.value[event.task_id]) {
          return;
        }
        upsertLiveTurn(event.task_id, {
          ...liveTurns.value[event.task_id],
          tools: [
            ...liveTurns.value[event.task_id].tools,
            {
              id: event.tool_call_id,
              label: event.tool_name,
              summary: event.summary,
              state: 'running',
            },
          ],
        });
        return;
      case 'tool_finished':
        setTaskActivity(event.task_id, 'working');
        mergeActiveTaskRuntime(event.task_id, event.runtime);
        ensureLiveTurn(event.task_id, event.turn_id);
        if (!liveTurns.value[event.task_id]) {
          return;
        }
        upsertLiveTurn(event.task_id, {
          ...liveTurns.value[event.task_id],
          tools: liveTurns.value[event.task_id].tools.map((tool) =>
            tool.id === event.tool_call_id
              ? {
                  ...tool,
                  state: event.status,
                  summary: event.summary || tool.summary,
                  preview: event.preview ?? undefined,
                }
              : tool,
          ),
        });
        return;
      case 'assistant_text_preview':
        setTaskActivity(event.task_id, 'working');
        mergeActiveTaskRuntime(event.task_id, event.runtime);
        ensureLiveTurn(event.task_id, event.turn_id);
        if (!liveTurns.value[event.task_id]) {
          return;
        }
        upsertLiveTurn(event.task_id, {
          ...liveTurns.value[event.task_id],
          author: formatAgentAuthor(event.agent_display_name || event.agent),
          state: 'streaming',
          statusLabel: '正在生成回复',
          content: event.message,
          errorMessage: '',
        });
        return;
      case 'assistant_message_checkpoint':
        setTaskActivity(event.task_id, 'working');
        mergeActiveTaskRuntime(event.task_id, event.runtime);
        ensureLiveTurn(event.task_id, event.turn_id);
        if (!liveTurns.value[event.task_id]) {
          return;
        }
        if (event.checkpoint_type === 'intermediate') {
          archiveIntermediateTurn(event.task_id, {
            ...liveTurns.value[event.task_id],
            author: formatAgentAuthor(event.agent_display_name || event.agent),
            content: event.content,
          }, event.message_id);
          upsertLiveTurn(event.task_id, {
            ...liveTurns.value[event.task_id],
            author: formatAgentAuthor(event.agent_display_name || event.agent),
            state: 'running',
            statusLabel: '正在继续处理',
            content: '',
            errorMessage: '',
            transitionKey: (liveTurns.value[event.task_id].transitionKey ?? 0) + 1,
          });
        }
        return;
      case 'final_assistant_message':
        setTaskActivity(event.task_id, 'review');
        if (snapshot.value?.active_task?.task.id === event.task_id) {
          snapshot.value = {
            ...snapshot.value,
            active_task: event.task,
          };
        }
        clearLiveTurn(event.task_id);
        return;
      case 'round_complete':
        setTaskActivity(event.task_id, 'review');
        if (snapshot.value?.active_task?.task.id === event.task_id) {
          snapshot.value = {
            ...snapshot.value,
            active_task: event.task,
          };
        }
        return;
      case 'turn_failed':
        clearTaskActivity(event.task_id);
        ensureLiveTurn(event.task_id, event.turn_id);
        if (!liveTurns.value[event.task_id]) {
          return;
        }
        const failedTurn: LiveTurn = {
          ...liveTurns.value[event.task_id],
          state: 'error',
          statusLabel: '本轮执行失败',
          errorMessage: event.message,
        };
        upsertLiveTurn(event.task_id, failedTurn);
        archiveFailedTurn(event.task_id, failedTurn);
        if (sendingTaskId.value === event.task_id) {
          sendingTaskId.value = null;
        }
        if (snapshot.value?.active_task?.task.id === event.task_id) {
          errorMessage.value = event.message;
        }
        return;
      case 'turn_cancelled':
        clearTaskActivity(event.task_id);
        if (snapshot.value?.active_task?.task.id === event.task_id) {
          snapshot.value = {
            ...snapshot.value,
            active_task: event.task,
          };
        }
        clearLiveTurn(event.task_id);
        if (sendingTaskId.value === event.task_id) {
          sendingTaskId.value = null;
        }
        errorMessage.value = '';
        return;
    }
  }

  function mergeActiveTaskRuntime(
    taskId: number,
    runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>,
  ) {
    if (snapshot.value?.active_task?.task.id !== taskId) {
      return;
    }

    snapshot.value = {
      ...snapshot.value,
      active_task: {
        ...snapshot.value.active_task,
        runtime,
      },
    };
  }

  function ensureLiveTurn(taskId: number, turnId: string) {
    if (liveTurns.value[taskId]?.turnId === turnId) {
      return;
    }

    upsertLiveTurn(taskId, {
      turnId,
      author: 'March',
      state: 'running',
      statusLabel: '正在处理',
      content: '',
      errorMessage: '',
      tools: [],
      transitionKey: 0,
    });
  }

  function upsertLiveTurn(taskId: number, turn: LiveTurn) {
    liveTurns.value = {
      ...liveTurns.value,
      [taskId]: turn,
    };
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
      return;
    }

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
}

function storageKey(workspacePath?: string) {
  return workspacePath ? `ma:archived-failed-turns:${workspacePath}` : '';
}

function intermediateStorageKey(workspacePath?: string) {
  return workspacePath ? `ma:archived-intermediate-turns:${workspacePath}` : '';
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
