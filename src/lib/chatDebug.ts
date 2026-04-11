import type { BackendAgentProgressEvent, BackendWorkspaceSnapshot, Turn } from '@/data/mock';

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
    activeTimelineLength: snapshot.active_task?.timeline.length ?? 0,
    notesCount: snapshot.active_task?.notes.length ?? 0,
    openFilesCount: snapshot.active_task?.open_files.length ?? 0,
    hintsCount: snapshot.active_task?.hints.length ?? 0,
    debugRoundsCount: snapshot.active_task?.debug_trace?.rounds.length ?? 0,
    runtimeOpenFilesCount: snapshot.active_task?.runtime?.open_files.length ?? 0,
  };
}

export function summarizeTurn(turn: Turn | null | undefined) {
  if (!turn) {
    return {
      hasTurn: false,
    };
  }

  return {
    hasTurn: true,
    turnId: turn.turnId,
    state: turn.state,
    statusLabel: turn.statusLabel ?? null,
    messagesCount: turn.messages.length,
  };
}

export function summarizeAgentEvent(event: BackendAgentProgressEvent) {
  switch (event.kind) {
    case 'user_message_appended':
      return {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        userMessageId: event.user_message_id,
        contentLength: event.content.length,
      };
    case 'turn_started':
      return {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        turnId: event.turn_id,
        agent: event.agent,
      };
    case 'message_started':
      return {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        turnId: event.turn_id,
        messageId: event.message_id,
      };
    case 'tool_started':
      return {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        turnId: event.turn_id,
        messageId: event.message_id,
        toolCallId: event.tool_call_id,
        toolName: event.tool_name,
        summary: event.summary,
      };
    case 'tool_finished':
      return {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        turnId: event.turn_id,
        messageId: event.message_id,
        toolCallId: event.tool_call_id,
        status: event.status,
        summary: event.summary,
      };
    case 'assistant_stream_delta':
      return {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        turnId: event.turn_id,
        messageId: event.message_id,
        field: event.field,
        deltaLength: event.delta.length,
      };
    case 'message_finished':
      return {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        turnId: event.turn_id,
        messageId: event.message_id,
      };
    case 'turn_finished':
      return {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        turnId: event.turn_id,
        reason: event.reason,
        taskTimelineLength: event.task.timeline.length,
        debugRoundsCount: event.task.debug_trace?.rounds.length ?? 0,
      };
    case 'round_complete':
      return {
        kind: event.kind,
        taskId: event.task_id,
        seq: event.seq,
        turnId: event.turn_id,
        iteration: event.debug_round.iteration,
        taskTimelineLength: event.task.timeline.length,
        debugRoundsCount: event.task.debug_trace?.rounds.length ?? 0,
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
