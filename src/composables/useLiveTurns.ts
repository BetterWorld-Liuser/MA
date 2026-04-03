import { ref, type Ref } from 'vue';
import type { BackendAgentProgressEvent, BackendWorkspaceSnapshot, LiveTurn } from '@/data/mock';

type UseLiveTurnsOptions = {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  sendingTaskId: Ref<number | null>;
  errorMessage: Ref<string>;
};

export function useLiveTurns({ snapshot, sendingTaskId, errorMessage }: UseLiveTurnsOptions) {
  const liveTurns = ref<Record<number, LiveTurn>>({});

  function applyAgentProgress(event: BackendAgentProgressEvent) {
    switch (event.kind) {
      case 'turn_started':
        upsertLiveTurn(event.task_id, {
          turnId: event.turn_id,
          state: 'pending',
          statusLabel: '正在整理上下文',
          content: '',
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
        upsertLiveTurn(event.task_id, {
          ...liveTurns.value[event.task_id],
          state: 'error',
          statusLabel: '本轮执行失败',
        });
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
      tools: [],
    });
  }

  function upsertLiveTurn(taskId: number, turn: LiveTurn) {
    liveTurns.value = {
      ...liveTurns.value,
      [taskId]: turn,
    };
  }

  function clearLiveTurn(taskId: number) {
    if (!(taskId in liveTurns.value)) {
      return;
    }

    const nextTurns = { ...liveTurns.value };
    delete nextTurns[taskId];
    liveTurns.value = nextTurns;
  }

  return {
    liveTurns,
    applyAgentProgress,
    upsertLiveTurn,
    clearLiveTurn,
  };
}
