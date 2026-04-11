# MEMORY 面板未保留本轮已激活记忆 根因分析

> 阶段:阶段二(根因分析)
> 状态:已确认
> 分析日期:2026-04-11
> 关联问题报告:`easysdd/issues/2026-04-11-memory-panel-does-not-persist-activated-memories/report.md`

## 1. 问题定位

历史 learnings 检索结果：未找到同类 pitfall 文档。

| 关键位置 | 说明 |
|---|---|
| `crates/march-core/src/agent/session.rs:226` | `build_context()` 每轮检索 memory，并把结果写进 `self.last_memory_index`。也就是说，流式回复阶段右侧看到的 memory 来自这个运行态缓存。 |
| `crates/march-core/src/agent/session.rs:99` | `AgentSession::create()` 初始化时把 `last_memory_index` 固定设为 `None`；`restore()` 走的也是这条路径，所以从持久化状态重建 session 时不会带回上一轮命中的 memory。 |
| `crates/march-core/src/ui/backend/messaging.rs:298` | `turn_finished` 收尾时，不是复用正在运行的 session，而是用 `restore_session_from_state(...)` 重建 `canonical_session`。 |
| `crates/march-core/src/ui/backend/messaging.rs:306` | 收尾事件发送的 `task` 快照来自 `live_task_snapshot(..., &canonical_session, ...)`；此时 `canonical_session` 的 `last_memory_index` 已经是空。 |
| `src/composables/workspaceApp/useWorkspaceSnapshotState.ts:214` | 前端收到 `turn_finished` / `round_complete` 携带的整份 `task` 后，会直接用 `syncTaskContextSnapshot()` 覆盖当前上下文 source。 |
| `src/data/mock.ts:826` | 右侧 `MEMORY` 区域完全来自 `activeTask.runtime.memories`；一旦收尾快照里的 `runtime.memories` 为空，面板就立即显示 `No matched memories`。 |

## 2. 失败路径还原

**正常路径**:
用户发送消息 → session 构建上下文时执行 memory search → `last_memory_index` 被写入 → 流式事件(`message_started` / `tool_started` / `assistant_stream_delta` / `message_finished`)都携带当前 session 的 `runtime` → 前端右侧 `MEMORY` 面板能显示本轮命中的 memory。

**失败路径**:
用户发送消息 → 流式阶段同样正确命中 memory → 一轮结束后，后端为发送 `round_complete` / `turn_finished` 快照，调用 `restore_session_from_state(...)` 重建了一个新的 session → 新 session 的 `last_memory_index` 在初始化时被置为 `None`，且没有重新执行一次 memory search → `live_task_snapshot()` 基于这个“空 memory 运行态”生成 `task.runtime.memories=[]` → 前端用这份整快照覆盖当前上下文 → 右侧 `MEMORY` 面板被清空。

**分叉点**:`crates/march-core/src/ui/backend/messaging.rs:298` — 收尾快照改为从“重建后的 session”生成，而这个 session 不包含流式阶段已经得到的 `last_memory_index`。

## 3. 根因

**根因类型**:状态污染 / 并发竞态

**根因描述**:
这不是 memory 检索本身失败，而是“用于 UI 展示的运行态 memory 索引”没有跨收尾快照保留下来。`last_memory_index` 属于 session 内存中的临时状态，只在 `build_context()` 时更新，不进入 `persisted_state()`。但收尾事件(`round_complete`、`turn_finished`)生成 UI 快照时，后端没有沿用刚跑完这一轮的 session，而是从持久化状态恢复出一个新的 session。这个新 session 默认 `last_memory_index = None`，导致最终快照里的 `runtime.memories` 为空，前端又把整份 task source 替换掉，于是用户看到“回复过程中有 memory，回复结束后消失”。

**是否有多个根因**:是

- 主根因：后端收尾快照丢失了 `last_memory_index` 这类非持久化运行态。
- 次根因：前端对 `turn_finished` / `round_complete` 的 `task` 采用整对象替换，没有对 runtime-only 字段做保留式合并，因此后端一旦漏带，这个字段就会被直接清空。

## 4. 影响面

- **影响范围**:不只影响 report 描述的单一场景；所有“依赖 `last_memory_index` 展示当前轮命中记忆”的收尾态都有同样风险。
- **潜在受害模块**:`MEMORY` 面板、`CONTEXT USAGE` 中的 memory token 统计、任何未来继续依赖 `UiRuntimeSnapshot.memories` 解释“本轮实际用了哪些记忆”的功能。
- **数据完整性风险**:无。持久化 memory 本身没有丢失，丢的是收尾后的可观测运行态。
- **严重程度复核**:维持 `P2`。它不破坏主对话和记忆存储，但会持续误导用户对“这一轮是否真的用了 memory”的判断。

## 5. 修复方案

### 方案 A: 后端在收尾快照中显式继承最后一次 memory index

- **做什么**:让 `round_complete` / `turn_finished` 生成快照时，不再丢弃正在运行 session 的 `last_memory_index`。可以通过两种落地方式实现：1) 直接用原 session 的 runtime 生成收尾快照；2) 在 `restore_session_from_state(...)` 后把最后一次 `memory_index` 显式注入 `canonical_session`。
- **优点**:根因最直接，语义最清楚。右侧 `MEMORY` 继续忠实反映“本轮实际激活过哪些 memory”，也符合架构里“工具结果/运行态在本轮收敛前可见”的设计。
- **缺点/风险**:需要梳理收尾阶段为什么当前必须 `restore_session_from_state(...)`；如果直接复用运行中的 session，要确认不会破坏多 agent 合并和持久化顺序。
- **影响面**:主要在 `crates/march-core/src/ui/backend/messaging.rs`、`crates/march-core/src/agent/session.rs`，可能需要给 `UiTaskSnapshot` 或 session 增加一个“继承最后 runtime 视图”的辅助接口。

### 方案 B: 前端对收尾快照的 runtime 做保留式合并

- **做什么**:在 `syncTaskContextSnapshot()` 或 `mergeTaskRuntimeSnapshot()` 附近增加字段级 merge 规则：如果新的 `task.runtime.memories` 为空且当前 task 仍处于刚结束本轮的 review 状态，就暂时保留上一份非空 memories。
- **优点**:改动集中在前端，验证速度快；还能顺手兜住其他“收尾快照漏带 runtime-only 字段”的类似问题。
- **缺点/风险**:这是 UI 兜底，不是根因修复。前端很难可靠区分“本轮真的没有 memory 命中”与“后端漏带了命中结果”，容易引入新的显示幻觉。
- **影响面**:`src/composables/workspaceApp/useWorkspaceSnapshotState.ts`、`src/data/mock.ts` 一带的快照合并逻辑。

### 方案 C: 把“本轮已激活 memories”提升为可持久化的 turn/task 事实

- **做什么**:把收尾时的 matched memory ids/title 摘要作为 turn 级或 task runtime 基线的一部分显式存进 `UiTaskSnapshot`/存储层，而不是完全依赖 `last_memory_index` 这种 session 内存字段。
- **优点**:模型更稳，未来做“回复后回看本轮 memory 使用情况”也更自然。
- **缺点/风险**:设计和改动范围最大，会触碰存储模型、事件协议和 UI 语义；对当前这个 bug 来说偏重。
- **影响面**:后端存储、UI 类型、事件模型、右栏展示全链路。

### 推荐方案

**推荐方案 A**，理由:它最直接修复真正的根因，改动范围也相对可控。问题的本质是“收尾快照丢了本轮最后一次 memory 运行态”，所以应该在后端把这份运行态完整带到收尾，而不是让前端猜测什么时候该保留旧值。  
如果实施时发现后端收尾链路短期内不方便改，再考虑追加方案 B 作为临时兜底，但不建议只做 B。

## 6. 修复记录

- 采用方案 A 落地：在 turn worker 收尾链路里把 `last_memory_index` 随 `RoundComplete` / `TurnFinished` 一起传回 UI backend，再在 `restore_session_from_state(...)` 之后显式恢复到 `canonical_session`，确保收尾快照继续携带本轮最后一次命中的 memory。
- 实际改动位置:
  - `crates/march-core/src/ui/backend/messaging.rs`：给 `TurnWorkerUpdate::RoundComplete` 和 `TurnExecutionOutcome::{Completed,Failed}` 增加 `memory_index` 字段；构建收尾 `task` 快照前恢复 `canonical_session.restore_last_memory_index(...)`。
  - `crates/march-core/src/agent/session.rs`：增加 `last_memory_index()` / `restore_last_memory_index(...)` 小接口，供收尾链路转交这份临时运行态。
- 验证:
  - 新增回归测试 `restored_session_can_reuse_last_memory_index_for_runtime_snapshot`
  - 命令: `cargo test -p march-core restored_session_can_reuse_last_memory_index_for_runtime_snapshot`
  - 结果: 通过

### 调试补充

- 首轮修复后，用户反馈现象仍然存在。复查发送完成链路发现：虽然 `turn_finished / round_complete` 已经保住了 `memory_index`，但 `handle_send_message()` 尾部还会再调用一次 `workspace_snapshot(Some(task_id))` 并返回最终 snapshot。
- 这份最终 snapshot 在 `crates/march-core/src/ui/backend/messaging.rs` 中使用了新的 `final_session = restore_session_from_state(...)`，却没有恢复 `last_memory_index`，所以最终返回给前端的整份 workspace 又把右侧 `MEMORY` 清空了一次。
- 追加修复：在 `handle_send_message()` 作用域维护 `last_memory_index`，并在生成最终 `runtime` 前执行 `final_session.restore_last_memory_index(last_memory_index)`。
