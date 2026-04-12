# 切换 Task 后漏同步已完成 Turn 修复记录

> 路径:快速通道
> 修复日期:2026-04-12

## 1. 问题描述

task A 正在回复时切到 task B，过一段时间再切回 task A，前端仍显示 task A 处于回复中；但重启应用后可见后端其实已经完成该 turn。

## 2. 根因

`src/composables/workspaceApp/useTaskTimelineState.ts` 的 active-task 同步分支在 task 已 hydrate 过时，会用 `active_task.last_seq` 提前推进前端本地的 `sinceSeq`。但这个 `last_seq` 是事件序号，可能先于持久化 timeline 前进；切回 task 后，`subscribe_task` 会以被抬高过的 `sinceSeq` 订阅，结果把缓冲区里本应回放的 `turn_finished` / `message_finished` 跳过，前端就一直保留旧的 `streaming` 状态。初版修复进一步尝试按 snapshot 覆盖时间线，导致未持久化完成的本地用户消息也可能被一并覆盖。

## 3. 修复方案

不要在已 hydrate task 切回时用 `active_task.last_seq` 提前推进前端本地 `sinceSeq`。因为这个 `last_seq` 是事件序号，可能先于持久化 timeline 前进；如果前端在 `subscribe_task` 之前就把本地游标提到该值，后端缓冲区里的 `turn_finished` / `message_finished` 就不会被回放。最终修复改为：已 hydrate task 切回时仅同步上下文 source，不改本地时间线和 seq，交给 `subscribe_task` 回放缺失事件；若缓冲区不够，再走既有 `gap_too_large -> loadTaskHistory` 路径补历史。

## 4. 改动文件清单

- `src/composables/workspaceApp/useTaskTimelineState.ts` — 删除已 hydrate task 切回时对本地 `taskLastSeq` 的提前推进，保留本地游标，确保 `subscribe_task` 可以回放漏掉的终态事件。
- `src/composables/workspaceApp/useTaskTimelineState.test.ts` — 新增回归测试，覆盖“已 hydrate task 切回后不应因 snapshot `last_seq` 前进而抬高本地 seq 游标”这一场景。

## 5. 验证结果

- 自动化验证: `npm run test:unit -- useTaskTimelineState taskRunLocks` 通过。
- 类型检查: `npm run typecheck` 通过。
- 项目校验: `npm run check` 通过。
- 前端启动验证: 已成功打开 Vite 页面。
- 调试结论: 初版修复曾尝试按 snapshot `last_seq` 回填时间线，但会把未持久化完成的本地消息覆盖掉；现已移除该路径，改为保留本地游标并依赖事件回放。
- 人工界面复现: 已由用户确认“task A 回复中切到 task B，等待后切回 task A”场景修复生效，用户消息不再消失。

## 6. 遗留事项

- 建议后续补一轮 `failed` / `cancelled` 场景的桌面端手工回归，确认切走期间的非成功终态也能正确回放。
- 无其他范围外改动。