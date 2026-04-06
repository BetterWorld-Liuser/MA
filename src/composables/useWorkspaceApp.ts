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
  type ChatImageAttachment,
  type ChatMessage,
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
  const optimisticTaskId = ref<string | null>(null);
  const optimisticDeletedTaskIds = ref<Set<string>>(new Set());
  const localComposerMessages = ref<Record<number, ChatMessage[]>>({});
  const workspacePath = computed(() => snapshot.value?.workspace_path);
  const busy = ref(false);
  const sendingTaskId = ref<number | null>(null);
  const cancellingTaskId = ref<number | null>(null);
  const errorMessage = ref('');
  const isMaximized = ref(false);
  const chatPaneRef = ref<ChatPaneHandle | null>(null);
  const noteDialogRef = ref<NoteEditorDialogHandle | null>(null);

  let unlistenAgentProgress: UnlistenFn | null = null;
  let unlistenWindowResize: UnlistenFn | null = null;

  const {
    liveTurns,
    archivedFailedTurns,
    applyAgentProgress,
    upsertLiveTurn,
    archiveFailedTurn,
    clearLiveTurn,
    clearArchivedFailedTurns,
  } = useLiveTurns({
    snapshot,
    sendingTaskId,
    errorMessage,
    workspacePath,
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
    providerProbeModels,
    providerProbeSuggestedModels,
    providerProbeModelsLoading,
    providerTestMessage,
    providerTestSuccess,
    refreshProviderSettings,
    openSettings,
    closeSettings,
    saveProvider,
    testProviderConnection,
    deleteProvider,
    saveProviderModel,
    deleteProviderModel,
    saveAgent,
    deleteAgent,
    restoreMarchPrompt,
    saveDefaultProvider,
    loadProviderModelsForSettings,
    loadProbeModels,
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
      const baseWorkspace = toWorkspaceView(snapshot.value);
      const archivedMessages = activeTaskId
        ? (archivedFailedTurns.value[activeTaskId] ?? []).map((entry) => entry.message)
        : [];
      const mergedChat = [...baseWorkspace.chat, ...archivedMessages].sort(
        (left, right) => (left.timestamp ?? Number.MAX_SAFE_INTEGER) - (right.timestamp ?? Number.MAX_SAFE_INTEGER),
      );
      return {
        ...baseWorkspace,
        chat: mergeChatWithComposerMessages(activeTaskId ?? undefined, mergedChat),
        liveTurn: activeTaskId ? liveTurns.value[activeTaskId] : undefined,
      };
    }

    return mockWorkspace;
  });

  const resolvedWorkspace = computed<WorkspaceView>(() => {
    const baseWorkspace = workspace.value;
    let nextWorkspace = baseWorkspace;

    if (optimisticTaskId.value) {
      const optimisticTask = {
        id: optimisticTaskId.value,
        name: '默认任务',
        status: 'active' as const,
        updatedAt: '刚刚',
      };

      nextWorkspace = {
        ...baseWorkspace,
        title: optimisticTask.name,
        tasks: [
          optimisticTask,
          ...baseWorkspace.tasks
            .filter((task) => task.id !== optimisticTask.id)
            .map((task) => ({
              ...task,
              status: 'idle' as const,
            })),
        ],
        activeTaskId: optimisticTask.id,
        selectedModel: undefined,
        selectedTemperature: undefined,
        selectedTopP: undefined,
        selectedPresencePenalty: undefined,
        selectedFrequencyPenalty: undefined,
        selectedMaxOutputTokens: undefined,
        workingDirectory: baseWorkspace.workspacePath ?? baseWorkspace.workingDirectory,
        chat: [],
        notes: [],
        openFiles: [],
        hints: [],
        skills: [],
        contextUsage: {
          percent: 0,
          current: '0',
          limit: baseWorkspace.contextUsage.limit,
          sections: [],
        },
        debugRounds: [],
        liveTurn: undefined,
      };
    }

    if (!optimisticDeletedTaskIds.value.size) {
      return nextWorkspace;
    }

    const visibleTasks = nextWorkspace.tasks.filter((task) => !optimisticDeletedTaskIds.value.has(task.id));
    const activeTaskVisible = nextWorkspace.activeTaskId && !optimisticDeletedTaskIds.value.has(nextWorkspace.activeTaskId);
    const fallbackActiveTaskId = activeTaskVisible ? nextWorkspace.activeTaskId : (visibleTasks[0]?.id ?? '');

    if (activeTaskVisible) {
      return {
        ...nextWorkspace,
        tasks: visibleTasks,
      };
    }

    const fallbackTaskName = visibleTasks.find((task) => task.id === fallbackActiveTaskId)?.name ?? 'March';

    return {
      ...nextWorkspace,
      title: fallbackTaskName,
      tasks: visibleTasks,
      activeTaskId: fallbackActiveTaskId,
      selectedModel: undefined,
      selectedTemperature: undefined,
      selectedTopP: undefined,
      selectedPresencePenalty: undefined,
      selectedFrequencyPenalty: undefined,
      selectedMaxOutputTokens: undefined,
      workingDirectory: nextWorkspace.workspacePath ?? nextWorkspace.workingDirectory,
      chat: [],
      notes: [],
      openFiles: [],
      hints: [],
      skills: [],
      contextUsage: {
        percent: 0,
        current: '0',
        limit: nextWorkspace.contextUsage.limit,
        sections: [],
      },
      debugRounds: [],
      liveTurn: undefined,
    };
  });

  const activeTaskIdNumber = computed(() => {
    const raw = resolvedWorkspace.value.activeTaskId;
    if (!raw || raw === optimisticTaskId.value) {
      return null;
    }
    const parsed = Number(raw);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
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

    optimisticTaskId.value = `pending-task-${Date.now()}`;
    await nextTick();

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('create_task', {
        input: {},
      });
    });
    optimisticTaskId.value = null;
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
        optimisticDeletedTaskIds.value = new Set([...optimisticDeletedTaskIds.value, taskId]);
        const succeeded = await runWorkspaceAction(async () => {
          snapshot.value = await invoke<BackendWorkspaceSnapshot>('delete_task', {
            input: { taskId: Number(taskId) },
          });
        });
        optimisticDeletedTaskIds.value = new Set(
          [...optimisticDeletedTaskIds.value].filter((id) => id !== taskId),
        );
        if (!succeeded) {
          return;
        }
        clearLiveTurn(Number(taskId));
        clearArchivedFailedTurns(Number(taskId));
        delete localComposerMessages.value[Number(taskId)];
        if (sendingTaskId.value === Number(taskId)) {
          sendingTaskId.value = null;
        }
        closeConfirmDialog();
      },
    });
  }

  async function sendMessage(payload: { content: string; directories: string[]; files: string[]; skills: string[]; images: ChatImageAttachment[] }) {
    if (!activeTaskIdNumber.value || sendingTaskId.value !== null) {
      return;
    }

    const taskId = activeTaskIdNumber.value;
    const content = augmentComposerMessage(payload);

    queueLocalComposerMessage(taskId, {
      role: 'user',
      author: 'User',
      time: new Date().toLocaleTimeString([], {
        hour: '2-digit',
        minute: '2-digit',
      }),
      timestamp: Date.now(),
      content,
      images: payload.images,
    });
    upsertLiveTurn(taskId, {
      turnId: `pending-${Date.now()}`,
      author: 'March',
      state: 'pending',
      statusLabel: '已发送，正在准备',
      content: '',
      errorMessage: '',
      tools: [],
    });
    sendingTaskId.value = taskId;

    try {
      const openFilePaths = Array.from(new Set([...payload.files, ...payload.skills]));
      if (openFilePaths.length) {
        snapshot.value = await invoke<BackendWorkspaceSnapshot>('open_files', {
          input: {
            taskId,
            paths: openFilePaths,
          },
        });
      }

      const nextSnapshot = await invoke<BackendWorkspaceSnapshot>('send_message', {
        input: {
          taskId,
          contentBlocks: [
            ...(content
              ? [{ type: 'text', text: content }]
              : []),
            ...payload.images.map((image) => ({
              type: 'image',
              media_type: image.mediaType,
              data_base64: extractBase64Payload(image.previewUrl),
              source_path: image.sourcePath ?? null,
              name: image.name,
            })),
          ],
        },
      });
      clearLocalComposerMessages(taskId);
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
        const failedTurn = {
          ...currentLiveTurn,
          state: 'error',
          statusLabel: '本轮执行失败',
          errorMessage: humanizeError(error),
        } as const;
        upsertLiveTurn(taskId, failedTurn);
        archiveFailedTurn(taskId, failedTurn);
        clearLiveTurn(taskId);
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

  function augmentComposerMessage(payload: { content: string; directories: string[]; files: string[]; skills: string[]; images: ChatImageAttachment[] }) {
    const base = payload.content.trim();
    const sections = [base];
    if (payload.directories.length) {
      sections.push(`[目录引用]\n${payload.directories.map((path) => `- ${path}`).join('\n')}`);
    }
    return sections.filter(Boolean).join('\n\n');
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

  async function setTaskModel(selection: { providerId?: number | null; model: string }) {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('set_task_model', {
        input: {
          taskId: activeTaskIdNumber.value,
          providerId: selection.providerId ?? null,
          model: selection.model,
        },
      });
    });
  }

  async function setTaskModelSettings(settings: {
    temperature?: number | null;
    topP?: number | null;
    presencePenalty?: number | null;
    frequencyPenalty?: number | null;
    maxOutputTokens?: number | null;
  }) {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('set_task_model_settings', {
        input: {
          taskId: activeTaskIdNumber.value,
          temperature: settings.temperature ?? null,
          topP: settings.topP ?? null,
          presencePenalty: settings.presencePenalty ?? null,
          frequencyPenalty: settings.frequencyPenalty ?? null,
          maxOutputTokens: settings.maxOutputTokens ?? null,
        },
      });
    });
  }

  async function setTaskWorkingDirectory(path?: string | null) {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('set_task_working_directory', {
        input: {
          taskId: activeTaskIdNumber.value,
          path: path ?? null,
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

  async function requestProbeModels(input: {
    id?: number;
    providerType: string;
    baseUrl: string;
    apiKey: string;
    probeModel?: string;
  }) {
    try {
      await loadProbeModels(input);
      errorMessage.value = '';
    } catch (error) {
      errorMessage.value = humanizeError(error);
    }
  }

  async function refreshSkills() {
    if (!activeTaskIdNumber.value || busy.value) {
      return;
    }

    await refreshWorkspace(activeTaskIdNumber.value);
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

  function confirmDeleteAgent(name: string) {
    openConfirmDialog({
      title: '删除角色',
      description: '删除后，这个角色将无法继续通过 @mention 召唤。',
      body: `确认删除角色「${name}」吗？`,
      confirmLabel: '删除角色',
      action: async () => {
        await deleteAgent(name);
        closeConfirmDialog();
      },
    });
  }

  async function runWorkspaceAction(action: () => Promise<void>) {
    busy.value = true;
    try {
      await action();
      errorMessage.value = '';
      return true;
    } catch (error) {
      optimisticTaskId.value = null;
      if (!snapshot.value) {
        console.warn('Failed to load workspace snapshot from Tauri backend, using mock data.', error);
      }
      errorMessage.value = humanizeError(error);
      return false;
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

  function queueLocalComposerMessage(taskId: number, message: ChatMessage) {
    localComposerMessages.value = {
      ...localComposerMessages.value,
      [taskId]: [...(localComposerMessages.value[taskId] ?? []), message],
    };
  }

  function clearLocalComposerMessages(taskId: number) {
    if (!(taskId in localComposerMessages.value)) {
      return;
    }

    const nextMessages = { ...localComposerMessages.value };
    delete nextMessages[taskId];
    localComposerMessages.value = nextMessages;
  }

  function mergeChatWithComposerMessages(taskId: number | undefined, chat: ChatMessage[]) {
    if (!taskId) {
      return chat;
    }

    const localMessages = localComposerMessages.value[taskId] ?? [];
    if (!localMessages.length) {
      return chat;
    }

    const merged = chat.map((message) => ({
      ...message,
      images: message.images ? [...message.images] : undefined,
    }));
    const usedIndices = new Set<number>();
    const unmatched: ChatMessage[] = [];

    for (const localMessage of localMessages) {
      const matchIndex = merged.findIndex((message, index) =>
        !usedIndices.has(index)
        && message.role === localMessage.role
        && message.content === localMessage.content
        && Math.abs((message.timestamp ?? 0) - (localMessage.timestamp ?? 0)) < 120000,
      );

      if (matchIndex >= 0) {
        usedIndices.add(matchIndex);
        if (localMessage.images?.length) {
          merged[matchIndex] = {
            ...merged[matchIndex],
            images: localMessage.images,
          };
        }
      } else {
        unmatched.push(localMessage);
      }
    }

    return [...merged, ...unmatched].sort(
      (left, right) => (left.timestamp ?? Number.MAX_SAFE_INTEGER) - (right.timestamp ?? Number.MAX_SAFE_INTEGER),
    );
  }

  function extractBase64Payload(dataUrl: string) {
    const separatorIndex = dataUrl.indexOf(',');
    return separatorIndex >= 0 ? dataUrl.slice(separatorIndex + 1) : dataUrl;
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
    workspace: resolvedWorkspace,
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
    providerProbeModels,
    providerProbeSuggestedModels,
    providerProbeModelsLoading,
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
    refreshSkills,
    setTaskModel,
    setTaskModelSettings,
    setTaskWorkingDirectory,
    handleOpenSettings,
    closeSettings,
    setTheme,
    saveProvider,
    testProviderConnection,
    saveProviderModel,
    deleteProviderModel,
    saveAgent,
    confirmDeleteAgent,
    restoreMarchPrompt,
    requestProbeModels,
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
