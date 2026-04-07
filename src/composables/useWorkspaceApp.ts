import { computed, ref } from 'vue';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { useConfirmDialog } from '@/composables/useConfirmDialog';
import { useLiveTurns } from '@/composables/useLiveTurns';
import { useNoteDialog } from '@/composables/useNoteDialog';
import { useAppearanceSettings } from '@/composables/useAppearanceSettings';
import { useProviderSettings } from '@/composables/useProviderSettings';
import type { BackendAgentProgressEvent, BackendWorkspaceSnapshot } from '@/data/mock';
import { useWindowControls } from '@/composables/workspaceApp/useWindowControls';
import { useWorkspaceSnapshotState } from '@/composables/workspaceApp/useWorkspaceSnapshotState';
import { useWorkspaceTaskActions } from '@/composables/workspaceApp/useWorkspaceTaskActions';
import {
  humanizeError,
  type ChatPaneHandle,
  type NoteEditorDialogHandle,
} from '@/composables/workspaceApp/types';

export type { ChatPaneHandle, NoteEditorDialogHandle } from '@/composables/workspaceApp/types';

export function useWorkspaceApp() {
  const appTitle = 'March';
  const busy = ref(false);
  const sendingTaskId = ref<number | null>(null);
  const cancellingTaskId = ref<number | null>(null);
  const errorMessage = ref('');
  const chatPaneRef = ref<ChatPaneHandle | null>(null);
  const noteDialogRef = ref<NoteEditorDialogHandle | null>(null);
  const snapshot = ref<BackendWorkspaceSnapshot | null>(null);
  const workspacePath = computed(() => snapshot.value?.workspace_path);
  const {
    isMaximized,
    initializeWindowState,
    disposeWindowState,
    minimizeWindow,
    toggleMaximize,
    closeWindow,
  } = useWindowControls();

  let unlistenAgentProgress: UnlistenFn | null = null;

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
  });

  const workspaceState = useWorkspaceSnapshotState({
    snapshot,
    liveTurns,
    archivedFailedTurns,
    archivedIntermediateTurns,
    taskActivityStatuses,
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
    runWorkspaceAction,
    refreshWorkspace,
    createTask,
    selectTask,
    deleteTask,
    sendMessage,
    cancelCurrentTurn,
    addNote,
    editNote,
    handleSubmitNoteDialog,
    deleteNote,
    toggleOpenFileLock,
    closeOpenFile,
    openFilesFromComposer,
    setTaskModel,
    setTaskModelSettings,
    setTaskWorkingDirectory,
    refreshSkills,
  } = useWorkspaceTaskActions({
    workspaceState,
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
    openConfirmDialog,
    closeConfirmDialog,
    noteDialogRef,
    submitNoteDialog,
  });

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

  const hasPendingSend = computed(() => sendingTaskId.value !== null);
  const isActiveTaskSending = computed(() =>
    !!workspaceState.activeTaskIdNumber.value && sendingTaskId.value === workspaceState.activeTaskIdNumber.value,
  );
  const isActiveTaskCancelling = computed(() =>
    !!workspaceState.activeTaskIdNumber.value && cancellingTaskId.value === workspaceState.activeTaskIdNumber.value,
  );

  async function initialize() {
    await initializeWindowState();
    unlistenAgentProgress = await listen<BackendAgentProgressEvent>('march://agent-progress', (event) => {
      applyAgentProgress(event.payload);
    });
    await refreshWorkspace();
    await refreshProviderSettings();
  }

  function dispose() {
    if (unlistenAgentProgress) {
      unlistenAgentProgress();
      unlistenAgentProgress = null;
    }
    disposeWindowState();
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
    isMaximized,
    chatPaneRef,
    noteDialogRef,
    workspace: workspaceState.resolvedWorkspace,
    activeTaskIdNumber: workspaceState.activeTaskIdNumber,
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
