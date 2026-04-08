import type { BackendAgentProgressEvent, BackendWorkspaceSnapshot, LiveTurn } from '@/data/mock';

declare global {
  interface Window {
    __MA_CHAT_DEBUG__?: boolean;
    __MA_CHAT_DEBUG_SEQ__?: number;
  }
}

export function debugChat(scope: string, event: string, details?: Record<string, unknown>) {
  if (typeof window !== 'undefined' && window.__MA_CHAT_DEBUG__ === false) {
    return;
  }

  const seq = nextSequence();
  const prefix = `[ma-chat-debug #${seq}] ${new Date().toISOString()} ${scope} ${event}`;
  if (!details || Object.keys(details).length === 0) {
    console.log(prefix);
    return;
  }

  console.log(prefix, details);
}

export function summarizeSnapshot(snapshot: BackendWorkspaceSnapshot | null | undefined) {
  if (!snapshot) {
    return {
      hasSnapshot: false,
    };
  }

  return {
    hasSnapshot: true,
    tasksCount: snapshot.tasks.length,
    activeTaskId: snapshot.active_task?.task.id ?? null,
    activeHistoryLength: snapshot.active_task?.history.length ?? 0,
    notesCount: snapshot.active_task?.notes.length ?? 0,
    openFilesCount: snapshot.active_task?.open_files.length ?? 0,
    hintsCount: snapshot.active_task?.hints.length ?? 0,
    debugRoundsCount: snapshot.active_task?.debug_trace?.rounds.length ?? 0,
    runtimeOpenFilesCount: snapshot.active_task?.runtime?.open_files.length ?? 0,
  };
}

export function summarizeLiveTurn(turn: LiveTurn | null | undefined) {
  if (!turn) {
    return {
      hasLiveTurn: false,
    };
  }

  return {
    hasLiveTurn: true,
    turnId: turn.turnId,
    state: turn.state,
    statusLabel: turn.statusLabel,
    contentLength: turn.content.length,
    toolsCount: turn.tools.length,
  };
}

export function summarizeAgentEvent(event: BackendAgentProgressEvent) {
  switch (event.kind) {
    case 'turn_started':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        userMessageLength: event.user_message.length,
        agent: event.agent,
      };
    case 'status':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        phase: event.phase,
        label: event.label,
      };
    case 'tool_started':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        toolCallId: event.tool_call_id,
        toolName: event.tool_name,
        summary: event.summary,
      };
    case 'tool_finished':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        toolCallId: event.tool_call_id,
        status: event.status,
        summary: event.summary,
      };
    case 'assistant_text_preview':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        messageLength: event.message.length,
      };
    case 'assistant_message_checkpoint':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        checkpointType: event.checkpoint_type,
        messageId: event.message_id,
        contentLength: event.content.length,
      };
    case 'final_assistant_message':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        assistantMessageTimestamp: event.assistant_message.timestamp,
        assistantMessageLength: event.assistant_message.content.length,
        taskHistoryLength: event.task.history.length,
        debugRoundsCount: event.task.debug_trace?.rounds.length ?? 0,
      };
    case 'round_complete':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        iteration: event.debug_round.iteration,
        taskHistoryLength: event.task.history.length,
        debugRoundsCount: event.task.debug_trace?.rounds.length ?? 0,
      };
    case 'turn_failed':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        stage: event.stage,
        message: event.message,
      };
    case 'turn_cancelled':
      return {
        kind: event.kind,
        taskId: event.task_id,
        turnId: event.turn_id,
        taskHistoryLength: event.task.history.length,
      };
  }
}

function nextSequence() {
  if (typeof window === 'undefined') {
    return 0;
  }

  const next = (window.__MA_CHAT_DEBUG_SEQ__ ?? 0) + 1;
  window.__MA_CHAT_DEBUG_SEQ__ = next;
  return next;
}
