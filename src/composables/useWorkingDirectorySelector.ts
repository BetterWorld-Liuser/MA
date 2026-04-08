import { computed, type Ref } from 'vue';
import { open as openPathDialog } from '@tauri-apps/plugin-dialog';
import { normalizePath } from './taskModelSelectorShared';

type UseWorkingDirectorySelectorOptions = {
  workingDirectory: Ref<string | undefined>;
  workspacePath: Ref<string | undefined>;
  emitSetWorkingDirectory: (path?: string | null) => void;
};

export function useWorkingDirectorySelector({
  workingDirectory,
  workspacePath,
  emitSetWorkingDirectory,
}: UseWorkingDirectorySelectorOptions) {
  const normalizedWorkspacePath = computed(() => normalizePath(workspacePath.value));
  const normalizedWorkingDirectory = computed(() => normalizePath(workingDirectory.value));
  const isCustomWorkingDirectory = computed(
    () =>
      !!normalizedWorkingDirectory.value
      && !!normalizedWorkspacePath.value
      && normalizedWorkingDirectory.value !== normalizedWorkspacePath.value,
  );
  const workingDirectoryLabel = computed(() => normalizedWorkingDirectory.value || '工作目录');
  const workingDirectoryTooltip = computed(() =>
    normalizedWorkingDirectory.value
      ? `AI 工作目录：${normalizedWorkingDirectory.value}`
      : '设置 AI 工作目录',
  );

  async function pickWorkingDirectory() {
    const selected = await openPathDialog({
      directory: true,
      multiple: false,
      defaultPath: workingDirectory.value || workspacePath.value,
      title: '选择 AI 工作目录',
    });
    if (!selected || Array.isArray(selected)) {
      return;
    }
    emitSetWorkingDirectory(selected);
  }

  function resetWorkingDirectory() {
    emitSetWorkingDirectory(null);
  }

  return {
    isCustomWorkingDirectory,
    workingDirectoryLabel,
    workingDirectoryTooltip,
    pickWorkingDirectory,
    resetWorkingDirectory,
  };
}
