export function addTaskLock(taskIds: Set<number>, taskId: number) {
  return new Set([...taskIds, taskId]);
}

export function removeTaskLock(taskIds: Set<number>, taskId: number) {
  if (!taskIds.has(taskId)) {
    return taskIds;
  }
  const next = new Set(taskIds);
  next.delete(taskId);
  return next;
}

export function hasTaskLock(taskIds: Set<number>, taskId: number) {
  return taskIds.has(taskId);
}

export function isTaskInteractionLocked(activeTaskId: number | null | undefined, sendingTaskIds: Set<number>) {
  return !!activeTaskId && sendingTaskIds.has(activeTaskId);
}

export function canSendForTask(taskId: number | null | undefined, sendingTaskIds: Set<number>) {
  return !!taskId && !sendingTaskIds.has(taskId);
}

export function canCancelForTask(
  taskId: number | null | undefined,
  sendingTaskIds: Set<number>,
  cancellingTaskIds: Set<number>,
) {
  return !!taskId && sendingTaskIds.has(taskId) && !cancellingTaskIds.has(taskId);
}