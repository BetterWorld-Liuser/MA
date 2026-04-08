import { invoke } from '@tauri-apps/api/core';
import type { BackendWorkspaceSnapshot } from '@/data/mock';
import type { RunWorkspaceAction, WorkspaceSnapshotState } from './types';

type TaskModelSettings = {
  temperature?: number | null;
  topP?: number | null;
  presencePenalty?: number | null;
  frequencyPenalty?: number | null;
  maxOutputTokens?: number | null;
};

export function createTaskSettingsActions({
  workspaceState,
  runWorkspaceAction,
}: {
  workspaceState: WorkspaceSnapshotState;
  runWorkspaceAction: RunWorkspaceAction;
}) {
  const { snapshot, activeTaskIdNumber } = workspaceState;

  async function setTaskModel(selection: { modelConfigId: number }) {
    if (!activeTaskIdNumber.value) {
      return;
    }

    await runWorkspaceAction(async () => {
      snapshot.value = await invoke<BackendWorkspaceSnapshot>('set_task_model', {
        input: {
          taskId: activeTaskIdNumber.value,
          modelConfigId: selection.modelConfigId,
        },
      });
    });
  }

  async function setTaskModelSettings(settings: TaskModelSettings, busy: { value: boolean }) {
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

  async function setTaskWorkingDirectory(path: string | null | undefined, busy: { value: boolean }) {
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

  return {
    setTaskModel,
    setTaskModelSettings,
    setTaskWorkingDirectory,
  };
}
