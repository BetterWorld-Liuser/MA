import { describe, expect, it } from 'vitest';
import {
  addTaskLock,
  canCancelForTask,
  canSendForTask,
  hasTaskLock,
  isTaskInteractionLocked,
  removeTaskLock,
} from './taskRunLocks';

describe('taskRunLocks', () => {
  it('keeps send and interaction locks scoped to the current task', () => {
    const sendingTaskIds = addTaskLock(new Set<number>(), 1);

    expect(hasTaskLock(sendingTaskIds, 1)).toBe(true);
    expect(isTaskInteractionLocked(1, sendingTaskIds)).toBe(true);

    expect(canSendForTask(2, sendingTaskIds)).toBe(true);
    expect(isTaskInteractionLocked(2, sendingTaskIds)).toBe(false);
    expect(canCancelForTask(2, sendingTaskIds, new Set<number>())).toBe(false);

    const sendingTwoTasks = addTaskLock(sendingTaskIds, 2);
    expect(canSendForTask(1, sendingTwoTasks)).toBe(false);
    expect(canSendForTask(2, sendingTwoTasks)).toBe(false);

    const cancellingTaskTwo = addTaskLock(new Set<number>(), 2);
    expect(canCancelForTask(2, sendingTwoTasks, cancellingTaskTwo)).toBe(false);
    expect(canCancelForTask(1, sendingTwoTasks, cancellingTaskTwo)).toBe(true);

    const unlockedTaskOne = removeTaskLock(sendingTwoTasks, 1);
    expect(hasTaskLock(unlockedTaskOne, 1)).toBe(false);
    expect(canSendForTask(1, unlockedTaskOne)).toBe(true);
    expect(hasTaskLock(unlockedTaskOne, 2)).toBe(true);
  });

  it('only allows cancellation for the active task that is still sending', () => {
    const sendingTaskIds = addTaskLock(addTaskLock(new Set<number>(), 1), 2);
    const cancellingTaskIds = addTaskLock(new Set<number>(), 2);

    expect(canCancelForTask(1, sendingTaskIds, cancellingTaskIds)).toBe(true);
    expect(canCancelForTask(2, sendingTaskIds, cancellingTaskIds)).toBe(false);
    expect(canCancelForTask(3, sendingTaskIds, cancellingTaskIds)).toBe(false);
    expect(canCancelForTask(null, sendingTaskIds, cancellingTaskIds)).toBe(false);

    const taskOneCancelling = addTaskLock(cancellingTaskIds, 1);
    expect(canCancelForTask(1, sendingTaskIds, taskOneCancelling)).toBe(false);

    const taskOneFinished = removeTaskLock(sendingTaskIds, 1);
    expect(canCancelForTask(1, taskOneFinished, cancellingTaskIds)).toBe(false);
    expect(canCancelForTask(2, taskOneFinished, new Set<number>())).toBe(true);
  });
});