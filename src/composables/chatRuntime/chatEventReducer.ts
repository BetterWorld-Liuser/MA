import type {
  AssistantMessage,
  AssistantTimelineEntry,
  BackendAgentProgressEvent,
  ChatImageAttachment,
  ReplyRef,
  TaskTimelineEntry,
  Turn,
} from '@/data/mock';

export function applyAgentEventToTimeline(
  timeline: TaskTimelineEntry[],
  event: BackendAgentProgressEvent,
): TaskTimelineEntry[] {
  switch (event.kind) {
    case 'user_message_appended':
      return appendOrReplaceUserMessage(timeline, {
        kind: 'user_message',
        userMessageId: event.user_message_id,
        content: event.content,
        mentions: event.mentions,
        replies: event.replies,
        ts: event.ts * 1000,
        author: 'User',
      });
    case 'turn_started':
      return upsertTurn(timeline, {
        kind: 'turn',
        turnId: event.turn_id,
        agentId: event.agent || 'march',
        agentName: event.agent_display_name || event.agent || 'March',
        trigger: event.trigger,
        state: 'streaming',
        ts: Date.now(),
        messages: [],
      });
    case 'message_started':
      return mapTurn(timeline, event.turn_id, (turn) => replaceOrAppendMessage(turn, {
        messageId: event.message_id,
        turnId: event.turn_id,
        state: 'streaming',
        reasoning: '',
        timeline: [],
      }));
    case 'tool_started':
      return mapTurn(timeline, event.turn_id, (turn) => {
        const message = findOrCreateMessage(turn, event.message_id);
        return replaceMessage(turn, message.messageId, {
          ...message,
          timeline: [
            ...message.timeline,
            {
              kind: 'tool',
              toolCallId: event.tool_call_id,
              toolName: event.tool_name,
              arguments: '',
              status: 'running',
              preview: event.summary,
            },
          ],
        });
      });
    case 'tool_finished':
      return mapTurn(timeline, event.turn_id, (turn) => {
        const message = findOrCreateMessage(turn, event.message_id);
        return replaceMessage(turn, message.messageId, {
          ...message,
          timeline: message.timeline.map((entry) =>
            entry.kind === 'tool' && entry.toolCallId === event.tool_call_id
              ? {
                  ...entry,
                  status: event.status === 'success' ? 'ok' : 'error',
                  preview: event.preview ?? event.summary,
                }
              : entry,
          ),
        });
      });
    case 'assistant_stream_delta':
      return mapTurn(timeline, event.turn_id, (turn) => {
        const message = findOrCreateMessage(turn, event.message_id);
        if (event.field === 'reasoning') {
          return replaceMessage(turn, message.messageId, {
            ...message,
            reasoning: `${message.reasoning}${event.delta}`,
          });
        }
        if (event.field === 'tool_call_arguments') {
          return replaceMessage(turn, message.messageId, {
            ...message,
            timeline: message.timeline.map((entry) =>
              entry.kind === 'tool' && entry.toolCallId === event.tool_call_id
                ? {
                    ...entry,
                    arguments: `${entry.arguments}${event.delta}`,
                  }
                : entry,
            ),
          });
        }
        return replaceMessage(turn, message.messageId, appendMessageTextDelta(message, event.delta));
      });
    case 'message_finished':
      return mapTurn(timeline, event.turn_id, (turn) => {
        const message = findOrCreateMessage(turn, event.message_id);
        return replaceMessage(turn, message.messageId, {
          ...message,
          state: 'done',
        });
      });
    case 'turn_finished':
      return mapTurn(timeline, event.turn_id, (turn) => {
        return {
          ...turn,
          state:
            event.reason === 'idle'
              ? 'done'
              : event.reason === 'failed'
                ? 'failed'
                : 'cancelled',
          errorMessage: event.error_message ?? undefined,
          messages: markStreamingMessagesDone(turn.messages),
        };
      });
    case 'round_complete':
      return timeline;
  }
}

export function appendUserMessage(
  timeline: TaskTimelineEntry[],
  input: {
    id: string;
    content: string;
    ts?: number;
    mentions?: string[];
    replies?: ReplyRef[];
    images?: ChatImageAttachment[];
  },
): TaskTimelineEntry[] {
  return [
    ...timeline,
    {
      kind: 'user_message',
      userMessageId: input.id,
      clientMessageId: input.id.startsWith('pending-user:') ? input.id : undefined,
      content: input.content,
      mentions: input.mentions ?? [],
      replies: input.replies ?? [],
      ts: input.ts ?? Date.now(),
      author: 'User',
      images: input.images,
    },
  ];
}

function upsertTurn(timeline: TaskTimelineEntry[], turn: Turn): TaskTimelineEntry[] {
  const existingIndex = timeline.findIndex((entry) => entry.kind === 'turn' && entry.turnId === turn.turnId);
  if (existingIndex === -1) {
    return [...timeline, turn];
  }
  return timeline.map((entry, index) => (index === existingIndex ? turn : entry));
}

function appendOrReplaceUserMessage(timeline: TaskTimelineEntry[], message: Extract<TaskTimelineEntry, { kind: 'user_message' }>) {
  const pendingIndex = timeline.findIndex(
    (entry) =>
      entry.kind === 'user_message'
      && entry.userMessageId.startsWith('pending-user:')
      && entry.content === message.content,
  );
  if (pendingIndex !== -1) {
    const pending = timeline[pendingIndex] as Extract<TaskTimelineEntry, { kind: 'user_message' }>;
    const merged = { ...message, clientMessageId: pending.clientMessageId ?? pending.userMessageId };
    return timeline.map((entry, index) => (index === pendingIndex ? merged : entry));
  }

  if (timeline.some((entry) => entry.kind === 'user_message' && entry.userMessageId === message.userMessageId)) {
    return timeline;
  }

  return [...timeline, message];
}

function mapTurn(
  timeline: TaskTimelineEntry[],
  turnId: string,
  updater: (turn: Turn) => Turn,
): TaskTimelineEntry[] {
  let updated = false;
  return timeline.map((entry) => {
    if (entry.kind !== 'turn' || entry.turnId !== turnId) {
      return entry;
    }
    updated = true;
    return updater(entry);
  });
}

function ensureStreamingMessage(turn: Turn): AssistantMessage {
  const lastMessage = turn.messages.at(-1);
  if (lastMessage && lastMessage.state === 'streaming') {
    return lastMessage;
  }
  return createStreamingMessage(turn.turnId, turn.messages.length + 1);
}

function findOrCreateMessage(turn: Turn, messageId: string): AssistantMessage {
  return turn.messages.find((message) => message.messageId === messageId) ?? {
    messageId,
    turnId: turn.turnId,
    state: 'streaming',
    reasoning: '',
    timeline: [],
  };
}

function replaceOrAppendMessage(turn: Turn, nextMessage: AssistantMessage): Turn {
  if (turn.messages.some((message) => message.messageId === nextMessage.messageId)) {
    return replaceMessage(turn, nextMessage.messageId, nextMessage);
  }

  return {
    ...turn,
    messages: [...turn.messages, nextMessage],
  };
}

function createStreamingMessage(turnId: string, ordinal: number): AssistantMessage {
  return {
    messageId: `${turnId}:message:${ordinal}`,
    turnId,
    state: 'streaming',
    reasoning: '',
    timeline: [],
  };
}

function replaceMessage(turn: Turn, messageId: string, nextMessage: AssistantMessage): Turn {
  const messageIndex = turn.messages.findIndex((message) => message.messageId === messageId);
  if (messageIndex === -1) {
    return {
      ...turn,
      messages: [...turn.messages, nextMessage],
    };
  }

  return {
    ...turn,
    messages: turn.messages.map((message, index) => (index === messageIndex ? nextMessage : message)),
  };
}

function setMessageText(message: AssistantMessage, text: string): AssistantMessage {
  const timeline = [...message.timeline];
  const lastEntry = timeline.at(-1);
  if (lastEntry?.kind === 'text') {
    timeline[timeline.length - 1] = {
      kind: 'text',
      textId: lastEntry.textId,
      text,
    };
  } else {
    timeline.push({
      kind: 'text',
      textId: crypto.randomUUID(),
      text,
    });
  }
  return {
    ...message,
    timeline,
  };
}

function appendMessageTextDelta(message: AssistantMessage, delta: string): AssistantMessage {
  const timeline = [...message.timeline];
  const lastEntry = timeline.at(-1);
  if (lastEntry?.kind === 'text') {
    timeline[timeline.length - 1] = {
      kind: 'text',
      textId: lastEntry.textId,
      text: `${lastEntry.text}${delta}`,
    };
  } else {
    timeline.push({
      kind: 'text',
      textId: crypto.randomUUID(),
      text: delta,
    });
  }
  return {
    ...message,
    timeline,
  };
}

function markStreamingMessagesDone(messages: AssistantMessage[]): AssistantMessage[] {
  return messages.map((message) => ({
    ...message,
    state: 'done',
  }));
}

export function countTurnToolCalls(turn: Turn, includeFinalMessage = false) {
  const messages = includeFinalMessage ? turn.messages : turn.messages.slice(0, -1);
  return messages
    .flatMap((message) => message.timeline)
    .filter((entry): entry is Extract<AssistantTimelineEntry, { kind: 'tool' }> => entry.kind === 'tool')
    .length;
}
