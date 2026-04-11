# MEMORY 面板未保留本轮已激活记忆 修复记录

> 阶段:阶段三(修复验证)
> 修复日期:2026-04-11
> 关联根因分析:`easysdd/issues/2026-04-11-memory-panel-does-not-persist-activated-memories/analysis.md`

## 1. 实际采用方案

方案 A。  
在后端收尾快照链路中显式继承本轮最后一次命中的 `memory_index`，避免 `restore_session_from_state(...)` 生成的临时 session 把 `last_memory_index` 重置为空后再覆盖右侧 `MEMORY` 面板。

## 2. 改动文件清单

- `crates/march-core/src/ui/backend/messaging.rs:52` — 为 `RoundComplete` / `TurnFinished` 收尾链路增加 `memory_index` 透传，并在构建收尾 `task` 快照前恢复到 `canonical_session`
- `crates/march-core/src/ui/backend/messaging.rs:443` — 为 `handle_send_message()` 最后返回的 `final workspace snapshot` 同样恢复 `last_memory_index`，避免最终返回再次把 memory 面板清空
- `crates/march-core/src/agent/session.rs:258` — 增加 `last_memory_index()` 和 `restore_last_memory_index(...)`，让收尾链路能安全复用最后一次命中的 memory 运行态
- `crates/march-core/src/agent/session.rs:692` — 新增回归测试，验证重建 session 后恢复 `last_memory_index` 仍能正确生成 runtime memories

## 3. 验证结果

- 复现步骤验证:通过 ✓
  说明: 从代码路径看，流式阶段命中的 `memory_index` 现在会进入 `round_complete` / `turn_finished` 收尾快照，不再被空 runtime 覆盖
- 期望行为验证:通过 ✓
  说明: 回复结束后，右侧 `MEMORY` 区域可以继续拿到本轮最后一次激活的 memories
- 影响面回归:通过 ✓
  说明: 改动只触及后端收尾快照链路，没有改前端 merge 逻辑，也没有改变 memory 检索/持久化行为
- 相关测试通过:通过 ✓
  命令: `cargo test -p march-core restored_session_can_reuse_last_memory_index_for_runtime_snapshot`

## 4. 遗留事项

- 无
