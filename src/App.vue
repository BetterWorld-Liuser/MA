<template>
  <div class="relative h-screen overflow-hidden bg-bg text-text">
    <AppTitleBar
      :title="appTitle"
      :is-maximized="isMaximized"
      @minimize="minimizeWindow"
      @toggle-maximize="toggleMaximize"
      @close="closeWindow"
    />

    <div class="mx-auto flex h-[calc(100%-2.5rem)] min-h-0 max-w-[1920px] flex-col gap-2 px-2 py-2 lg:px-3 lg:py-3">
      <div
        v-if="errorMessage"
        class="bg-[rgba(224,82,82,0.06)] px-4 py-3 text-sm text-text"
        style="border-bottom: 1px solid rgba(224, 82, 82, 0.28)"
      >
        {{ errorMessage }}
      </div>

      <main class="grid min-h-0 flex-1 overflow-hidden gap-0 lg:grid-cols-[256px_minmax(0,1fr)_332px]">
        <TaskList
          title="任务"
          :tasks="workspace.tasks"
          :active-task-id="workspace.activeTaskId"
          :busy="busy"
          @select="selectTask"
          @create="createTask"
          @delete="deleteTask"
          @open-settings="handleOpenSettings"
        />
        <ChatPane
          ref="chatPaneRef"
          :chat="workspace.chat"
          :live-turn="workspace.liveTurn"
          :task-id="activeTaskIdNumber"
          :selected-model="workspace.selectedModel"
          :working-directory="workspace.workingDirectory"
          :workspace-path="workspace.workspacePath"
          :settings-open="settingsOpen"
          :disabled="!activeTaskIdNumber"
          :sending="isActiveTaskSending"
          :interaction-locked="hasPendingSend"
          :cancelling="isActiveTaskCancelling"
          @send="sendMessage"
          @cancel-turn="cancelCurrentTurn"
          @open-files="openFilesFromComposer"
          @set-model="setTaskModel"
          @set-working-directory="setTaskWorkingDirectory"
        />
        <ContextPanel
          :notes="workspace.notes"
          :open-files="workspace.openFiles"
          :hints="workspace.hints"
          :skills="workspace.skills"
          :usage="workspace.contextUsage"
          :debug-rounds="workspace.debugRounds"
          :busy="busy"
          @add-note="addNote"
          @edit-note="editNote"
          @delete-note="deleteNote"
          @toggle-file-lock="toggleOpenFileLock"
          @close-file="closeOpenFile"
          @refresh-skills="refreshSkills"
        />
      </main>
    </div>

    <div
      v-if="settingsOpen"
      class="absolute inset-x-0 bottom-0 top-10 z-40 backdrop-blur-md"
      style="background: var(--ma-settings-backdrop)"
    >
      <SettingsPage
        :theme="theme"
        :settings="providerSettings"
        :busy="busy"
        :models-loading="providerModelsLoading"
        :available-models="providerModels"
        :suggested-models="providerSuggestedModels"
        :probe-models="providerProbeModels"
        :probe-suggested-models="providerProbeSuggestedModels"
        :probe-models-loading="providerProbeModelsLoading"
        :provider-test-message="providerTestMessage"
        :provider-test-success="providerTestSuccess"
        @close="closeSettings"
        @update-theme="setTheme"
        @save-provider="saveProvider"
        @test-provider="testProviderConnection"
        @delete-provider="confirmDeleteProvider"
        @save-default-provider="saveDefaultProvider"
        @request-models="loadProviderModelsForSettings"
        @request-probe-models="requestProbeModels"
      />
    </div>

    <NoteEditorDialog
      ref="noteDialogRef"
      :open="noteDialogOpen"
      :mode="noteDialogMode"
      :draft-id="noteDraftId"
      :draft-content="noteDraftContent"
      :busy="busy"
      @update:open="handleNoteDialogOpenChange"
      @update:draft-id="noteDraftId = $event"
      @update:draft-content="noteDraftContent = $event"
      @submit="handleSubmitNoteDialog"
      @cancel="closeNoteDialog"
    />

    <ConfirmActionDialog
      :open="confirmDialogOpen"
      :title="confirmDialogTitle"
      :description="confirmDialogDescription"
      :body="confirmDialogBody"
      :confirm-label="confirmDialogLabel"
      :busy="busy"
      @update:open="handleConfirmDialogOpenChange"
      @confirm="submitConfirmDialog"
    />
  </div>
</template>

<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue';
import AppTitleBar from '@/components/AppTitleBar.vue';
import ChatPane from '@/components/ChatPane.vue';
import ConfirmActionDialog from '@/components/ConfirmActionDialog.vue';
import ContextPanel from '@/components/ContextPanel.vue';
import NoteEditorDialog from '@/components/NoteEditorDialog.vue';
import SettingsPage from '@/components/SettingsPage.vue';
import TaskList from '@/components/TaskList.vue';
import { useWorkspaceApp } from '@/composables/useWorkspaceApp';

const workspaceApp = useWorkspaceApp();

const {
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
  setTaskWorkingDirectory,
  handleOpenSettings,
  closeSettings,
  setTheme,
  saveProvider,
  testProviderConnection,
  requestProbeModels,
  confirmDeleteProvider,
  saveDefaultProvider,
  loadProviderModelsForSettings,
  handleConfirmDialogOpenChange,
  submitConfirmDialog,
  minimizeWindow,
  toggleMaximize,
  closeWindow,
} = workspaceApp;

onMounted(() => {
  void initialize();
});

onUnmounted(() => {
  dispose();
});
</script>
