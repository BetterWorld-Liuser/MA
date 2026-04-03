<template>
  <div class="h-screen overflow-hidden bg-bg text-text">
    <header data-tauri-drag-region class="flex h-10 items-center border-b border-border bg-bg-secondary/95 backdrop-blur-sm select-none">
      <div data-tauri-drag-region class="flex min-w-0 items-center gap-3 px-4">
        <span class="inline-flex h-5 w-5 items-center justify-center rounded-sm border border-accent/50 bg-accent-dim text-[11px] font-semibold text-accent">
          M
        </span>
        <div data-tauri-drag-region class="min-w-0">
          <p class="truncate text-[13px] font-medium text-text">{{ currentTaskTitle }}</p>
        </div>
      </div>
      <div data-tauri-drag-region class="min-w-0 flex-1"></div>
      <div class="flex shrink-0 items-center">
        <button class="titlebar-button" type="button" aria-label="Minimize" data-no-drag @click="minimizeWindow">
          <Icon :icon="minusIcon" class="titlebar-icon" />
        </button>
        <button class="titlebar-button" type="button" aria-label="Maximize" data-no-drag @click="toggleMaximize">
          <Icon :icon="isMaximized ? copyIcon : squareIcon" class="titlebar-icon" />
        </button>
        <button class="titlebar-button titlebar-button-close" type="button" aria-label="Close" data-no-drag @click="closeWindow">
          <Icon :icon="xIcon" class="titlebar-icon titlebar-icon-close" />
        </button>
      </div>
    </header>

    <div class="mx-auto flex h-[calc(100%-2.5rem)] min-h-0 max-w-[1920px] flex-col gap-2 px-2 py-2 lg:px-3 lg:py-3">

      <div v-if="errorMessage" class="bg-[rgba(224,82,82,0.06)] px-4 py-3 text-sm text-text" style="border-bottom: 1px solid rgba(224, 82, 82, 0.28)">
        {{ errorMessage }}
      </div>

      <main class="grid min-h-0 flex-1 overflow-hidden gap-0 lg:grid-cols-[256px_minmax(0,1fr)_332px]">
        <TaskList
          :title="appTitle"
          :tasks="workspace.tasks"
          :active-task-id="workspace.activeTaskId"
          :busy="busy"
          @select="selectTask"
          @create="createTask"
          @delete="deleteTask"
        />
        <ChatPane
          ref="chatPaneRef"
          :chat="workspace.chat"
          :live-turn="workspace.liveTurn"
          :task-id="activeTaskIdNumber"
          :selected-model="workspace.selectedModel"
          :disabled="!activeTaskIdNumber"
          :sending="hasPendingSend"
          @send="sendMessage"
          @open-files="openFilesFromComposer"
          @set-model="setTaskModel"
        />
        <ContextPanel
          :notes="workspace.notes"
          :open-files="workspace.openFiles"
          :hints="workspace.hints"
          :usage="workspace.contextUsage"
          :debug-rounds="workspace.debugRounds"
          :busy="busy"
          @add-note="addNote"
          @edit-note="editNote"
          @delete-note="deleteNote"
          @toggle-file-lock="toggleOpenFileLock"
          @close-file="closeOpenFile"
        />
      </main>
    </div>

    <Dialog :open="noteDialogOpen" @update:open="handleNoteDialogOpenChange">
      <DialogContent class="overflow-hidden bg-[linear-gradient(180deg,rgba(255,255,255,0.035),rgba(255,255,255,0.015)),rgba(10,10,10,0.94)]">
        <form class="contents" @submit.prevent="submitNoteDialog">
          <DialogHeader class="gap-0 px-5 pb-3 pt-5 text-left">
            <DialogTitle class="text-[18px] font-semibold tracking-[-0.01em] text-text">
              {{ noteDialogMode === 'edit' ? `编辑 Note · ${noteDraftId}` : '新增 Note' }}
            </DialogTitle>
            <DialogDescription class="mt-1 text-[12px] leading-5 text-text-muted">
              Notes 会直接进入 AI 下一轮上下文，适合保留目标、约束和临时决策。
            </DialogDescription>
          </DialogHeader>
          <div class="space-y-4 px-5 pb-4">
            <div class="dialog-field">
              <label class="dialog-label" for="note-id">Note id</label>
              <Input
                id="note-id"
                ref="noteIdInputRef"
                v-model="noteDraftId"
                class="font-mono"
                maxlength="40"
                placeholder="target"
                :disabled="noteDialogMode === 'edit'"
              />
            </div>
            <div class="dialog-field">
              <label class="dialog-label" for="note-content">Content</label>
              <Textarea
                id="note-content"
                ref="noteContentInputRef"
                v-model="noteDraftContent"
                placeholder="写下这轮之后仍然重要的信息。"
              />
            </div>
          </div>
          <DialogFooter class="border-t border-white/8 px-5 py-4 sm:justify-end">
            <Button type="button" variant="outline" :disabled="busy" @click="closeNoteDialog">取消</Button>
            <Button type="submit" :disabled="busy">{{ noteDialogMode === 'edit' ? '保存修改' : '添加 Note' }}</Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>

    <AlertDialog :open="confirmDialogOpen" @update:open="handleConfirmDialogOpenChange">
      <AlertDialogContent class="overflow-hidden bg-[linear-gradient(180deg,rgba(255,255,255,0.035),rgba(255,255,255,0.015)),rgba(10,10,10,0.94)]">
        <AlertDialogHeader class="gap-0 px-5 pb-3 pt-5 text-left">
          <AlertDialogTitle class="text-[18px] font-semibold tracking-[-0.01em] text-text">
            {{ confirmDialogTitle }}
          </AlertDialogTitle>
          <AlertDialogDescription class="mt-1 text-[12px] leading-5 text-text-muted">
            {{ confirmDialogDescription }}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div class="px-5 pb-4">
          <p class="text-[13px] leading-6 text-text">
            {{ confirmDialogBody }}
          </p>
        </div>
        <AlertDialogFooter class="border-t border-white/8 px-5 py-4 sm:justify-end">
          <AlertDialogCancel :disabled="busy">取消</AlertDialogCancel>
          <AlertDialogAction class="!bg-[rgba(224,82,82,0.16)] !border-[rgba(224,82,82,0.25)] !text-[#ffb2b2] hover:!bg-[rgba(224,82,82,0.24)]" :disabled="busy" @click="submitConfirmDialog">
            {{ confirmDialogLabel }}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
</template>

<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue';
import { Icon } from '@iconify/vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import minusIcon from '@iconify-icons/lucide/minus';
import copyIcon from '@iconify-icons/lucide/copy';
import squareIcon from '@iconify-icons/lucide/square';
import xIcon from '@iconify-icons/lucide/x';
import { AlertDialog, AlertDialogAction, AlertDialogCancel, AlertDialogContent, AlertDialogDescription, AlertDialogFooter, AlertDialogHeader, AlertDialogTitle } from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Textarea } from '@/components/ui/textarea';
import ChatPane from './components/ChatPane.vue';
import ContextPanel from './components/ContextPanel.vue';
import TaskList from './components/TaskList.vue';
import {
  mockWorkspace,
  toWorkspaceView,
  type BackendAgentProgressEvent,
  type BackendWorkspaceSnapshot,
  type LiveTurn,
  type WorkspaceView,
} from './data/mock';

const snapshot = ref<BackendWorkspaceSnapshot | null>(null);
const busy = ref(false);
const sendingTaskId = ref<number | null>(null);
const errorMessage = ref('');
const isMaximized = ref(false);
const appWindow = getCurrentWindow();
let unlistenAgentProgress: UnlistenFn | null = null;
const liveTurns = ref<Record<number, LiveTurn>>({});
type FocusableField = { focus: () => void; select: () => void };
type ChatPaneHandle = { focusComposer: () => void };
const chatPaneRef = ref<ChatPaneHandle | null>(null);
const noteDialogOpen = ref(false);
const noteDialogMode = ref<'create' | 'edit'>('create');
const noteDraftId = ref('');
const noteDraftContent = ref('');
const noteIdInputRef = ref<FocusableField | null>(null);
const noteContentInputRef = ref<FocusableField | null>(null);
const confirmDialogOpen = ref(false);
const confirmDialogTitle = ref('');
const confirmDialogDescription = ref('');
const confirmDialogBody = ref('');
const confirmDialogLabel = ref('删除');
const confirmDialogAction = ref<(() => Promise<void>) | null>(null);

const workspace = computed<WorkspaceView>(() => {
  const activeTaskId =
    snapshot.value?.active_task?.task.id ??
    (snapshot.value?.tasks[0]?.id ?? null);

  if (snapshot.value) {
    return {
      ...toWorkspaceView(snapshot.value),
      liveTurn: activeTaskId ? liveTurns.value[activeTaskId] : undefined,
    };
  }
  return mockWorkspace;
});

const activeTaskIdNumber = computed(() => {
  const raw = workspace.value.activeTaskId;
  return raw ? Number(raw) : null;
});

const appTitle = 'March';

const currentTaskTitle = computed(() => workspace.value.title || appTitle);
const hasPendingSend = computed(() => sendingTaskId.value !== null);

onMounted(async () => {
  isMaximized.value = await appWindow.isMaximized();
  unlistenAgentProgress = await listen<BackendAgentProgressEvent>('ma://agent-progress', (event) => {
    applyAgentProgress(event.payload);
  });
  await refreshWorkspace();
  await appWindow.onResized(async () => {
    isMaximized.value = await appWindow.isMaximized();
  });
});

onUnmounted(() => {
  if (unlistenAgentProgress) {
    unlistenAgentProgress();
    unlistenAgentProgress = null;
  }
});

async function refreshWorkspace(activeTaskId?: number | null) {
  await runWorkspaceAction(async () => {
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('load_workspace_snapshot', {
      activeTaskId: activeTaskId ?? undefined,
    });
  });
}

async function createTask() {
  if (busy.value) {
    return;
  }

  await runWorkspaceAction(async () => {
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('create_task', {
      input: {},
    });
  });
  await nextTick();
  chatPaneRef.value?.focusComposer();
}

async function selectTask(taskId: string) {
  if (!taskId || busy.value) {
    return;
  }

  await runWorkspaceAction(async () => {
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('select_task', {
      input: { taskId: Number(taskId) },
    });
  });
}

async function deleteTask(taskId: string) {
  if (!taskId || busy.value) {
    return;
  }

  const task = workspace.value.tasks.find((item) => item.id === taskId);
  openConfirmDialog({
    title: '删除任务',
    description: '删除后，这个主题窗口及其聊天记录会从当前工作区移除。',
    body: `确认删除「${task?.name ?? taskId}」吗？这个操作目前不能撤销。`,
    confirmLabel: '删除任务',
    action: async () => {
      await runWorkspaceAction(async () => {
        snapshot.value = await invoke<BackendWorkspaceSnapshot>('delete_task', {
          input: { taskId: Number(taskId) },
        });
      });
      clearLiveTurn(Number(taskId));
      if (sendingTaskId.value === Number(taskId)) {
        sendingTaskId.value = null;
      }
      closeConfirmDialog();
    },
  });
}

async function sendMessage(payload: { content: string; directories: string[] }) {
  if (!activeTaskIdNumber.value || sendingTaskId.value !== null) {
    return;
  }

  const taskId = activeTaskIdNumber.value;
  const content = augmentComposerMessage(payload);

  appendOptimisticUserMessage(content);
  upsertLiveTurn(taskId, {
    turnId: `pending-${Date.now()}`,
    state: 'pending',
    statusLabel: '已发送，正在准备',
    content: '',
    tools: [],
  });
  sendingTaskId.value = taskId;
  try {
    const nextSnapshot = await invoke<BackendWorkspaceSnapshot>('send_message', {
      input: {
        taskId,
        content,
      },
    });
    clearLiveTurn(taskId);
    if (snapshot.value?.active_task?.task.id === taskId) {
      snapshot.value = nextSnapshot;
    } else if (snapshot.value) {
      snapshot.value = {
        ...snapshot.value,
        tasks: nextSnapshot.tasks,
      };
    }
    errorMessage.value = '';
  } catch (error) {
    const currentLiveTurn = liveTurns.value[taskId];
    if (currentLiveTurn) {
      upsertLiveTurn(taskId, {
        ...currentLiveTurn,
        state: 'error',
        statusLabel: '本轮执行失败',
      });
    }
    errorMessage.value = humanizeError(error);
  } finally {
    if (sendingTaskId.value === taskId) {
      sendingTaskId.value = null;
    }
  }
}

function augmentComposerMessage(payload: { content: string; directories: string[] }) {
  const base = payload.content.trim();
  if (!payload.directories.length) {
    return base;
  }

  return `${base}\n\n[目录引用]\n${payload.directories.map((path) => `- ${path}`).join('\n')}`;
}

function appendOptimisticUserMessage(content: string) {
  if (!snapshot.value?.active_task) {
    return;
  }

  snapshot.value = {
    ...snapshot.value,
    active_task: {
      ...snapshot.value.active_task,
      history: [
        ...snapshot.value.active_task.history,
        {
          role: 'User',
          content,
          timestamp: Math.floor(Date.now() / 1000),
          tool_summaries: [],
        },
      ],
    },
  };
}

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

async function addNote() {
  if (!activeTaskIdNumber.value || busy.value) {
    return;
  }

  noteDialogMode.value = 'create';
  noteDraftId.value = 'target';
  noteDraftContent.value = '';
  noteDialogOpen.value = true;
  await nextTick();
  noteIdInputRef.value?.focus();
  noteIdInputRef.value?.select();
}

async function editNote(noteId: string) {
  if (!activeTaskIdNumber.value || busy.value) {
    return;
  }

  const existing = workspace.value.notes.find((note) => note.id === noteId);
  noteDialogMode.value = 'edit';
  noteDraftId.value = noteId;
  noteDraftContent.value = existing?.content ?? '';
  noteDialogOpen.value = true;
  await nextTick();
  noteContentInputRef.value?.focus();
  noteContentInputRef.value?.select();
}

async function submitNoteDialog() {
  const noteId = noteDraftId.value.trim();
  const content = noteDraftContent.value.trim();

  if (!noteId) {
    noteIdInputRef.value?.focus();
    return;
  }
  if (!content) {
    noteContentInputRef.value?.focus();
    return;
  }

  await saveNote(noteId, content);
  closeNoteDialog();
}

function closeNoteDialog() {
  noteDialogOpen.value = false;
  noteDialogMode.value = 'create';
  noteDraftId.value = '';
  noteDraftContent.value = '';
}

function handleNoteDialogOpenChange(open: boolean) {
  if (!open) {
    closeNoteDialog();
  }
}

async function saveNote(noteId: string, content: string) {
  await runWorkspaceAction(async () => {
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('upsert_note', {
      input: {
        taskId: activeTaskIdNumber.value,
        noteId,
        content,
      },
    });
  });
}

async function deleteNote(noteId: string) {
  if (!activeTaskIdNumber.value) {
    return;
  }

  openConfirmDialog({
    title: '删除 Note',
    description: '删除后，这条上下文不会再注入到下一轮 AI 视图中。',
    body: `确认删除 Note「${noteId}」吗？`,
    confirmLabel: '删除 Note',
    action: async () => {
      await runWorkspaceAction(async () => {
        snapshot.value = await invoke<BackendWorkspaceSnapshot>('delete_note', {
          input: {
            taskId: activeTaskIdNumber.value,
            noteId,
          },
        });
      });
      closeConfirmDialog();
    },
  });
}

function openConfirmDialog(input: {
  title: string;
  description: string;
  body: string;
  confirmLabel: string;
  action: () => Promise<void>;
}) {
  confirmDialogTitle.value = input.title;
  confirmDialogDescription.value = input.description;
  confirmDialogBody.value = input.body;
  confirmDialogLabel.value = input.confirmLabel;
  confirmDialogAction.value = input.action;
  confirmDialogOpen.value = true;
}

function closeConfirmDialog() {
  confirmDialogOpen.value = false;
  confirmDialogTitle.value = '';
  confirmDialogDescription.value = '';
  confirmDialogBody.value = '';
  confirmDialogLabel.value = '删除';
  confirmDialogAction.value = null;
}

function handleConfirmDialogOpenChange(open: boolean) {
  confirmDialogOpen.value = open;

  // Radix/shadcn 的 action/cancel 会先驱动弹窗关闭。
  // 这里如果顺手把 action 清空，后面的 click handler 就拿不到真正的删除操作了。
  if (open) {
    return;
  }

  confirmDialogTitle.value = '';
  confirmDialogDescription.value = '';
  confirmDialogBody.value = '';
  confirmDialogLabel.value = '删除';
}

async function submitConfirmDialog() {
  const action = confirmDialogAction.value;
  if (!action) {
    return;
  }
  await action();
}

async function toggleOpenFileLock(path: string, locked: boolean) {
  if (!activeTaskIdNumber.value) {
    return;
  }

  await runWorkspaceAction(async () => {
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('toggle_open_file_lock', {
      input: {
        taskId: activeTaskIdNumber.value,
        path,
        locked,
      },
    });
  });
}

async function closeOpenFile(path: string) {
  if (!activeTaskIdNumber.value) {
    return;
  }

  await runWorkspaceAction(async () => {
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('close_open_file', {
      input: {
        taskId: activeTaskIdNumber.value,
        path,
      },
    });
  });
}

async function openFilesFromComposer(paths: string[]) {
  if (!activeTaskIdNumber.value || !paths.length) {
    return;
  }

  await runWorkspaceAction(async () => {
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('open_files', {
      input: {
        taskId: activeTaskIdNumber.value,
        paths,
      },
    });
  });
}

async function setTaskModel(model: string) {
  if (!activeTaskIdNumber.value || busy.value) {
    return;
  }

  await runWorkspaceAction(async () => {
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('set_task_model', {
      input: {
        taskId: activeTaskIdNumber.value,
        model,
      },
    });
  });
}

async function runWorkspaceAction(action: () => Promise<void>) {
  busy.value = true;
  try {
    await action();
    errorMessage.value = '';
  } catch (error) {
    if (!snapshot.value) {
      console.warn('Failed to load workspace snapshot from Tauri backend, using mock data.', error);
    }
    errorMessage.value = humanizeError(error);
  } finally {
    busy.value = false;
  }
}

function humanizeError(error: unknown) {
  if (typeof error === 'string') {
    return error;
  }
  if (error && typeof error === 'object' && 'message' in error && typeof error.message === 'string') {
    return error.message;
  }
  return 'Unknown error while talking to the March backend.';
}

async function minimizeWindow() {
  await appWindow.minimize();
}

async function toggleMaximize() {
  await appWindow.toggleMaximize();
  isMaximized.value = await appWindow.isMaximized();
}

async function closeWindow() {
  await appWindow.close();
}

</script>
