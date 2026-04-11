# 流式过程消息与最终消息均不显示 修复记录

> 阶段: 阶段三(修复验证)
> 修复日期: 2026-04-11
> 关联根因分析: `easysdd/issues/2026-04-11-streaming-and-final-message-not-visible/analysis.md`

## 1. 实际采用方案

方案 B。

在 `agent` 运行循环里，把最终 `response.content` 统一并入当前 message 的 timeline 文本流；如果前面已经流式收到了一部分正文，只补发缺失后缀；如果本轮完全没有流式 delta，则补发完整正文。这样流式、非流式和 fallback 最终都走同一条正文展示通道。

## 2. 改动文件清单

- `crates/march-core/src/agent/runner.rs` — 在 `message_finished` 之前补齐缺失的 assistant 文本 delta，避免最终正文只存在于 `response.content` 而未写入 timeline
- `crates/march-core/src/agent/runner.rs` — 删除运行时对 `FinalAssistantMessage` 事件的正文展示依赖
- `crates/march-core/src/agent/runner.rs` — 新增定向单测，覆盖补全文本 delta 的几个关键分支

## 3. 验证结果

- 复现步骤验证: 通过 `cargo check -p march-core` 验证修复代码可编译，且从代码路径上已覆盖 report 中描述的 non-streaming / fallback 正文丢失场景
- 期望行为验证: 当前实现保证最终正文会先进入 `message.timeline`，再 `message_finished`，符合“流式阶段可显示、结束后最终消息可显示”的期望通道设计
- 影响面回归: 本次仅改 `runner.rs` 的正文归并逻辑，未触碰前端渲染组件与持久化结构；理论上不会改变已有正常流式场景，只会为缺失场景补齐正文
- 相关测试:
  - `cargo check -p march-core` ✓
  - `cargo test -p march-core ...` ✗，被仓库既有测试夹具问题阻塞，失败点在 `crates/march-core/src/agent.rs`

## 4. 遗留事项

- `crates/march-core/src/agent.rs:467` 的测试夹具仍使用旧字段，导致 `cargo test -p march-core` 当前无法通过；建议后续单独提 issue 修复测试基建
- 前端真实界面回归尚未在桌面应用里手动走一遍；若你愿意，我下一步可以继续帮你跑一次实际 UI 冒烟验证
