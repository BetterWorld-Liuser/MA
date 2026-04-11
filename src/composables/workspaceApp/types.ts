import type { Ref } from 'vue';
import type {
  BackendActiveTask,
  BackendRuntimeSnapshot,
  BackendWorkspaceSnapshot,
  ChatImageAttachment,
  ContextUsage,
  DebugRoundItem,
  HintItem,
  MemoryItem,
  NoteItem,
  OpenFileItem,
  ReplyRef,
  SkillItem,
  TaskTimelineEntry,
  TaskItem,
  WorkspaceView,
} from '@/data/mock';

export type ChatPaneHandle = { focusComposer: () => void };

export type NoteEditorDialogHandle = {
  focusIdField: () => void;
  focusContentField: () => void;
};

export type MemoryEditorDialogHandle = {
  focusIdField: () => void;
  focusContentField: () => void;
};

export type ComposerPayload = {
  content: string;
  mentions: string[];
  replies: ReplyRef[];
  directories: string[];
  files: string[];
  skills: string[];
  images: ChatImageAttachment[];
};

export type ComposerReplyPreview = {
  kind: ReplyRef['kind'];
  id: string;
  author: string;
  summary: string;
};

export type RunWorkspaceAction = (action: () => Promise<void>) => Promise<boolean>;

export type WorkspaceTaskListView = {
  tasks: TaskItem[];
  activeTaskId: string;
};

export type WorkspaceChatView = {
  timeline: TaskTimelineEntry[];
};

export type WorkspaceComposerView = {
  selectedModel?: string;
  selectedTemperature?: number;
  selectedTopP?: number;
  selectedPresencePenalty?: number;
  selectedFrequencyPenalty?: number;
  selectedMaxOutputTokens?: number;
  workingDirectory?: string;
  workspacePath?: string;
};

export type WorkspaceContextView = {
  notes: NoteItem[];
  openFiles: OpenFileItem[];
  workingDirectory?: string;
  hints: HintItem[];
  skills: SkillItem[];
  memories: MemoryItem[];
  memoryWarnings: string[];
  contextUsage: ContextUsage;
  debugRounds: DebugRoundItem[];
};

export type WorkspaceSnapshotState = {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  workspacePath: Readonly<Ref<string | undefined>>;
  workspace: Readonly<Ref<WorkspaceView>>;
  resolvedWorkspace: Readonly<Ref<WorkspaceView>>;
  taskListView: Readonly<Ref<WorkspaceTaskListView>>;
  composerView: Readonly<Ref<WorkspaceComposerView>>;
  contextView: Readonly<Ref<WorkspaceContextView>>;
  optimisticTaskId: Ref<string | null>;
  optimisticActiveTaskId: Ref<string | null>;
  optimisticDeletedTaskIds: Ref<Set<string>>;
  activeTaskIdNumber: Readonly<Ref<number | null>>;
  setTaskRuntimeSnapshot: (
    taskId: number,
    runtime: BackendRuntimeSnapshot,
  ) => void;
  hydrateTaskDebugTrace: (taskId: number, rounds: DebugRoundItem[]) => void;
  appendTaskDebugRound: (taskId: number, round: DebugRoundItem) => void;
  clearDeletedTaskOptimism: (taskId: number) => void;
  syncTaskContextSnapshot: (taskId: number, task: BackendActiveTask) => void;
};

export type TaskChatState = {
  chatView: Readonly<Ref<WorkspaceChatView>>;
  hydrateTaskTimeline: (taskId: number, timeline: TaskTimelineEntry[]) => void;
  optimisticAppendUserMessage: (
    taskId: number,
    input: {
      id: string;
      content: string;
      ts?: number;
      mentions?: string[];
      replies?: ReplyRef[];
      images?: ChatImageAttachment[];
    },
  ) => void;
  clearTaskTimeline: (taskId: number) => void;
  markTaskTimelineNeedsHydration: (taskId: number) => void;
  loadTaskHistory: (taskId: number) => Promise<void>;
  getTaskLastSeq: (taskId: number) => number;
};

export function humanizeError(error: unknown) {
  if (typeof error === 'string') {
    return error;
  }
  if (error && typeof error === 'object' && 'message' in error && typeof error.message === 'string') {
    return error.message;
  }
  return 'Unknown error while talking to the March backend.';
}

export function augmentComposerMessage(payload: ComposerPayload) {
  const base = payload.content.trim();
  const sections = [base];
  if (payload.directories.length) {
    sections.push(`[目录引用]\n${payload.directories.map((path) => `- ${path}`).join('\n')}`);
  }
  return sections.filter(Boolean).join('\n\n');
}

export function extractBase64Payload(dataUrl: string) {
  const separatorIndex = dataUrl.indexOf(',');
  return separatorIndex >= 0 ? dataUrl.slice(separatorIndex + 1) : dataUrl;
}

export function extractAgentMentionsFromComposerText(content: string) {
  const matches = content.matchAll(/(^|\s)@([^\s@]+)/g);
  const mentions = new Set<string>();

  for (const match of matches) {
    const rawName = match[2]?.trim().toLowerCase();
    if (!rawName) {
      continue;
    }

    const normalizedName = rawName.replace(/[,:，：。!！?？]+$/g, '');
    if (normalizedName) {
      mentions.add(normalizedName);
    }
  }

  return [...mentions];
}
