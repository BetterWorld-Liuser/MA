---
track: pitfall
date: 2026-04-11
slug: memory-runtime-snapshot-overwrite-at-turn-finish
component: march-core/ui-backend-messaging
severity: medium
tags: [memory, runtime, snapshot, turn-finish, workspace-snapshot, ui, overwrite, debugging]
---

## 问题
右侧 `MEMORY` 面板在回复生成过程中能看到命中的 memory，但回复结束后会被收尾 snapshot 清空。

## 症状
- 回复流式进行时，右侧 `MEMORY` 区域短暂显示本轮激活的 memory
- `turn_finished` 之后或整轮完全结束后，右侧又变成 `No matched memories`
- 第一轮修复看起来已经让收尾事件带上了 memory，但用户实测现象仍然完全一样

## 没用的做法
- 只检查流式阶段的 runtime 事件，确认“过程中有 memory”后就停止排查
- 只修 `turn_finished / round_complete` 事件返回的 task snapshot，默认以为这就是最后一次覆盖
- 把问题简单归因成“前端没保留状态”，没有继续追发送完成后的最终 snapshot

## 解法
沿完整发送链路检查“谁最后写进右栏上下文 source”：

1. 先修 `turn_finished / round_complete`，让它们从运行中的 session 继承 `last_memory_index`
2. 再检查 `handle_send_message()` 尾部是否还会额外返回一份 `workspace_snapshot`
3. 如果存在最终 snapshot，必须同样恢复本轮最后一次 `memory_index`，否则前面的修复会被最后这份 snapshot 再次覆盖

这次最终生效的改法是：在 `crates/march-core/src/ui/backend/messaging.rs` 的整个 `handle_send_message()` 作用域维护 `last_memory_index`，并在生成最终 `final_session` 的 runtime 前执行 `final_session.restore_last_memory_index(last_memory_index)`。

## 为什么有效
这个问题不是单点 bug，而是“同一轮结束时存在多次 snapshot 覆盖”。  
中间事件修好以后，最终返回给前端的 `workspace_snapshot` 仍然可能来自一个重新 `restore_session_from_state(...)` 的 session；如果这个新 session 没恢复 `last_memory_index`，它就会用空的 `runtime.memories` 再覆盖一次右栏状态。于是用户体感上就是“明明修了，但现象完全没变”。

## 预防
- 排查“流式阶段正常、回复结束后消失”的 UI 问题时，不要只看中间事件；必须把整轮结束前的所有 snapshot 出口按顺序列出来
- 对依赖 session 临时运行态的 UI 字段，不要只修一条收尾事件，要核对：
  - `round_complete`
  - `turn_finished`
  - `handle_send_message()` 或同等最终返回路径
  - 任何额外的 `refreshWorkspace` / `workspace_snapshot`
- 如果一个问题表现为“第一版修复看起来合理，但用户实测仍然完全一样”，优先怀疑还有第二次覆盖，而不是马上否定第一版根因

## 相关文档
- `easysdd/issues/2026-04-11-memory-panel-does-not-persist-activated-memories/report.md`
- `easysdd/issues/2026-04-11-memory-panel-does-not-persist-activated-memories/analysis.md`
- `easysdd/issues/2026-04-11-memory-panel-does-not-persist-activated-memories/fix-note.md`
