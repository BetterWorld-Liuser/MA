import { describe, expect, it, vi } from 'vitest';
import { ref } from 'vue';
import type { BackendWorkspaceSnapshot, DebugRoundItem, TaskActivityStatus } from '@/data/mock';
import { useTaskTimelineState } from './useTaskTimelineState';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue({
    timeline: [],
    last_seq: 0,
  }),
}));

describe('useTaskTimelineState', () => {
  it('keeps the local seq cursor for hydrated tasks so buffered events can replay on resubscribe', async () => {
    const initialSnapshot: BackendWorkspaceSnapshot = {
      tasks: [
        {
          id: 7,
          name: 'Task A',
          working_directory: 'd:/playground/MA',
          last_active: 1710000000,
        },
      ],
      active_task: {
        task: {
          id: 7,
          name: 'Task A',
          working_directory: 'd:/playground/MA',
        },
        last_seq: 4,
        timeline: [],
        open_files: [],
        notes: [],
        hints: [],
      },
    };
    const snapshot = ref<BackendWorkspaceSnapshot | null>(null);
    const activeTaskIdNumber = ref<number | null>(7);
    const taskActivityStatuses = ref<Record<number, TaskActivityStatus>>({});
    const runtimeCalls: number[] = [];
    const contextSyncCalls: number[] = [];
    const debugRounds: Array<{ taskId: number; round: DebugRoundItem }> = [];

    const state = useTaskTimelineState({
      snapshot,
      activeTaskIdNumber,
      taskActivityStatuses,
      setTaskRuntimeSnapshot: (taskId) => {
        runtimeCalls.push(taskId);
      },
      syncTaskContextSnapshot: (taskId) => {
        contextSyncCalls.push(taskId);
      },
      appendTaskDebugRound: (taskId, round) => {
        debugRounds.push({ taskId, round });
      },
    });

    state.hydrateTaskTimeline(7, [], 2);

    snapshot.value = initialSnapshot;
    state.hydrateTaskTimeline(7, [], 2);

    snapshot.value = {
      ...initialSnapshot,
      active_task: {
        ...initialSnapshot.active_task!,
        last_seq: 9,
      },
    } satisfies BackendWorkspaceSnapshot;

    await Promise.resolve();

    expect(state.getTaskLastSeq(7)).toBe(2);
    expect(contextSyncCalls).toContain(7);
    expect(runtimeCalls).toHaveLength(0);
    expect(debugRounds).toHaveLength(0);
  });
});