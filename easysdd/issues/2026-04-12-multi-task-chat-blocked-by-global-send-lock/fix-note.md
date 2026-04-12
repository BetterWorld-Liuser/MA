# 多 Task 对话被全局发送锁阻塞 修复记录

> 阶段:阶段三（修复验证）
> 修复日期:2026-04-12
> 处理路径:快速通道（根因已明确，未单独产出 report.md / analysis.md）

## 1. 实际采用方案

方案 A：把前端发送/取消状态从单个 `taskId` 重构为按 task 维度追踪的集合，并且只锁定当前 active task 的输入区。

## 2. 改动文件清单

- `src/composables/useWorkspaceApp.ts` — 将 `sendingTaskId` / `cancellingTaskId` 改为 `Set<number>`，派生出“当前 task 是否发送中 / 是否取消中 / 是否需要锁输入”的计算状态。
- `src/composables/workspaceApp/messageActions.ts` — 发送入口改为只拦截“当前 task 已在发送”的情况；删除 task、结束发送、取消发送时都按 task 粒度增删锁状态；取消逻辑改为优先针对当前 active task。
- `src/composables/workspaceApp/useWorkspaceTaskActions.ts` — 同步透传新的按 task 集合状态类型。
- `src/composables/workspaceApp/taskRunLocks.ts` — 把多 task 发送/取消锁的判定与集合操作提炼成纯函数，供运行时代码与单测共用。
- `src/composables/workspaceApp/taskRunLocks.test.ts` — 新增单测，覆盖“task A 发送中时 task B 仍可发送且不被错误锁定”以及“当前 task 的取消资格不会泄漏到别的 task”两个核心场景。
- `src/App.vue` — `ChatPane` 的 `interaction-locked` 改为只跟随当前 task，而不是任意 task 的全局 pending 状态。

## 3. 验证结果

- 根因验证：已确认原实现把“任意 task 正在发送”建模成全局互斥，导致切到另一个 task 时发送入口仍被锁死。
- 单元测试：`npm run test:unit` 通过，已覆盖“发送锁仅作用于当前 task”与“取消只作用于当前 active task”两条核心规则。
- 静态检查：`npm run typecheck` 通过。
- 项目校验：`npm run check` 通过，包含 `typecheck` 与 `check:shadow-js`。
- 人工界面验证：未执行。当前未启动桌面 UI 走实际点击验证，需要补一轮“task A 发送中时切到 task B 再发送”的手工确认。

## 4. 遗留事项

- 需要在桌面端验证以下场景：
  1. task A 发送中时，切到 task B 仍可发送。
  2. task A 与 task B 同时发送后，分别切回时都能看到正确的发送/取消状态。
  3. 当前 task 的“中断”按钮不会误取消另一个 task。
- 本次未改后端并发执行逻辑；后端本身按 task 维度维护 in-flight turn，未见需要调整的阻塞点。
