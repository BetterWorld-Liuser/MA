<template>
  <div class="relative h-screen overflow-hidden bg-bg text-text">
    <AppTitleBar
      :title="appTitle"
      :is-maximized="isMaximized"
      @minimize="minimizeWindow"
      @toggle-maximize="toggleMaximize"
      @close="closeWindow"
    />

    <div class="flex h-[calc(100%-2.5rem)] min-h-0 flex-col gap-2 px-0 py-0">
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

      <main class="workspace-shell flex min-h-0 gap-0">
        <div
          class="workspace-sidebar-shell"
          :style="{ width: leftSidebarVisible ? `${leftSidebarWidth}px` : `${collapsedRailWidth}px` }"
        >
          <TaskList
            v-if="leftSidebarVisible"
            title="任务"
            :tasks="taskListView.tasks"
            :active-task-id="taskListView.activeTaskId"
            :busy="busy"
            @select="selectTask"
            @create="createTask"
            @delete="deleteTask"
            @open-settings="handleOpenSettings"
            @collapse="leftSidebarVisible = false"
          />
          <div v-else class="sidebar-rail task-column-divider">
            <button
              class="sidebar-rail-button"
              type="button"
              title="展开任务栏"
              aria-label="展开任务栏"
              @click="leftSidebarVisible = true"
            >
              <Icon :icon="panelLeftOpenIcon" class="h-4 w-4" />
            </button>
            <button
              class="sidebar-rail-button mt-auto"
              type="button"
              title="打开设置"
              aria-label="打开设置"
              @click="handleOpenSettings"
            >
              <Icon :icon="settings2Icon" class="h-4 w-4" />
            </button>
          </div>
        </div>
        <div
          v-if="leftSidebarVisible"
          class="sidebar-resize-handle"
          role="separator"
          aria-label="调整任务栏宽度"
          aria-orientation="vertical"
          @pointerdown="startSidebarResize('left', $event)"
        ></div>
        <ChatPane
          ref="chatPaneRef"
          class="min-w-0 flex-1"
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
        <div
          v-if="rightSidebarVisible"
          class="sidebar-resize-handle"
          role="separator"
          aria-label="调整上下文栏宽度"
          aria-orientation="vertical"
          @pointerdown="startSidebarResize('right', $event)"
        ></div>
        <div
          class="workspace-sidebar-shell"
          :style="{ width: rightSidebarVisible ? `${rightSidebarWidth}px` : `${collapsedRailWidth}px` }"
        >
          <ContextPanel
            v-if="rightSidebarVisible"
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
            @collapse="rightSidebarVisible = false"
          />
          <div v-else class="sidebar-rail context-column-divider">
            <button
              class="sidebar-rail-button"
              type="button"
              title="展开上下文栏"
              aria-label="展开上下文栏"
              @click="rightSidebarVisible = true"
            >
              <Icon :icon="panelRightOpenIcon" class="h-4 w-4" />
            </button>
          </div>
        </div>
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
import { Icon } from '@iconify/vue';
import panelLeftOpenIcon from '@iconify-icons/lucide/panel-left-open';
import panelRightOpenIcon from '@iconify-icons/lucide/panel-right-open';
import settings2Icon from '@iconify-icons/lucide/settings-2';
import { onMounted, onUnmounted, ref, watch } from 'vue';
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
const collapsedRailWidth = 44;
const leftSidebarWidth = ref(loadSidebarWidth('left', 220));
const rightSidebarWidth = ref(loadSidebarWidth('right', 272));
const leftSidebarVisible = ref(loadSidebarVisibility('left', true));
const rightSidebarVisible = ref(loadSidebarVisibility('right', true));
let activeResize: { side: 'left' | 'right'; pointerId: number } | null = null;

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
  window.addEventListener('pointermove', handleSidebarResizeMove);
  window.addEventListener('pointerup', finishSidebarResize);
  window.addEventListener('pointercancel', finishSidebarResize);
  void initialize();
});

onUnmounted(() => {
  debugChat('app', 'unmounted', {
    appInstanceId,
  });
  void frontendDiagnosticLogger.info('app', 'unmounted', {
    appInstanceId,
  });
  window.removeEventListener('pointermove', handleSidebarResizeMove);
  window.removeEventListener('pointerup', finishSidebarResize);
  window.removeEventListener('pointercancel', finishSidebarResize);
  dispose();
});

function startSidebarResize(side: 'left' | 'right', event: PointerEvent) {
  activeResize = { side, pointerId: event.pointerId };
  document.body.classList.add('sidebar-resizing');
}

function handleSidebarResizeMove(event: PointerEvent) {
  if (!activeResize || activeResize.pointerId !== event.pointerId) {
    return;
  }

  if (activeResize.side === 'left') {
    const nextWidth = clamp(Math.round(event.clientX), 180, 360);
    leftSidebarWidth.value = nextWidth;
    persistSidebarWidth('left', nextWidth);
    return;
  }

  const nextWidth = clamp(Math.round(window.innerWidth - event.clientX), 240, 420);
  rightSidebarWidth.value = nextWidth;
  persistSidebarWidth('right', nextWidth);
}

function finishSidebarResize() {
  if (!activeResize) {
    return;
  }
  activeResize = null;
  document.body.classList.remove('sidebar-resizing');
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

function loadSidebarWidth(side: 'left' | 'right', fallback: number) {
  try {
    const raw = window.localStorage.getItem(`ma:sidebar-width:${side}`);
    const parsed = raw ? Number.parseInt(raw, 10) : Number.NaN;
    return Number.isFinite(parsed) ? parsed : fallback;
  } catch {
    return fallback;
  }
}

function persistSidebarWidth(side: 'left' | 'right', width: number) {
  try {
    window.localStorage.setItem(`ma:sidebar-width:${side}`, String(width));
  } catch {
    // Ignore persistence failures.
  }
}

function loadSidebarVisibility(side: 'left' | 'right', fallback: boolean) {
  try {
    const raw = window.localStorage.getItem(`ma:sidebar-visible:${side}`);
    if (raw === 'true') {
      return true;
    }
    if (raw === 'false') {
      return false;
    }
    return fallback;
  } catch {
    return fallback;
  }
}

function persistSidebarVisibility(side: 'left' | 'right', visible: boolean) {
  try {
    window.localStorage.setItem(`ma:sidebar-visible:${side}`, String(visible));
  } catch {
    // Ignore persistence failures.
  }
}

watch(leftSidebarVisible, (visible) => persistSidebarVisibility('left', visible), { immediate: true });
watch(rightSidebarVisible, (visible) => persistSidebarVisibility('right', visible), { immediate: true });
</script>
