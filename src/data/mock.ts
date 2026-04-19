export type TaskItem = {
  id: string;
  name: string;
  status: 'active' | 'idle';
  activityStatus?: 'working' | 'review';
  updatedAt: string;
};

export type ChatImageAttachment = {
  id: string;
  name: string;
  previewUrl: string;
  mediaType: string;
  sourcePath?: string;
};

export type ReplyRef =
  | { kind: 'turn'; id: string }
  | { kind: 'user_message'; id: string };

export type UserMessage = {
  kind: 'user_message';
  userMessageId: string;
  clientMessageId?: string;
  content: string;
  mentions: string[];
  replies: ReplyRef[];
  ts: number;
  author: string;
  images?: ChatImageAttachment[];
};

export type AssistantTimelineTextEntry = {
  kind: 'text';
  textId: string;
  text: string;
};

export type AssistantTimelineToolEntry = {
  kind: 'tool';
  toolCallId: string;
  toolName: string;
  arguments: string;
  status: 'running' | 'ok' | 'error';
  preview?: string;
  durationMs?: number;
};

export type AssistantTimelineEntry = AssistantTimelineTextEntry | AssistantTimelineToolEntry;

export type AssistantMessage = {
  messageId: string;
  turnId: string;
  state: 'streaming' | 'done';
  reasoning: string;
  timeline: AssistantTimelineEntry[];
};

export type Turn = {
  kind: 'turn';
  turnId: string;
  agentId: string;
  agentName: string;
  trigger:
    | { kind: 'user'; id: string }
    | { kind: 'turn'; id: string };
  state: 'streaming' | 'done' | 'failed' | 'cancelled';
  statusLabel?: string;
  errorMessage?: string;
  ts: number;
  messages: AssistantMessage[];
};

export type TaskTimelineEntry = UserMessage | Turn;

export type NoteItem = {
  id: string;
  content: string;
};

export type OpenFileItem = {
  scope: string;
  path: string;
  tokenUsage: string;
  freshness: 'high' | 'medium' | 'low';
  locked: boolean;
  state?: {
    kind: 'available' | 'deleted' | 'moved';
    newPath?: string;
  };
};

export type HintItem = {
  source: string;
  content: string;
  timeLeft: string;
  turnsLeft: string;
};

export type SkillItem = {
  name: string;
  path: string;
  description: string;
  opened: boolean;
};

export type MemoryItem = {
  id: string;
  type: string;
  topic: string;
  title: string;
  level: 'project' | 'global';
};

export type MemoryDetail = {
  id: string;
  memoryType: string;
  topic: string;
  title: string;
  content: string;
  tags: string[];
  scope: string;
  level: 'project' | 'global';
  accessCount: number;
  skipCount: number;
  updatedAt: number;
};

export type ContextUsage = {
  percent: number;
  current: string;
  limit: string;
  sections: Array<{
    name: string;
    size: string;
  }>;
};

export type DebugRoundItem = {
  iteration: number;
  contextPreview: string;
  providerRequestJson: string;
  providerResponseJson: string;
  providerResponseRaw: string;
  toolCalls: Array<{
    id: string;
    name: string;
    argumentsJson: string;
  }>;
  toolResults: string[];
};

export type WorkspaceView = {
  title: string;
  tasks: TaskItem[];
  activeTaskId: string;
  selectedModel?: string;
  selectedTemperature?: number;
  selectedTopP?: number;
  selectedPresencePenalty?: number;
  selectedFrequencyPenalty?: number;
  selectedMaxOutputTokens?: number;
  workingDirectory?: string;
  timeline: TaskTimelineEntry[];
  notes: NoteItem[];
  openFiles: OpenFileItem[];
  hints: HintItem[];
  skills: SkillItem[];
  memories: MemoryItem[];
  memoryWarnings: string[];
  contextUsage: ContextUsage;
  debugRounds: DebugRoundItem[];
  workspacePath?: string;
  databasePath?: string;
};

export type BackendActiveTask = NonNullable<BackendWorkspaceSnapshot['active_task']>;
export type BackendTaskOpenFile = BackendActiveTask['open_files'][number];
export type BackendOpenFileSnapshot = NonNullable<BackendTaskOpenFile['snapshot']>;
export type BackendRuntimeSnapshot = NonNullable<BackendActiveTask['runtime']>;
export type BackendRuntimeOpenFile = BackendRuntimeSnapshot['open_files'][number];
export type WorkspaceContextData = Pick<
  WorkspaceView,
  'notes' | 'openFiles' | 'workingDirectory' | 'hints' | 'skills' | 'memories' | 'memoryWarnings' | 'contextUsage' | 'debugRounds'
>;

export type TaskActivityStatus = 'working' | 'review';

export type BackendWorkspaceSnapshot = {
  workspace_path?: string;
  database_path?: string;
  tasks: Array<{
    id: number;
    name: string;
    working_directory: string;
    last_active: number;
    selected_model?: string | null;
    model_temperature?: number | null;
    model_top_p?: number | null;
    model_presence_penalty?: number | null;
    model_frequency_penalty?: number | null;
    model_max_output_tokens?: number | null;
  }>;
  active_task?: {
    task: {
      id: number;
      name: string;
      working_directory: string;
      selected_model?: string | null;
      model_temperature?: number | null;
      model_top_p?: number | null;
      model_presence_penalty?: number | null;
      model_frequency_penalty?: number | null;
      model_max_output_tokens?: number | null;
    };
    active_agent?: string;
    last_seq: number;
    timeline: Array<
      | {
          kind: 'user_message';
          user_message_id: string;
          content: string;
          images: Array<{
            id: string;
            name: string;
            media_type: string;
            data_url: string;
            source_path?: string | null;
          }>;
          mentions: string[];
          replies: ReplyRef[];
          timestamp: number;
        }
      | {
          kind: 'turn';
          turn_id: string;
          agent_id: string;
          agent_display_name: string;
          trigger:
            | { kind: 'user'; id: string }
            | { kind: 'turn'; id: string };
          state: string;
          error_message?: string | null;
          timestamp: number;
          messages: Array<{
            message_id: string;
            turn_id: string;
            state: string;
            reasoning: string;
            timeline: Array<
              | {
                  kind: 'text';
                  text: string;
                }
              | {
                  kind: 'tool';
                  tool_call_id: string;
                  tool_name: string;
                  arguments: string;
                  status: string;
                  preview?: string | null;
                  duration_ms?: number | null;
                }
            >;
          }>;
        }
    >;
    notes: Array<{
      scope?: string;
      id: string;
      content: string;
    }>;
    open_files: Array<{
      scope?: string;
      path: string;
      locked: boolean;
      snapshot?: (
        | {
            Available: {
              path: string;
              content: string;
              last_modified_at: number;
            };
          }
        | {
            Deleted: {
              path: string;
              last_seen_at: number;
            };
          }
        | {
            Moved: {
              path: string;
              new_path: string;
              last_seen_at: number;
            };
          }
      ) | null;
    }>;
    hints: Array<{
      content: string;
      expires_at?: number | null;
      turns_remaining?: number | null;
    }>;
    runtime?: {
      working_directory: string;
      available_shells: Array<{
        kind: string;
        program: string;
      }>;
      open_files: Array<
        | {
            Available: {
              path: string;
              content: string;
              last_modified_at: number;
              modified_by: 'Agent' | 'User' | 'External' | 'Unknown';
            };
          }
        | {
            Deleted: {
              path: string;
              last_seen_at: number;
              modified_by: 'Agent' | 'User' | 'External' | 'Unknown';
            };
          }
        | {
            Moved: {
              path: string;
              new_path: string;
              last_seen_at: number;
              modified_by: 'Agent' | 'User' | 'External' | 'Unknown';
            };
          }
      >;
      skills: Array<{
        name: string;
        path: string;
        description: string;
        opened: boolean;
      }>;
      memories: Array<{
        id: string;
        memory_type: string;
        topic: string;
        title: string;
        level: string;
      }>;
      memory_warnings: string[];
      system_status: {
        locked_files: string[];
        context_pressure?: {
          used_percent: number;
          message: string;
        } | null;
      };
      context_usage: {
        used_percent: number;
        used_tokens: number;
        budget_tokens: number;
        sections: Array<{
          name: string;
          tokens: number;
        }>;
      };
    } | null;
    debug_trace?: {
      rounds: Array<{
        iteration: number;
        context_preview: string;
        provider_request_json: string;
        provider_response_json: string;
        provider_response_raw: string;
        tool_calls: Array<{
          id: string;
          name: string;
          arguments_json: string;
        }>;
        tool_results: string[];
      }>;
    } | null;
  } | null;
};

export type BackendAgentProgressEvent =
  | {
      kind: 'user_message_appended';
      task_id: number;
      seq: number;
      user_message_id: string;
      content: string;
      ts: number;
      mentions: string[];
      replies: ReplyRef[];
    }
  | {
      kind: 'turn_started';
      task_id: number;
      seq: number;
      turn_id: string;
      agent: string;
      agent_display_name: string;
      trigger:
        | { kind: 'user'; id: string }
        | { kind: 'turn'; id: string };
    }
  | {
      kind: 'message_started';
      task_id: number;
      seq: number;
      turn_id: string;
      message_id: string;
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'tool_started';
      task_id: number;
      seq: number;
      turn_id: string;
      message_id: string;
      tool_call_id: string;
      tool_name: string;
      summary: string;
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'tool_finished';
      task_id: number;
      seq: number;
      turn_id: string;
      message_id: string;
      tool_call_id: string;
      status: 'success' | 'error';
      summary: string;
      preview?: string | null;
      detail?: string | null;
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'assistant_stream_delta';
      task_id: number;
      seq: number;
      turn_id: string;
      message_id: string;
      field: 'reasoning' | 'content' | 'tool_call_arguments';
      delta: string;
      tool_call_id?: string | null;
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'message_finished';
      task_id: number;
      seq: number;
      turn_id: string;
      message_id: string;
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'turn_finished';
      task_id: number;
      seq: number;
      turn_id: string;
      reason: 'idle' | 'failed' | 'cancelled';
      error_message?: string | null;
      task: NonNullable<BackendWorkspaceSnapshot['active_task']>;
    }
  | {
      kind: 'round_complete';
      task_id: number;
      seq: number;
      turn_id: string;
      debug_round: BackendDebugRoundView;
      task: NonNullable<BackendWorkspaceSnapshot['active_task']>;
    };

export type WorkspaceEntryView = {
  path: string;
  kind: 'file' | 'directory';
};

export type MentionTargetView =
  | {
      kind: 'agent';
      name: string;
      displayName: string;
      description: string;
      avatarColor: string;
      source: string;
    }
  | {
      kind: 'file' | 'directory';
      path: string;
    };

export type SearchSkillView = {
  kind: 'skill';
  name: string;
  path: string;
  description: string;
  opened: boolean;
  autoTriggered: boolean;
  triggerReason?: string | null;
};

export type WorkspaceImageView = {
  path: string;
  mediaType: string;
  dataUrl: string;
  name: string;
};

export type ProviderModelsView = {
  current_model: string;
  available_models: string[];
  suggested_models: string[];
  provider_cache_key: string;
};

export type TaskModelSelectorView = {
  currentModelConfigId?: number | null;
  currentModel: string;
  currentTemperature?: number | null;
  currentTopP?: number | null;
  currentPresencePenalty?: number | null;
  currentFrequencyPenalty?: number | null;
  currentMaxOutputTokens?: number | null;
  currentModelCapabilities: {
    contextWindow: number;
    maxOutputTokens: number;
    supportsToolUse: boolean;
    supportsVision: boolean;
    supportsAudio: boolean;
    supportsPdf: boolean;
    serverTools: Array<{
      capability: string;
      format: string;
    }>;
  };
  models: Array<{
    modelConfigId: number;
    providerId: number;
    providerName: string;
    providerType: string;
    displayName: string;
    modelId: string;
  }>;
};

export type BackendTaskTimelineEntry = NonNullable<
  NonNullable<BackendWorkspaceSnapshot['active_task']>['timeline']
>[number];
export type BackendTaskHistoryView = {
  timeline: BackendTaskTimelineEntry[];
  last_seq: number;
};
export type BackendTaskSubscriptionView = {
  status: 'subscribed' | 'gap_too_large';
};
export type BackendDebugRoundView = NonNullable<
  NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['debug_trace']>['rounds']
>[number];

export type ProviderConnectionTestResult = {
  success: boolean;
  message: string;
  suggestedModel?: string | null;
};

export type ProviderModelCapabilityProbeResult = {
  contextWindow: number;
  maxOutputTokens: number;
  supportsToolUse: boolean;
  supportsVision: boolean;
  supportsAudio: boolean;
  supportsPdf: boolean;
  warnings: string[];
  source: string;
};

export type ProviderSettingsView = {
  databasePath: string;
  providers: Array<{
    id: number;
    name: string;
    providerType: string;
    baseUrl?: string | null;
    apiKey: string;
    apiKeyHint: string;
    createdAt: number;
    models: Array<{
      id: number;
      providerId: number;
      modelId: string;
      displayName?: string | null;
      probedAt?: number | null;
      capabilities: {
        contextWindow: number;
        maxOutputTokens: number;
        supportsToolUse: boolean;
        supportsVision: boolean;
        supportsAudio: boolean;
        supportsPdf: boolean;
        serverTools: Array<{
          capability: string;
          format: string;
        }>;
      };
    }>;
  }>;
  agents: Array<{
    id?: number | null;
    name: string;
    displayName: string;
    description: string;
    systemPrompt: string;
    avatarColor: string;
    providerId?: number | null;
    modelId?: string | null;
    isBuiltIn: boolean;
    source: string;
  }>;
  defaultModelConfigId?: number | null;
  defaultModel?: string | null;
};

export type BackendMemoryDetailView = {
  id: string;
  memory_type: string;
  topic: string;
  title: string;
  content: string;
  tags: string[];
  scope: string;
  level: string;
  access_count: number;
  skip_count: number;
  updated_at: number;
};

export const mockWorkspace: WorkspaceView = {
  title: '默认任务',
  tasks: [
    { id: 'task-1', name: '重构认证模块', status: 'active', updatedAt: '14:32' },
    { id: 'task-2', name: '添加支付集成', status: 'idle', updatedAt: '11:08' },
    { id: 'task-3', name: '修复登录 bug', status: 'idle', activityStatus: 'working', updatedAt: '09:41' },
  ] satisfies TaskItem[],
  activeTaskId: 'task-1',
  selectedModel: 'claude-sonnet-4-6',
  selectedTemperature: 0.2,
  selectedTopP: 1,
  selectedPresencePenalty: 0,
  selectedFrequencyPenalty: 0,
  workingDirectory: 'D:/playground/MA',
  timeline: [],
  notes: [
    { id: 'target', content: '当前目标：拆分 auth 模块' },
    { id: 'plan', content: '1. 读现有结构 2. 拆接口层 3. 补测试' },
  ] satisfies NoteItem[],
  openFiles: [
    { scope: 'shared', path: 'src/auth.rs', tokenUsage: '2.8k', freshness: 'high', locked: false },
    { scope: 'shared', path: 'src/lib.rs', tokenUsage: '1.9k', freshness: 'high', locked: false },
    { scope: 'shared', path: 'src/models.rs', tokenUsage: '0.9k', freshness: 'medium', locked: false },
    { scope: 'shared', path: 'config/prod.toml', tokenUsage: '0.3k', freshness: 'low', locked: true },
  ] satisfies OpenFileItem[],
  hints: [
    { source: 'Telegram', content: 'foo: 部署好了吗？', timeLeft: '4m32s', turnsLeft: '3轮' },
    { source: 'CI', content: 'main 构建失败 exit 1', timeLeft: '12m08s', turnsLeft: '1轮' },
  ] satisfies HintItem[],
  skills: [
    {
      name: 'rust',
      path: 'C:/Users/CPCli/.agent/skills/rust/SKILL.md',
      description: 'Rust 项目工作流',
      opened: true,
    },
    {
      name: 'api-style',
      path: 'D:/playground/MA/.march/skills/api-style/SKILL.md',
      description: '本项目 API 风格约定',
      opened: false,
    },
  ] satisfies SkillItem[],
  memories: [
    {
      id: 'p:auth-timeout-fix',
      type: 'pattern',
      topic: 'auth',
      title: '登录超时问题通常先看 token 续期链路',
      level: 'project',
    },
  ] satisfies MemoryItem[],
  memoryWarnings: [] as string[],
  contextUsage: {
    percent: 42,
    current: '10.2k',
    limit: '128k',
    sections: [
      { name: '文件', size: '6.1k' },
      { name: '笔记', size: '0.8k' },
      { name: '提示', size: '0.1k' },
      { name: '对话', size: '2.1k' },
      { name: '系统', size: '1.2k' },
    ],
  } satisfies ContextUsage,
  debugRounds: [
    {
      iteration: 1,
      contextPreview: '[open_files]\nsrc/auth.rs\n\n[recent_chat]\nUser: 帮我把 auth 模块拆成更小的单元。',
      providerRequestJson: '{\n  "model": "gpt-5",\n  "messages": [],\n  "tools": []\n}',
      providerResponseJson:
        '{\n  "choices": [\n    {\n      "message": {\n        "tool_calls": [\n          {\n            "id": "call_1",\n            "function": {\n              "name": "open_file",\n              "arguments": "{\\"path\\":\\"src/auth.rs\\"}"\n            }\n          }\n        ]\n      }\n    }\n  ]\n}',
      providerResponseRaw:
        '{\n  "choices": [\n    {\n      "message": {\n        "tool_calls": [\n          {\n            "id": "call_1",\n            "function": {\n              "name": "open_file",\n              "arguments": "{\\"path\\":\\"src/auth.rs\\"}"\n            }\n          }\n        ]\n      }\n    }\n  ]\n}',
      toolCalls: [
        {
          id: 'call_1',
          name: 'open_file',
          argumentsJson: '{"path":"src/auth.rs"}',
        },
      ],
      toolResults: ['opened D:/playground/MA/src/auth.rs'],
    },
  ] satisfies DebugRoundItem[],
};

export function createEmptyWorkspaceView(input?: {
  title?: string;
  workspacePath?: string;
  workingDirectory?: string;
  contextLimit?: string;
}): WorkspaceView {
  const workspacePath = input?.workspacePath;
  const workingDirectory = input?.workingDirectory ?? workspacePath;

  return {
    title: input?.title ?? 'March',
    tasks: [],
    activeTaskId: '',
    selectedModel: undefined,
    selectedTemperature: undefined,
    selectedTopP: undefined,
    selectedPresencePenalty: undefined,
    selectedFrequencyPenalty: undefined,
    selectedMaxOutputTokens: undefined,
    workingDirectory,
    timeline: [],
    notes: [],
    openFiles: [],
    hints: [],
    skills: [],
    memories: [],
    memoryWarnings: [],
    contextUsage: {
      percent: 0,
      current: '0',
      limit: input?.contextLimit ?? '128k',
      sections: [],
    },
    debugRounds: [],
    workspacePath,
    databasePath: undefined,
  };
}

export function toWorkspaceView(snapshot: unknown): WorkspaceView {
  const workspace = snapshot as BackendWorkspaceSnapshot;
  const activeTask = workspace.active_task;
  const activeTaskId = activeTask ? String(activeTask.task.id) : '';

  return {
    title: activeTask?.task.name ?? 'March',
    workspacePath: workspace.workspace_path,
    databasePath: workspace.database_path,
    tasks: workspace.tasks.map((task) => ({
      id: String(task.id),
      name: task.name,
      status: String(task.id) === activeTaskId ? 'active' : 'idle',
      updatedAt: formatRelativeTime(task.last_active),
    })),
    activeTaskId,
    selectedModel: activeTask?.task.selected_model ?? workspace.tasks.find((task) => task.id === Number(activeTaskId))?.selected_model ?? undefined,
    selectedTemperature:
      activeTask?.task.model_temperature
      ?? workspace.tasks.find((task) => task.id === Number(activeTaskId))?.model_temperature
      ?? undefined,
    selectedTopP:
      activeTask?.task.model_top_p
      ?? workspace.tasks.find((task) => task.id === Number(activeTaskId))?.model_top_p
      ?? undefined,
    selectedPresencePenalty:
      activeTask?.task.model_presence_penalty
      ?? workspace.tasks.find((task) => task.id === Number(activeTaskId))?.model_presence_penalty
      ?? undefined,
    selectedFrequencyPenalty:
      activeTask?.task.model_frequency_penalty
      ?? workspace.tasks.find((task) => task.id === Number(activeTaskId))?.model_frequency_penalty
      ?? undefined,
    selectedMaxOutputTokens:
      activeTask?.task.model_max_output_tokens
      ?? workspace.tasks.find((task) => task.id === Number(activeTaskId))?.model_max_output_tokens
      ?? undefined,
    timeline: activeTask ? toTaskTimelineEntries(activeTask.timeline) : [],
    ...toWorkspaceContextView(activeTask, activeTask?.task.working_directory ?? workspace.workspace_path),
  };
}

export function toWorkspaceContextView(
  activeTask?: BackendActiveTask | null,
  fallbackWorkingDirectory?: string,
): WorkspaceContextData {
  return {
    notes: activeTask?.notes ?? [],
    openFiles: buildOpenFileItems(activeTask),
    workingDirectory: activeTask?.runtime?.working_directory ?? activeTask?.task.working_directory ?? fallbackWorkingDirectory,
    hints: activeTask?.hints.map((hint, index) => ({
      source: `Hint ${index + 1}`,
      content: hint.content,
      timeLeft: formatHintTime(hint.expires_at),
      turnsLeft: hint.turns_remaining ? `${hint.turns_remaining}轮` : '∞',
    })) ?? [],
    skills: activeTask?.runtime?.skills.map((skill) => ({
      name: skill.name,
      path: normalizePath(skill.path),
      description: skill.description,
      opened: skill.opened,
    })) ?? [],
    memories: activeTask?.runtime?.memories.map((memory) => ({
      id: memory.id,
      type: memory.memory_type,
      topic: memory.topic,
      title: memory.title,
      level: memory.level === 'global' ? 'global' : 'project',
    })) ?? [],
    memoryWarnings: activeTask?.runtime?.memory_warnings ?? [],
    contextUsage: formatContextUsage(activeTask?.runtime?.context_usage),
    debugRounds: activeTask?.debug_trace?.rounds.map(toDebugRoundItem) ?? [],
  };
}

export function mergeTaskRuntimeSnapshot(
  activeTask: BackendActiveTask,
  runtime: BackendRuntimeSnapshot,
): BackendActiveTask {
  return {
    ...activeTask,
    runtime,
  };
}

export function toTaskTimelineEntries(entries: BackendTaskTimelineEntry[]): TaskTimelineEntry[] {
  return entries.map((entry) => {
    if (entry.kind === 'user_message') {
      return {
        kind: 'user_message',
        userMessageId: entry.user_message_id,
        content: entry.content,
        mentions: [...entry.mentions],
        replies: entry.replies.map((reply) => ({ ...reply })),
        ts: entry.timestamp * 1000,
        author: 'User',
        images: entry.images.map((image) => ({
          id: image.id,
          name: image.name,
          previewUrl: image.data_url,
          mediaType: image.media_type,
          sourcePath: image.source_path ?? undefined,
        })),
      } satisfies UserMessage;
    }

    return {
      kind: 'turn',
      turnId: entry.turn_id,
      agentId: entry.agent_id,
      agentName: entry.agent_display_name,
      trigger: { ...entry.trigger },
      state: toTurnState(entry.state),
      errorMessage: entry.error_message ?? undefined,
      ts: entry.timestamp * 1000,
      messages: entry.messages.map((message) => ({
        messageId: message.message_id,
        turnId: message.turn_id,
        state: toAssistantMessageState(message.state),
        reasoning: message.reasoning,
        timeline: message.timeline.map((timelineEntry) => {
          if (timelineEntry.kind === 'text') {
            return {
              kind: 'text',
              textId: crypto.randomUUID(),
              text: timelineEntry.text,
            } satisfies AssistantTimelineTextEntry;
          }

          return {
            kind: 'tool',
            toolCallId: timelineEntry.tool_call_id,
            toolName: timelineEntry.tool_name,
            arguments: timelineEntry.arguments,
            status: toToolCallState(timelineEntry.status),
            preview: timelineEntry.preview ?? undefined,
            durationMs: timelineEntry.duration_ms ?? undefined,
          } satisfies AssistantTimelineToolEntry;
        }),
      })),
    } satisfies Turn;
  });
}

export function toDebugRoundItem(round: BackendDebugRoundView): DebugRoundItem {
  return {
    iteration: round.iteration,
    contextPreview: round.context_preview,
    providerRequestJson: round.provider_request_json,
    providerResponseJson: round.provider_response_json,
    providerResponseRaw: round.provider_response_raw,
    toolCalls: round.tool_calls.map((toolCall) => ({
      id: toolCall.id,
      name: toolCall.name,
      argumentsJson: toolCall.arguments_json,
    })),
    toolResults: round.tool_results,
  };
}

function toTurnState(state: string): Turn['state'] {
  switch (state) {
    case 'streaming':
      return 'streaming';
    case 'failed':
      return 'failed';
    case 'cancelled':
      return 'cancelled';
    default:
      return 'done';
  }
}

function toAssistantMessageState(state: string): AssistantMessage['state'] {
  return state === 'streaming' ? 'streaming' : 'done';
}

function toToolCallState(state: string): AssistantTimelineToolEntry['status'] {
  switch (state) {
    case 'running':
      return 'running';
    case 'error':
      return 'error';
    default:
      return 'ok';
  }
}

function formatRelativeTime(timestamp: number) {
  const nowSeconds = Math.floor(Date.now() / 1000);
  const diffSeconds = Math.max(0, nowSeconds - timestamp);
  const minutes = Math.floor(diffSeconds / 60);

  if (minutes < 1) {
    return '刚刚';
  }
  if (minutes < 60) {
    return `${minutes} 分`;
  }

  const hours = Math.floor(minutes / 60);
  if (hours < 24) {
    return `${hours} 小时`;
  }

  const days = Math.floor(hours / 24);
  return `${days} 天`;
}

function normalizePath(path: string) {
  const normalized = path.replaceAll('\\', '/');
  if (normalized.startsWith('//?/UNC/')) {
    return `//${normalized.slice('//?/UNC/'.length)}`;
  }
  if (normalized.startsWith('//?/')) {
    return normalized.slice('//?/'.length);
  }
  return normalized;
}

function buildOpenFileItems(activeTask?: BackendActiveTask | null): OpenFileItem[] {
  if (!activeTask) {
    return [];
  }

  const runtimeByPath = new Map(
    (activeTask.runtime?.open_files ?? []).map((entry) => [normalizePath(runtimeOpenFilePath(entry)), entry] as const),
  );

  const items = activeTask.open_files.map((file) => {
    const normalizedPath = normalizePath(file.path);
    const runtimeEntry = runtimeByPath.get(normalizedPath);

    return {
      scope: file.scope ?? 'shared',
      path: normalizedPath,
      tokenUsage: formatOpenFileTokenUsage(file.snapshot, runtimeEntry),
      freshness: resolveOpenFileFreshness(file.locked, file.snapshot, runtimeEntry),
      locked: file.locked,
      state: mapOpenFileState(file.snapshot, runtimeEntry),
    };
  });

  const orphanRuntimeEntries = (activeTask.runtime?.open_files ?? []).filter((entry) => {
    const normalizedPath = normalizePath(runtimeOpenFilePath(entry));
    return !activeTask.open_files.some((file) => normalizePath(file.path) === normalizedPath);
  });

  for (const runtimeEntry of orphanRuntimeEntries) {
    items.push({
      scope: 'shared',
      path: normalizePath(runtimeOpenFilePath(runtimeEntry)),
      tokenUsage: formatOpenFileTokenUsage(undefined, runtimeEntry),
      freshness: resolveOpenFileFreshness(false, undefined, runtimeEntry),
      locked: false,
      state: mapOpenFileState(undefined, runtimeEntry),
    });
  }

  return items;
}

function runtimeOpenFilePath(entry: BackendRuntimeOpenFile) {
  if ('Available' in entry) {
    return entry.Available.path;
  }
  if ('Deleted' in entry) {
    return entry.Deleted.path;
  }
  if ('Moved' in entry) {
    return entry.Moved.path;
  }
  return assertNever(entry);
}

function formatOpenFileTokenUsage(snapshot?: BackendOpenFileSnapshot | null, runtimeEntry?: BackendRuntimeOpenFile) {
  if (runtimeEntry && 'Available' in runtimeEntry) {
    return formatTokenCount(estimateTokenCount(runtimeEntry.Available.content));
  }

  if (!snapshot) {
    return '0';
  }

  if ('Available' in snapshot) {
    return formatTokenCount(estimateTokenCount(snapshot.Available.content));
  }

  // Deleted / moved entries在上下文里仍有少量状态成本。
  return formatTokenCount(8);
}

function formatHintTime(expiresAt?: number | null) {
  if (!expiresAt) {
    return 'no ttl';
  }

  const seconds = Math.max(0, expiresAt - Math.floor(Date.now() / 1000));
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds % 60;
  return `${minutes}m${String(remainder).padStart(2, '0')}s`;
}

function mapOpenFileState(snapshot?: BackendOpenFileSnapshot | null, runtimeEntry?: BackendRuntimeOpenFile) {
  if (runtimeEntry) {
    if ('Moved' in runtimeEntry) {
      return {
        kind: 'moved' as const,
        newPath: normalizePath(runtimeEntry.Moved.new_path),
      };
    }

    if ('Deleted' in runtimeEntry) {
      return { kind: 'deleted' as const };
    }
  }

  if (!snapshot || 'Available' in snapshot) {
    return { kind: 'available' as const };
  }

  if ('Moved' in snapshot) {
    return {
      kind: 'moved' as const,
      newPath: normalizePath(snapshot.Moved.new_path),
    };
  }

  return { kind: 'deleted' as const };
}

function formatContextUsage(
  usage?: BackendWorkspaceSnapshot['active_task'] extends infer T
    ? T extends { runtime?: { context_usage: infer U } | null }
      ? U
      : never
    : never,
): ContextUsage {
  if (!usage) {
    return {
      percent: 0,
      current: '0',
      limit: '128k',
      sections: [],
    };
  }

  return {
    percent: usage.used_percent,
    current: formatTokenCount(usage.used_tokens),
    limit: formatTokenCount(usage.budget_tokens),
    sections: usage.sections.map((section) => ({
      name: section.name,
      size: formatTokenCount(section.tokens),
    })),
  };
}

function formatTokenCount(tokens: number) {
  if (tokens >= 1000) {
    return `${(tokens / 1000).toFixed(1)}k`;
  }
  return `${tokens}`;
}

function estimateTokenCount(text: string) {
  let asciiChars = 0;
  let nonAsciiChars = 0;

  for (const char of text) {
    if (char.charCodeAt(0) <= 0x7f) {
      asciiChars += 1;
    } else {
      nonAsciiChars += 1;
    }
  }

  return Math.ceil(asciiChars / 4) + nonAsciiChars;
}

function resolveOpenFileFreshness(
  locked: boolean,
  snapshot?: BackendOpenFileSnapshot | null,
  runtimeEntry?: BackendRuntimeOpenFile,
): OpenFileItem['freshness'] {
  if (locked) {
    return 'low';
  }
  if (runtimeEntry) {
    return 'high';
  }
  if (snapshot) {
    return 'high';
  }
  return 'medium';
}

function assertNever(value: never): never {
  throw new Error(`Unhandled open file variant: ${JSON.stringify(value)}`);
}
