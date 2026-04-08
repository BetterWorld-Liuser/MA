import type { Ref } from 'vue';
import type { BackendWorkspaceSnapshot } from '@/data/mock';
import { humanizeError, type RunWorkspaceAction } from './types';

type CreateWorkspaceActionRunnerOptions = {
  busy: Ref<boolean>;
  errorMessage: Ref<string>;
  snapshot: Ref<BackendWorkspaceSnapshot | null>;
  optimisticTaskId: Ref<string | null>;
};

export function createWorkspaceActionRunner({
  busy,
  errorMessage,
  snapshot,
  optimisticTaskId,
}: CreateWorkspaceActionRunnerOptions): RunWorkspaceAction {
  return async (action) => {
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
  };
}
