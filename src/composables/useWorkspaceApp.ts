import { computed, ref } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useConfirmDialog } from '@/composables/useConfirmDialog';
import { useLiveTurns } from '@/composables/useLiveTurns';
import { useNoteDialog } from '@/composables/useNoteDialog';
import { useMemoryDialog } from '@/composables/useMemoryDialog';
import { useAppearanceSettings } from '@/composables/useAppearanceSettings';
import { useSettingsMemories } from '@/composables/useSettingsMemories';
import { useProviderSettings } from '@/composables/useProviderSettings';
import type { BackendAgentProgressEvent, BackendWorkspaceSnapshot, ChatMessage, DebugRoundItem } from '@/data/mock';
import { debugChat, summarizeAgentEvent, summarizeSnapshot } from '@/lib/chatDebug';
import { useWindowControls } from '@/composables/workspaceApp/useWindowControls';
import { useWorkspaceSnapshotState } from '@/composables/workspaceApp/useWorkspaceSnapshotState';
import { useTaskChatState } from '@/composables/workspaceApp/useTaskChatState';
import { useWorkspaceTaskActions } from '@/composables/workspaceApp/useWorkspaceTaskActions';
import {
  humanizeError,
  type ChatPaneHandle,
  type MemoryEditorDialogHandle,
  type NoteEditorDialogHandle,
} from '@/composables/workspaceApp/types';

export type { ChatPaneHandle, MemoryEditorDialogHandle, NoteEditorDialogHandle } from '@/composables/workspaceApp/types';

type BackendNotice = {
  id: string;
  level: 'error' | 'warning';
  source: string;
  message: string;
  timestamp: number;
};

export function useWorkspaceApp() {
  const appTitle = 'March';
  const busy = ref(false);
  const sendingTaskId = ref<number | null>(null);
  const cancellingTaskId = ref<number | null>(null);
  const errorMessage = ref('');
  const backendNotices = ref<BackendNotice[]>([]);
  const chatPaneRef = ref<ChatPaneHandle | null>(null);
  const noteDialogRef = ref<NoteEditorDialogHandle | null>(null);
  const memoryDialogRef = ref<MemoryEditorDialogHandle | null>(null);
  const snapshot = ref<BackendWorkspaceSnapshot | null>(null);
  const workspacePath = computed(() => snapshot.value?.workspace_path);
  let appendTaskChatMessage = (_taskId: number, _message: ChatMessage) => {};
  let appendTaskDebugRound = (_taskId: number, _round: DebugRoundItem) => {};
  let setTaskRuntimeSnapshot = (
    _taskId: number,
    _runtime: NonNullable<NonNullable<BackendWorkspaceSnapshot['active_task']>['runtime']>,
  ) => {};
  const {
    isMaximized,
    initializeWindowState,
    disposeWindowState,
    minimizeWindow,
    toggleMaximize,
    closeWindow,
  } = useWindowControls();

  let unlistenAgentProgress: UnlistenFn | null = null;
  let unlistenMemoryChanged: UnlistenFn | null = null;
  let unlistenBackendNotice: UnlistenFn | null = null;

  const {
    liveTurns,
    archivedFailedTurns,
    archivedIntermediateTurns,
    taskActivityStatuses,
    applyAgentProgress,
    upsertLiveTurn,
    archiveFailedTurn,
    clearLiveTurn,
    clearTaskActivity,
    clearArchivedFailedTurns,
    clearArchivedIntermediateTurns,
  } = useLiveTurns({
    snapshot,
    sendingTaskId,
    errorMessage,
    workspacePath,
    appendTaskChatMessage: (taskId, message) => appendTaskChatMessage(taskId, message),
    appendTaskDebugRound: (taskId, round) => appendTaskDebugRound(taskId, round),
    setTaskRuntimeSnapshot: (taskId, runtime) => setTaskRuntimeSnapshot(taskId, runtime),
  });

  const workspaceState = useWorkspaceSnapshotState({
    snapshot,
    liveTurns,
    taskActivityStatuses,
  });
  setTaskRuntimeSnapshot = workspaceState.setTaskRuntimeSnapshot;
  const taskChatState = useTaskChatState({
    snapshot,
    activeTaskIdNumber: workspaceState.activeTaskIdNumber,
    liveTurns,
    archivedFailedTurns,
    archivedIntermediateTurns,
  });
  appendTaskChatMessage = taskChatState.appendTaskChatMessage;
  appendTaskDebugRound = workspaceState.appendTaskDebugRound;

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
    memoryDialogOpen,
    memoryDialogMode,
    memoryDraftId,
    memoryDraftType,
    memoryDraftTopic,
    memoryDraftTitle,
    memoryDraftContent,
    memoryDraftTags,
    memoryDraftScope,
    memoryDraftLevel,
    openCreateMemoryDialog,
    openEditMemoryDialog,
    closeMemoryDialog,
    handleMemoryDialogOpenChange,
    submitMemoryDialog,
  } = useMemoryDialog();

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
    runWorkspaceAction,
    refreshWorkspace,
    createTask,
    selectTask,
    deleteTask,
    sendMessage,
    cancelCurrentTurn,
    addNote,
    editNote,
    addMemory,
    editMemory,
    handleSubmitNoteDialog,
    handleSubmitMemoryDialog,
    deleteNote,
    deleteMemory,
    toggleOpenFileLock,
    closeOpenFile,
    openFilesFromComposer,
    setTaskModel,
    setTaskModelSettings,
    setTaskWorkingDirectory,
    refreshSkills,
  } = useWorkspaceTaskActions({
    workspaceState,
    taskChatState,
    liveTurns,
    sendingTaskId,
    cancellingTaskId,
    busy,
    errorMessage,
    chatPaneRef,
    clearTaskActivity,
    upsertLiveTurn,
    archiveFailedTurn,
    clearLiveTurn,
    clearArchivedFailedTurns,
    clearArchivedIntermediateTurns,
    openCreateNoteDialog,
    openEditNoteDialog,
    openCreateMemoryDialog,
    openEditMemoryDialog,
    openConfirmDialog,
    closeConfirmDialog,
    noteDialogRef,
    memoryDialogRef,
    submitNoteDialog,
    submitMemoryDialog,
    onMemoryMutated: async () => {
      if (settingsOpen.value) {
        await refreshSettingsMemories();
      }
    },
  });

  const {
    settingsOpen,
    providerSettings,
    providerProbeModels,
    providerProbeSuggestedModels,
    providerProbeModelsLoading,
    providerTestLoading,
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
    saveDefaultModel,
    loadProbeModels,
    invalidateProbeModelsCache,
  } = useProviderSettings({
    runWorkspaceAction,
    humanizeError,
  });

  const {
    settingsMemories,
    settingsMemoriesLoading,
    refreshSettingsMemories,
    createMemoryFromSettings,
    editMemoryFromSettings,
    deleteMemoryFromSettings,
  } = useSettingsMemories({
    snapshot,
    activeTaskIdNumber: workspaceState.activeTaskIdNumber,
    runWorkspaceAction,
    openCreateMemoryDialog,
    openEditMemoryDialog,
  });

  const hasPendingSend = computed(() => sendingTaskId.value !== null);
  const isActiveTaskSending = computed(() =>
    !!workspaceState.activeTaskIdNumber.value && sendingTaskId.value === workspaceState.activeTaskIdNumber.value,
  );
  const isActiveTaskCancelling = computed(() =>
    !!workspaceState.activeTaskIdNumber.value && cancellingTaskId.value === workspaceState.activeTaskIdNumber.value,
  );

  async function initialize() {
    debugChat('workspace-app', 'initialize:start');
    await initializeWindowState();
    unlistenAgentProgress = await listen<BackendAgentProgressEvent>('march://agent-progress', (event) => {
      debugChat('workspace-app', 'event:received', summarizeAgentEvent(event.payload));
      applyAgentProgress(event.payload);
    });
    unlistenMemoryChanged = await listen('march://memory-changed', async () => {
      debugChat('workspace-app', 'memory-changed:received', {
        busy: busy.value,
        activeTaskId: workspaceState.activeTaskIdNumber.value,
      });
      if (!busy.value) {
        debugChat('workspace-app', 'memory-changed:refreshWorkspace:start', {
          activeTaskId: workspaceState.activeTaskIdNumber.value,
        });
        await refreshWorkspace(workspaceState.activeTaskIdNumber.value);
        debugChat('workspace-app', 'memory-changed:refreshWorkspace:done', summarizeSnapshot(snapshot.value));
        if (settingsOpen.value) {
          await refreshSettingsMemories();
        }
      }
    });
    unlistenBackendNotice = await listen<{
      level: 'error' | 'warning';
      source: string;
      message: string;
      timestamp: number;
    }>('march://backend-notice', (event) => {
      pushBackendNotice(event.payload);
    });
    debugChat('workspace-app', 'refreshWorkspace:init:start');
    await refreshWorkspace();
    debugChat('workspace-app', 'refreshWorkspace:init:done', summarizeSnapshot(snapshot.value));
    await refreshProviderSettings();
    debugChat('workspace-app', 'initialize:done');
  }

  function dispose() {
    debugChat('workspace-app', 'dispose:start');
    if (unlistenAgentProgress) {
      unlistenAgentProgress();
      unlistenAgentProgress = null;
    }
    if (unlistenMemoryChanged) {
      unlistenMemoryChanged();
      unlistenMemoryChanged = null;
    }
    if (unlistenBackendNotice) {
      unlistenBackendNotice();
      unlistenBackendNotice = null;
    }
    disposeWindowState();
    debugChat('workspace-app', 'dispose:done');
  }

  function pushBackendNotice(payload: {
    level: 'error' | 'warning';
    source: string;
    message: string;
    timestamp: number;
  }) {
    const normalizedMessage = payload.message.trim();
    if (!normalizedMessage) {
      return;
    }

    const latest = backendNotices.value[0];
    if (
      latest
      && latest.level === payload.level
      && latest.message === normalizedMessage
      && Math.abs(latest.timestamp - payload.timestamp) < 1500
    ) {
      return;
    }

    backendNotices.value = [
      {
        id: `${payload.timestamp}:${payload.level}:${payload.source}:${normalizedMessage}`,
        level: payload.level,
        source: payload.source,
        message: normalizedMessage,
        timestamp: payload.timestamp,
      },
      ...backendNotices.value,
    ].slice(0, 8);
  }

  function dismissBackendNotice(id: string) {
    backendNotices.value = backendNotices.value.filter((notice) => notice.id !== id);
  }

  async function handleOpenSettings() {
    if (busy.value) {
      return;
    }
    await openSettings();
    await refreshSettingsMemories();
  }

  async function requestProbeModels(input: {
    id?: number;
    providerType: string;
    baseUrl: string;
    apiKey: string;
    probeModel?: string;
    forceRefresh?: boolean;
  }) {
    try {
      await loadProbeModels(input, {
        forceRefresh: input.forceRefresh,
      });
      errorMessage.value = '';
    } catch (error) {
      errorMessage.value = humanizeError(error);
    }
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

  return {
    appTitle,
    busy,
    errorMessage,
    backendNotices,
    isMaximized,
    chatPaneRef,
    noteDialogRef,
    memoryDialogRef,
    workspace: workspaceState.resolvedWorkspace,
    taskListView: workspaceState.taskListView,
    chatView: taskChatState.chatView,
    composerView: workspaceState.composerView,
    contextView: workspaceState.contextView,
    activeTaskIdNumber: workspaceState.activeTaskIdNumber,
    hasPendingSend,
    isActiveTaskSending,
    isActiveTaskCancelling,
    settingsOpen,
    theme,
    providerSettings,
    providerProbeModels,
    providerProbeSuggestedModels,
    providerProbeModelsLoading,
    providerTestLoading,
    providerTestMessage,
    providerTestSuccess,
    settingsMemories,
    settingsMemoriesLoading,
    noteDialogOpen,
    noteDialogMode,
    noteDraftId,
    noteDraftContent,
    memoryDialogOpen,
    memoryDialogMode,
    memoryDraftId,
    memoryDraftType,
    memoryDraftTopic,
    memoryDraftTitle,
    memoryDraftContent,
    memoryDraftTags,
    memoryDraftScope,
    memoryDraftLevel,
    confirmDialogOpen,
    confirmDialogTitle,
    confirmDialogDescription,
    confirmDialogBody,
    confirmDialogLabel,
    initialize,
    dispose,
    dismissBackendNotice,
    createTask,
    selectTask,
    deleteTask,
    sendMessage,
    cancelCurrentTurn,
    addNote,
    editNote,
    addMemory,
    editMemory,
    handleSubmitNoteDialog,
    handleSubmitMemoryDialog,
    closeNoteDialog,
    closeMemoryDialog,
    handleNoteDialogOpenChange,
    handleMemoryDialogOpenChange,
    deleteNote,
    deleteMemory,
    toggleOpenFileLock,
    closeOpenFile,
    openFilesFromComposer,
    refreshSkills,
    setTaskModel,
    setTaskModelSettings,
    setTaskWorkingDirectory,
    handleOpenSettings,
    closeSettings,
    createMemoryFromSettings,
    editMemoryFromSettings,
    deleteMemoryFromSettings,
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
    saveDefaultModel,
    invalidateProbeModelsCache,
    handleConfirmDialogOpenChange,
    submitConfirmDialog,
    minimizeWindow,
    toggleMaximize,
    closeWindow,
  };
}
