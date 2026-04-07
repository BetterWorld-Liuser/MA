import type { Ref } from 'vue';
import type {
  BackendWorkspaceSnapshot,
  ChatImageAttachment,
  ChatMessage,
  WorkspaceView,
} from '@/data/mock';

export type ChatPaneHandle = { focusComposer: () => void };

export type NoteEditorDialogHandle = {
  focusIdField: () => void;
  focusContentField: () => void;
};

export type ComposerPayload = {
  content: string;
  directories: string[];
  files: string[];
  skills: string[];
  images: ChatImageAttachment[];
};

export type RunWorkspaceAction = (action: () => Promise<void>) => Promise<boolean>;

export type WorkspaceSnapshotState = {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  workspacePath: Readonly<Ref<string | undefined>>;
  workspace: Readonly<Ref<WorkspaceView>>;
  resolvedWorkspace: Readonly<Ref<WorkspaceView>>;
  optimisticTaskId: Ref<string | null>;
  optimisticActiveTaskId: Ref<string | null>;
  optimisticDeletedTaskIds: Ref<Set<string>>;
  localComposerMessages: Ref<Record<number, ChatMessage[]>>;
  activeTaskIdNumber: Readonly<Ref<number | null>>;
  queueLocalComposerMessage: (taskId: number, message: ChatMessage) => void;
  clearLocalComposerMessages: (taskId: number) => void;
  clearTaskComposerState: (taskId: number) => void;
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
