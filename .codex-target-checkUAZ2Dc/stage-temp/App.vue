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
          :disabled="!activeTaskIdNumber"
          :sending="sending"
          @send="sendMessage"
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
const sending = ref(false);
const errorMessage = ref('');
const isMaximized = ref(false);
const appWindow = getCurrentWindow();
let unlistenAgentProgress: UnlistenFn | null = null;
const liveTurn = ref<LiveTurn | null>(null);
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
  if (snapshot.value) {
    return {
      ...toWorkspaceView(snapshot.value),
      liveTurn: liveTurn.value ?? undefined,
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
      closeConfirmDialog();
    },
  });
}

async function sendMessage(content: string) {
  if (!activeTaskIdNumber.value || sending.value) {
    return;
  }

  appendOptimisticUserMessage(content);
  liveTurn.value = {
    turnId: `pending-${Date.now()}`,
    state: 'pending',
    statusLabel: '已发送，正在准备',
    content: '',
    tools: [],
  };
  sending.value = true;
  try {
    snapshot.value = await invoke<BackendWorkspaceSnapshot>('send_message', {
      input: {
        taskId: activeTaskIdNumber.value,
        content,
      },
    });
    liveTurn.value = null;
    errorMessage.value = '';
  } catch (error) {
    if (liveTurn.value) {
      liveTurn.value = {
        ...liveTurn.value,
        state: 'error',
        statusLabel: '本轮执行失败',
      };
    }
    errorMessage.value = humanizeError(error);
  } finally {
    sending.value = false;
  }
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
  if (!snapshot.value?.active_task) {
    return;
  }
  if (!activeTaskIdNumber.value || event.task_id !== activeTaskIdNumber.value) {
    return;
  }

  switch (event.kind) {
    case 'turn_started':
      liveTurn.value = {
        turnId: event.turn_id,
        state: 'pending',
        statusLabel: '正在整理上下文',
        content: '',
        tools: [],
      };
      return;
    case 'status':
      ensureLiveTurn(event.turn_id);
      if (!liveTurn.value) {
        return;
      }
      liveTurn.value = {
        ...liveTurn.value,
        state: liveTurn.value.content ? liveTurn.value.state : 'running',
        statusLabel: event.label,
      };
      return;
    case 'tool_started':
      ensureLiveTurn(event.turn_id);
      if (!liveTurn.value) {
        return;
      }
      liveTurn.value = {
        ...liveTurn.value,
        tools: [
          ...liveTurn.value.tools,
          {
            id: event.tool_call_id,
            label: event.tool_name,
            summary: event.summary,
            state: 'running',
          },
        ],
      };
      return;
    case 'tool_finished':
      ensureLiveTurn(event.turn_id);
      if (!liveTurn.value) {
        return;
      }
      liveTurn.value = {
        ...liveTurn.value,
        tools: liveTurn.value.tools.map((tool) =>
          tool.id === event.tool_call_id
            ? {
                ...tool,
                state: event.status,
                summary: event.summary || tool.summary,
                preview: event.preview ?? undefined,
              }
            : tool,
        ),
      };
      return;
    case 'reply_preview':
      ensureLiveTurn(event.turn_id);
      if (!liveTurn.value) {
        return;
      }
      liveTurn.value = {
        ...liveTurn.value,
        state: 'streaming',
        statusLabel: '正在生成回复',
        content: event.message,
      };
      return;
    case 'reply':
      snapshot.value = {
        ...snapshot.value,
        active_task: event.task,
      };
      if (event.wait) {
        liveTurn.value = null;
      }
      return;
    case 'round_complete':
      snapshot.value = {
        ...snapshot.value,
        active_task: event.task,
      };
      return;
  }
}

function ensureLiveTurn(turnId: string) {
  if (liveTurn.value?.turnId === turnId) {
    return;
  }

  liveTurn.value = {
    turnId,
    state: 'running',
    statusLabel: '正在处理',
    content: '',
    tools: [],
  };
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
