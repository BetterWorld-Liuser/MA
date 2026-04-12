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
      <div v-if="backendNotices.length || errorMessage" class="space-y-2">
        <div
          v-for="notice in backendNotices"
          :key="notice.id"
          class="flex items-start justify-between gap-3 rounded-xl px-4 py-3 text-sm text-text"
          :style="notice.level === 'error'
            ? 'background: rgba(224,82,82,0.06); border: 1px solid rgba(224,82,82,0.28);'
            : 'background: rgba(230,168,23,0.06); border: 1px solid rgba(230,168,23,0.24);'"
        >
          <div class="min-w-0">
            <p class="text-[11px] uppercase tracking-[0.12em]" :class="notice.level === 'error' ? 'text-error' : 'text-warning'">
              {{ notice.level }} · {{ notice.source }}
            </p>
            <p class="mt-1 whitespace-pre-wrap break-words text-sm text-text">{{ notice.message }}</p>
          </div>
          <button
            class="shrink-0 rounded-md px-2 py-1 text-[11px] text-text-dim transition hover:bg-bg-hover hover:text-text"
            type="button"
            @click="dismissBackendNotice(notice.id)"
          >
            关闭
          </button>
        </div>

        <div
          v-if="!backendNotices.length && errorMessage"
          class="bg-[rgba(224,82,82,0.06)] px-4 py-3 text-sm text-text"
          style="border-bottom: 1px solid rgba(224, 82, 82, 0.28)"
        >
          {{ errorMessage }}
        </div>
      </div>

      <main class="grid min-h-0 flex-1 overflow-hidden gap-0 lg:grid-cols-[256px_minmax(0,1fr)_332px]">
        <TaskList
          title="任务"
          :tasks="taskListView.tasks"
          :active-task-id="taskListView.activeTaskId"
          :busy="busy"
          @select="selectTask"
          @create="createTask"
          @delete="deleteTask"
          @open-settings="handleOpenSettings"
        />
        <ChatPane
          ref="chatPaneRef"
          :timeline="chatView.timeline"
          :task-id="activeTaskIdNumber"
          :selected-model="composerView.selectedModel"
          :selected-temperature="composerView.selectedTemperature"
          :selected-top-p="composerView.selectedTopP"
          :selected-presence-penalty="composerView.selectedPresencePenalty"
          :selected-frequency-penalty="composerView.selectedFrequencyPenalty"
          :selected-max-output-tokens="composerView.selectedMaxOutputTokens"
          :working-directory="composerView.workingDirectory"
          :workspace-path="composerView.workspacePath"
          :settings-open="settingsOpen"
          :disabled="!activeTaskIdNumber"
          :sending="isActiveTaskSending"
          :interaction-locked="isActiveTaskInteractionLocked"
          :cancelling="isActiveTaskCancelling"
          @send="sendMessage"
          @cancel-turn="cancelCurrentTurn"
          @set-model="setTaskModel"
          @set-model-settings="setTaskModelSettings"
          @set-working-directory="setTaskWorkingDirectory"
        />
        <ContextPanel
          :notes="contextView.notes"
          :open-files="contextView.openFiles"
          :working-directory="contextView.workingDirectory"
          :hints="contextView.hints"
          :skills="contextView.skills"
          :memories="contextView.memories"
          :memory-warnings="contextView.memoryWarnings"
          :usage="contextView.contextUsage"
          :debug-rounds="contextView.debugRounds"
          :busy="busy"
          @add-note="addNote"
          @edit-note="editNote"
          @delete-note="deleteNote"
          @add-memory="addMemory"
          @edit-memory="editMemory"
          @delete-memory="deleteMemory"
          @toggle-file-lock="toggleOpenFileLock"
          @close-file="closeOpenFile"
          @refresh-skills="refreshSkills"
          @open-skill="openFilesFromComposer([$event])"
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
        :probe-models="providerProbeModels"
        :probe-suggested-models="providerProbeSuggestedModels"
        :probe-models-loading="providerProbeModelsLoading"
        :provider-test-loading="providerTestLoading"
        :provider-test-message="providerTestMessage"
        :provider-test-success="providerTestSuccess"
        :memories="settingsMemories"
        :memories-loading="settingsMemoriesLoading"
        @close="closeSettings"
        @update-theme="setTheme"
        @save-provider="saveProvider"
        @save-provider-model="saveProviderModel"
        @save-agent="saveAgent"
        @test-provider="testProviderConnection"
        @delete-provider="confirmDeleteProvider"
        @delete-provider-model="deleteProviderModel"
        @create-memory="createMemoryFromSettings"
        @edit-memory="editMemoryFromSettings"
        @delete-memory="deleteMemoryFromSettings"
        @delete-agent="confirmDeleteAgent"
        @restore-march-prompt="restoreMarchPrompt"
        @save-default-model="saveDefaultModel"
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

    <MemoryEditorDialog
      ref="memoryDialogRef"
      :open="memoryDialogOpen"
      :mode="memoryDialogMode"
      :draft-id="memoryDraftId"
      :draft-type="memoryDraftType"
      :draft-topic="memoryDraftTopic"
      :draft-title="memoryDraftTitle"
      :draft-content="memoryDraftContent"
      :draft-tags="memoryDraftTags"
      :draft-scope="memoryDraftScope"
      :draft-level="memoryDraftLevel"
      :available-agents="providerSettings?.agents ?? []"
      :busy="busy"
      @update:open="handleMemoryDialogOpenChange"
      @update:draft-id="memoryDraftId = $event"
      @update:draft-type="memoryDraftType = $event"
      @update:draft-topic="memoryDraftTopic = $event"
      @update:draft-title="memoryDraftTitle = $event"
      @update:draft-content="memoryDraftContent = $event"
      @update:draft-tags="memoryDraftTags = $event"
      @update:draft-scope="memoryDraftScope = $event"
      @update:draft-level="memoryDraftLevel = $event"
      @submit="handleSubmitMemoryDialog"
      @cancel="closeMemoryDialog"
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
import MemoryEditorDialog from '@/components/MemoryEditorDialog.vue';
import NoteEditorDialog from '@/components/NoteEditorDialog.vue';
import SettingsPage from '@/components/SettingsPage.vue';
import TaskList from '@/components/TaskList.vue';
import { debugChat } from '@/lib/chatDebug';
import { frontendDiagnosticLogger } from '@/lib/frontendDiagnosticLogger';
import { useWorkspaceApp } from '@/composables/useWorkspaceApp';

const workspaceApp = useWorkspaceApp();
const appInstanceId = Math.random().toString(36).slice(2, 8);

const {
  appTitle,
  busy,
  errorMessage,
  backendNotices,
  isMaximized,
  chatPaneRef,
  noteDialogRef,
  memoryDialogRef,
  taskListView,
  chatView,
  composerView,
  contextView,
  activeTaskIdNumber,
  isActiveTaskInteractionLocked,
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
  saveProviderModel,
  saveAgent,
  testProviderConnection,
  deleteProviderModel,
  confirmDeleteAgent,
  restoreMarchPrompt,
  requestProbeModels,
  confirmDeleteProvider,
  saveDefaultModel,
  handleConfirmDialogOpenChange,
  submitConfirmDialog,
  minimizeWindow,
  toggleMaximize,
  closeWindow,
} = workspaceApp;

onMounted(() => {
  debugChat('app', 'mounted', {
    appInstanceId,
  });
  void frontendDiagnosticLogger.info('app', 'mounted', {
    appInstanceId,
  });
  void initialize();
});

onUnmounted(() => {
  debugChat('app', 'unmounted', {
    appInstanceId,
  });
  void frontendDiagnosticLogger.info('app', 'unmounted', {
    appInstanceId,
  });
  dispose();
});
</script>
