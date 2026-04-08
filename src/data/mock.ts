export type TaskItem = {
  id: string;
  name: string;
  status: 'active' | 'idle';
  activityStatus?: 'working' | 'review';
  updatedAt: string;
};

export type ChatTool = {
  label: string;
  summary: string;
};

export type ChatImageAttachment = {
  id: string;
  name: string;
  previewUrl: string;
  mediaType: string;
  sourcePath?: string;
};

export type LiveToolItem = {
  id: string;
  label: string;
  summary: string;
  state: 'running' | 'success' | 'error';
  preview?: string;
};

export type LiveTurn = {
  turnId: string;
  author: string;
  state: 'pending' | 'running' | 'streaming' | 'error';
  statusLabel: string;
  content: string;
  errorMessage?: string;
  tools: LiveToolItem[];
  transitionKey?: number;
};

export type ChatMessage = {
  id?: string;
  role: 'user' | 'assistant';
  author: string;
  time: string;
  timestamp?: number;
  content: string;
  images?: ChatImageAttachment[];
  tools?: ChatTool[];
  variant?: 'default' | 'intermediate' | 'failed';
};

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
  chat: ChatMessage[];
  notes: NoteItem[];
  openFiles: OpenFileItem[];
  hints: HintItem[];
  skills: SkillItem[];
  memories: MemoryItem[];
  memoryWarnings: string[];
  contextUsage: ContextUsage;
  debugRounds: DebugRoundItem[];
  liveTurn?: LiveTurn;
  workspacePath?: string;
  databasePath?: string;
};

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
    history: Array<{
      role: 'System' | 'User' | 'Assistant' | 'Tool';
      agent?: string;
      agent_display_name?: string;
      content: string;
      images?: Array<{
        id: string;
        name: string;
        mediaType: string;
        dataUrl: string;
        sourcePath?: string | null;
      }>;
      timestamp: number;
      tool_summaries: Array<{
        name: string;
        summary: string;
      }>;
    }>;
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
      kind: 'turn_started';
      task_id: number;
      turn_id: string;
      user_message: string;
      agent: string;
      agent_display_name: string;
    }
  | {
      kind: 'status';
      task_id: number;
      turn_id: string;
      agent: string;
      agent_display_name: string;
      phase: 'building_context' | 'waiting_model' | 'running_tool' | 'streaming';
      label: string;
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'tool_started';
      task_id: number;
      turn_id: string;
      tool_call_id: string;
      tool_name: string;
      summary: string;
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'tool_finished';
      task_id: number;
      turn_id: string;
      tool_call_id: string;
      status: 'success' | 'error';
      summary: string;
      preview?: string | null;
      detail?: string | null;
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'assistant_text_preview';
      task_id: number;
      turn_id: string;
      agent: string;
      agent_display_name: string;
      message: string;
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'assistant_message_checkpoint';
      task_id: number;
      turn_id: string;
      agent: string;
      agent_display_name: string;
      message_id: string;
      content: string;
      checkpoint_type: 'intermediate' | 'final';
      runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>;
    }
  | {
      kind: 'final_assistant_message';
      task_id: number;
      turn_id: string;
      assistant_message: BackendHistoryTurn;
      task: NonNullable<BackendWorkspaceSnapshot['active_task']>;
    }
  | {
      kind: 'round_complete';
      task_id: number;
      turn_id: string;
      debug_round: BackendDebugRoundView;
      task: NonNullable<BackendWorkspaceSnapshot['active_task']>;
    }
  | {
      kind: 'turn_failed';
      task_id: number;
      turn_id: string;
      stage: 'context' | 'tool' | 'provider' | 'internal';
      message: string;
      retryable: boolean;
    }
  | {
      kind: 'turn_cancelled';
      task_id: number;
      turn_id: string;
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

export type BackendHistoryTurn = NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['history']>[number];
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
  chat: [
    {
      role: 'user',
      author: 'User',
      time: '14:32',
      content: '帮我把 auth 模块拆成更小的单元。',
    },
    {
      role: 'assistant',
      author: 'March',
      time: '14:32',
      content: '好的，我先看一下现有结构，然后把依赖边界切开。',
      tools: [
        { label: 'open_file', summary: 'src/auth.rs' },
        { label: 'replace_lines', summary: '12-30' },
        { label: 'reply', summary: '发送了用户可见消息' },
      ],
    },
    {
      role: 'assistant',
      author: 'March',
      time: '14:33',
      content: '已完成，auth 模块现在拆成了三个文件，接口层更清晰了。',
    },
  ] satisfies ChatMessage[],
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
      contextPreview: '# Open Files\nsrc/auth.rs\n\n# Recent Chat\nUser: 帮我把 auth 模块拆成更小的单元。',
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
    chat: [],
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
    liveTurn: undefined,
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
    workingDirectory:
      activeTask?.task.working_directory
      ?? workspace.tasks.find((task) => task.id === Number(activeTaskId))?.working_directory
      ?? workspace.workspace_path,
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
    chat: activeTask ? toChatMessages(activeTask.history) : [],
    notes: activeTask?.notes ?? [],
    openFiles: activeTask?.open_files.map((file) => ({
      scope: file.scope ?? 'shared',
      path: normalizePath(file.path),
      tokenUsage: formatOpenFileTokenUsage(file.snapshot),
      freshness: file.locked ? 'low' : file.snapshot ? 'high' : 'medium',
      locked: file.locked,
      state: mapOpenFileState(file.snapshot),
    })) ?? [],
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

export function toChatMessages(history: BackendHistoryTurn[]): ChatMessage[] {
  return history.map(toChatMessage);
}

export function toChatMessage(turn: BackendHistoryTurn): ChatMessage {
  return {
    id: buildHistoryMessageId(turn),
    role: turn.role === 'User' ? 'user' : 'assistant',
    author: turn.role === 'User' ? 'User' : (turn.agent_display_name || turn.agent || 'March'),
    time: formatTime(turn.timestamp),
    timestamp: turn.timestamp * 1000,
    content: turn.content,
    images: turn.images?.map((image) => ({
      id: image.id,
      name: image.name,
      previewUrl: image.dataUrl,
      mediaType: image.mediaType,
      sourcePath: image.sourcePath ?? undefined,
    })),
    tools: turn.tool_summaries.map((tool) => ({
      label: tool.name,
      summary: tool.summary,
    })),
  };
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

function formatTime(timestamp: number) {
  return new Date(timestamp * 1000).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

function buildHistoryMessageId(turn: BackendHistoryTurn) {
  return [
    'history',
    turn.role,
    turn.timestamp,
    turn.agent_display_name || turn.agent || '',
    hashString(turn.content),
  ].join(':');
}

function hashString(value: string) {
  let hash = 0;
  for (let index = 0; index < value.length; index += 1) {
    hash = (hash * 31 + value.charCodeAt(index)) >>> 0;
  }
  return hash.toString(16);
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

function formatOpenFileTokenUsage(snapshot: BackendWorkspaceSnapshot['active_task'] extends infer T
  ? T extends { open_files: Array<infer OpenFile> }
    ? OpenFile extends { snapshot?: infer Snapshot }
      ? Snapshot | undefined
      : never
    : never
  : never) {
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

function mapOpenFileState(snapshot: BackendWorkspaceSnapshot['active_task'] extends infer T
  ? T extends { open_files: Array<infer OpenFile> }
    ? OpenFile extends { snapshot?: infer Snapshot }
      ? Snapshot | undefined
      : never
    : never
  : never) {
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
