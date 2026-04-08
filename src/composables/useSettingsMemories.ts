import { ref, type Ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { BackendMemoryDetailView, BackendWorkspaceSnapshot } from '@/data/mock';

type RunWorkspaceAction = (action: () => Promise<void>) => Promise<boolean>;

type UseSettingsMemoriesOptions = {
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  activeTaskIdNumber: Readonly<Ref<number | null>>;
  runWorkspaceAction: RunWorkspaceAction;
  openCreateMemoryDialog: () => void;
  openEditMemoryDialog: (memory: BackendMemoryDetailView) => void;
};

export function useSettingsMemories({
  snapshot,
  activeTaskIdNumber,
  runWorkspaceAction,
  openCreateMemoryDialog,
  openEditMemoryDialog,
}: UseSettingsMemoriesOptions) {
  const settingsMemories = ref<BackendMemoryDetailView[]>([]);
  const settingsMemoriesLoading = ref(false);

  function removeMemoryFromSettingsList(memoryId: string) {
    settingsMemories.value = settingsMemories.value.filter((memory) => memory.id !== memoryId);
  }

  async function refreshSettingsMemories(taskId = activeTaskIdNumber.value) {
    if (!taskId) {
      settingsMemories.value = [];
      return;
    }

    settingsMemoriesLoading.value = true;
    try {
      settingsMemories.value = await invoke<BackendMemoryDetailView[]>('list_memories', {
        input: { taskId },
      });
    } catch (error) {
      console.warn('Failed to load settings memories.', error);
      settingsMemories.value = [];
    } finally {
      settingsMemoriesLoading.value = false;
    }
  }

  function createMemoryFromSettings() {
    openCreateMemoryDialog();
  }

  async function editMemoryFromSettings(memoryId: string) {
    const taskId = activeTaskIdNumber.value;
    if (!taskId) {
      return;
    }

    const memory = await invoke<BackendMemoryDetailView>('get_memory', {
      input: {
        taskId,
        id: memoryId,
      },
    });
    openEditMemoryDialog(memory);
  }

  async function deleteMemoryFromSettings(memoryId: string) {
    const taskId = activeTaskIdNumber.value;
    if (!taskId) {
      return;
    }

    const succeeded = await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('delete_memory', {
        input: {
          taskId,
          id: memoryId,
        },
      });
    });

    if (succeeded) {
      removeMemoryFromSettingsList(memoryId);
    }
  }

  return {
    settingsMemories,
    settingsMemoriesLoading,
    refreshSettingsMemories,
    removeMemoryFromSettingsList,
    createMemoryFromSettings,
    editMemoryFromSettings,
    deleteMemoryFromSettings,
  };
}
