import { computed, nextTick, ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { useConfirmDialog } from '@/composables/useConfirmDialog';
import { useLiveTurns } from '@/composables/useLiveTurns';
import { useNoteDialog } from '@/composables/useNoteDialog';
import { useAppearanceSettings } from '@/composables/useAppearanceSettings';
import { useProviderSettings } from '@/composables/useProviderSettings';
import {
  mockWorkspace,
  toWorkspaceView,
  type BackendAgentProgressEvent,
  type BackendWorkspaceSnapshot,
  type WorkspaceView,
} from '@/data/mock';

export type ChatPaneHandle = { focusComposer: () => void };
export type NoteEditorDialogHandle = {
  focusIdField: () => void;
  focusContentField: () => void;
};

export function useWorkspaceApp() {
  const appTitle = 'March';
  const appWindow = getCurrentWindow();
  const snapshot = ref<BackendWorkspaceSnapshot | null>(null);
  const busy = ref(false);
  const sendingTaskId = ref<number | null>(null);
  const cancellingTaskId = ref<number | null>(null);
  const errorMessage = ref('');
  const isMaximized = ref(false);
  const chatPaneRef = ref<ChatPaneHandle | null>(null);
  const noteDialogRef = ref<NoteEditorDialogHandle | null>(null);

  let unlistenAgentProgress: UnlistenFn | null = null;
  let unlistenWindowResize: UnlistenFn | null = null;

  const { liveTurns, applyAgentProgress, upsertLiveTurn, clearLiveTurn } = useLiveTurns({
    snapshot,
    sendingTaskId,
    errorMessage,
  });

  const {
    noteDialogOpen,
    noteDialogMode,
    noteDraftId,
    noteDraftContent,
    openCreateNoteDialog,
    openEditNoteDialog,
    closeNoteDialog,
    handleNoteDialogOpenChange,
    submitNoteDialog,
  } = useNoteDialog();

  const {
    confirmDialogOpen,
    confirmDialogTitle,
    confirmDialogDescription,
    confirmDialogBody,
    confirmDialogLabel,
    openConfirmDialog,
    closeConfirmDialog,
    handleConfirmDialogOpenChange,
    submitConfirmDialog,
  } = useConfirmDialog();

  const { theme, setTheme } = useAppearanceSettings();

  const {
    settingsOpen,
    providerSettings,
    providerModels,
    providerSuggestedModels,
    providerModelsLoading,
    providerTestMessage,
    providerTestSuccess,
    refreshProviderSettings,
    openSettings,
    closeSettings,
    saveProvider,
    testProviderConnection,
    deleteProvider,
    saveDefaultProvider,
    loadProviderModelsForSettings,
  } = useProviderSettings({
    runWorkspaceAction,
    setErrorMessage: (message) => {
      errorMessage.value = message;
    },
    humanizeError,
  });

  const workspace = computed<WorkspaceView>(() => {
    const activeTaskId = snapshot.value?.active_task?.task.id ?? (snapshot.value?.tasks[0]?.id ?? null);

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

  const hasPendingSend = computed(() => sendingTaskId.value !== null);
  const isActiveTaskSending = computed(() =>
    !!activeTaskIdNumber.value && sendingTaskId.value === activeTaskIdNumber.value,
  );
  const isActiveTaskCancelling = computed(() =>
    !!activeTaskIdNumber.value && cancellingTaskId.value === activeTaskIdNumber.value,
  );

  async function initialize() {
    isMaximized.value = await appWindow.isMaximized();
    unlistenAgentProgress = await listen<BackendAgentProgressEvent>('ma://agent-progress', (event) => {
      applyAgentProgress(event.payload);
    });
    unlistenWindowResize = await appWindow.onResized(async () => {
      isMaximized.value = await appWindow.isMaximized();
    });
    await refreshWorkspace();
    await refreshProviderSettings();
  }

  function dispose() {
    if (unlistenAgentProgress) {
      unlistenAgentProgress();
      unlistenAgentProgress = null;
    }
    if (unlistenWindowResize) {
      unlistenWindowResize();
      unlistenWindowResize = null;
    }
  }

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
      if (cancellingTaskId.value === taskId) {
        cancellingTaskId.value = null;
      }
    }
  }

  async function cancelCurrentTurn() {
    if (!sendingTaskId.value || cancellingTaskId.value === sendingTaskId.value) {
      return;
    }

    const taskId = sendingTaskId.value;
    cancellingTaskId.value = taskId;
    const currentLiveTurn = liveTurns.value[taskId];
    if (currentLiveTurn) {
      upsertLiveTurn(taskId, {
        ...currentLiveTurn,
        statusLabel: '正在中断…',
      });
    }

    try {
      await invoke('cancel_turn', { taskId });
    } catch (error) {
      cancellingTaskId.value = null;
      errorMessage.value = humanizeError(error);
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

  async function addNote() {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }
    openCreateNoteDialog();
    await nextTick();
    noteDialogRef.value?.focusIdField();
  }

  async function editNote(noteId: string) {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    const existing = workspace.value.notes.find((note) => note.id === noteId);
    openEditNoteDialog({
      id: noteId,
      content: existing?.content ?? '',
    });
    await nextTick();
    noteDialogRef.value?.focusContentField();
  }

  async function handleSubmitNoteDialog() {
    await submitNoteDialog(saveNote, {
      id: () => {
        noteDialogRef.value?.focusIdField();
      },
      content: () => {
        noteDialogRef.value?.focusContentField();
      },
    });
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

  async function handleOpenSettings() {
    if (busy.value) {
      return;
    }
    await openSettings();
  }

  function confirmDeleteProvider(providerId: number) {
    openConfirmDialog({
      title: '删除 Provider',
      description: '删除后，March 将无法继续使用这个 provider 发起模型请求。',
      body: '确认删除这个 provider 吗？',
      confirmLabel: '删除 Provider',
      action: async () => {
        await deleteProvider(providerId);
        closeConfirmDialog();
      },
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

  return {
    appTitle,
    busy,
    errorMessage,
    isMaximized,
    chatPaneRef,
    noteDialogRef,
    workspace,
    activeTaskIdNumber,
    hasPendingSend,
    isActiveTaskSending,
    isActiveTaskCancelling,
    settingsOpen,
    theme,
    providerSettings,
    providerModels,
    providerSuggestedModels,
    providerModelsLoading,
    providerTestMessage,
    providerTestSuccess,
    noteDialogOpen,
    noteDialogMode,
    noteDraftId,
    noteDraftContent,
    confirmDialogOpen,
    confirmDialogTitle,
    confirmDialogDescription,
    confirmDialogBody,
    confirmDialogLabel,
    initialize,
    dispose,
    createTask,
    selectTask,
    deleteTask,
    sendMessage,
    cancelCurrentTurn,
    addNote,
    editNote,
    handleSubmitNoteDialog,
    closeNoteDialog,
    handleNoteDialogOpenChange,
    deleteNote,
    toggleOpenFileLock,
    closeOpenFile,
    openFilesFromComposer,
    setTaskModel,
    handleOpenSettings,
    closeSettings,
    setTheme,
    saveProvider,
    testProviderConnection,
    confirmDeleteProvider,
    saveDefaultProvider,
    loadProviderModelsForSettings,
    handleConfirmDialogOpenChange,
    submitConfirmDialog,
    minimizeWindow,
    toggleMaximize,
    closeWindow,
  };
}
