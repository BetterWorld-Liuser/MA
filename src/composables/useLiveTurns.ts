import { ref, watch, type Ref } from 'vue';
import type { BackendAgentProgressEvent, BackendWorkspaceSnapshot, ChatMessage, LiveTurn } from '@/data/mock';

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

export function useLiveTurns({ snapshot, sendingTaskId, errorMessage, workspacePath }: UseLiveTurnsOptions) {
  const liveTurns = ref<Record<number, LiveTurn>>({});
  const archivedFailedTurns = ref<Record<number, ArchivedFailedTurn[]>>({});

  watch(
    workspacePath,
    () => {
      archivedFailedTurns.value = loadArchivedFailedTurns(workspacePath.value);
    },
    { immediate: true },
  );

  function applyAgentProgress(event: BackendAgentProgressEvent) {
    switch (event.kind) {
      case 'turn_started':
        upsertLiveTurn(event.task_id, {
          turnId: event.turn_id,
          state: 'pending',
          statusLabel: '正在整理上下文',
          content: '',
          errorMessage: '',
          tools: [],
        });
        return;
      case 'status':
        ensureLiveTurn(event.task_id, event.turn_id);
        if (!liveTurns.value[event.task_id]) {
          return;
        }
        upsertLiveTurn(event.task_id, {
          ...liveTurns.value[event.task_id],
          state: liveTurns.value[event.task_id].content ? liveTurns.value[event.task_id].state : 'running',
          statusLabel: event.label,
        });
        return;
      case 'tool_started':
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
        ensureLiveTurn(event.task_id, event.turn_id);
        if (!liveTurns.value[event.task_id]) {
          return;
        }
        upsertLiveTurn(event.task_id, {
          ...liveTurns.value[event.task_id],
          state: 'streaming',
          statusLabel: '正在生成回复',
          content: event.message,
          errorMessage: '',
        });
        return;
      case 'final_assistant_message':
        if (snapshot.value?.active_task?.task.id === event.task_id) {
          snapshot.value = {
            ...snapshot.value,
            active_task: event.task,
          };
        }
        clearLiveTurn(event.task_id);
        return;
      case 'round_complete':
        if (snapshot.value?.active_task?.task.id === event.task_id) {
          snapshot.value = {
            ...snapshot.value,
            active_task: event.task,
          };
        }
        return;
      case 'turn_failed':
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

  function ensureLiveTurn(taskId: number, turnId: string) {
    if (liveTurns.value[taskId]?.turnId === turnId) {
      return;
    }

    upsertLiveTurn(taskId, {
      turnId,
      state: 'running',
      statusLabel: '正在处理',
      content: '',
      errorMessage: '',
      tools: [],
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
      role: 'assistant',
      author: 'March',
      time: formatArchivedTurnTime(Date.now()),
      timestamp: Date.now(),
      content,
      tools: turn.tools.map((tool) => ({
        label: tool.label,
        summary: tool.summary || tool.preview || tool.label,
      })),
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

  return {
    liveTurns,
    archivedFailedTurns,
    applyAgentProgress,
    upsertLiveTurn,
    archiveFailedTurn,
    clearLiveTurn,
    clearArchivedFailedTurns,
  };
}

function storageKey(workspacePath?: string) {
  return workspacePath ? `ma:archived-failed-turns:${workspacePath}` : '';
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

function formatArchivedTurnTime(timestamp: number) {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}
