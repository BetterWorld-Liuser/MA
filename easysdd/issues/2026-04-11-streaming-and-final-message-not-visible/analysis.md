# 流式过程消息与最终消息均不显示 根因分析

> 阶段: 阶段二(根因分析)
> 状态: 草稿
> 分析日期: 2026-04-11
> 关联问题报告: `easysdd/issues/2026-04-11-streaming-and-final-message-not-visible/report.md`

## 1. 问题定位

本次未搜到同类历史 pitfall 记录。

| 关键位置 | 说明 |
|---|---|
| `crates/march-core/src/agent/runner.rs:125` | agent 运行时只把 `ProviderProgressEvent::ContentDelta` 转成 `AssistantTextPreview`；是否有正文进入 UI，取决于 provider 是否真的推送了 delta。 |
| `crates/march-core/src/agent/runner.rs:171` | 最终一轮无 tool call 时，最终文本来自 `response.content`，随后发出 `FinalAssistantMessage`。 |
| `crates/march-core/src/ui/backend/messaging.rs:677` | UI 后端在事件转发时直接忽略 `FinalAssistantMessage`，没有把最终文本转成前端可消费的 timeline 事件。 |
| `crates/march-core/src/ui/backend/messaging.rs:993` | 持久化 timeline 只处理 `AssistantTextPreview`，把文本写入 message.timeline；`FinalAssistantMessage` 被显式忽略。 |
| `crates/march-core/src/provider/execution.rs:99` | provider 可能直接走 non-streaming 模式。 |
| `crates/march-core/src/provider/execution.rs:165` | 流式失败后还会 fallback 到 non-streaming；这条路径不会产生 `ContentDelta`，但仍可能返回 `response.content`。 |
| `src/components/chat/TurnGroup.vue:33` | 聊天卡片只展示 `message.timeline` 中已有的文本/工具项；如果 timeline 没有 text，UI 只能显示外壳。 |
| `src/components/chat/TimelineRenderer.vue:5` | 文本 entry 为空时不会渲染任何 Markdown 内容，因此最终表现为“有卡片、无正文”。 |
| `easysdd/architecture/ui-events.md:334` | 架构明确规定 `assistant_text_preview` / `final_assistant_message` 已废弃，正文应由 `assistant_stream_delta + message_finished` 完整覆盖。 |

## 2. 失败路径还原

**正常路径**:
用户发送消息 → 后端创建 turn/message → provider 在流式阶段持续推送 content delta，或在结束时把最终文本补进同一条 message → 后端把文本写入该 message 的 timeline → 前端 `TurnGroup` / `TimelineRenderer` 从 `message.timeline` 渲染正文 → turn 结束后最后一条 message 作为“最终消息”显示。

**失败路径**:
用户发送消息 → 后端创建 turn/message → provider 没有稳定地产生 `ContentDelta`（例如直接 non-streaming、stream fallback、或文本只在最终 `response.content` 里出现）→ agent 只发出 `FinalAssistantMessage` 携带最终文本 → UI 后端和持久化层都忽略 `FinalAssistantMessage`，没有把文本写入 `message.timeline` → 前端收到的是“有 turn / 有 message / 可能有 tool，但 timeline 没有 text” → 聊天区只能显示“X 个动作 ▸ / 最终消息”等结构，正文为空。

**分叉点**: `crates/march-core/src/ui/backend/messaging.rs:677` 与 `crates/march-core/src/ui/backend/messaging.rs:1006` — 最终文本事件被显式忽略；同时正文写入逻辑只依赖 `AssistantTextPreview`，没有兜底处理 `response.content`。

## 3. 根因

**根因类型**: 数据格式 + 状态流错位

**根因描述**:
聊天运行态的架构已经迁移到“正文只通过 message timeline 进入前端”的模型，但当前实现仍保留了一条旧的最终消息通道：最终文本在 `agent/runner.rs` 中会以 `FinalAssistantMessage` 形式单独发出，而 UI/backend 与 persisted timeline 已经把这条事件当成废弃路径直接忽略。结果是，只要正文没有在更早的 `ContentDelta -> AssistantTextPreview` 阶段写进 timeline，最终文本就会彻底丢失。

这又被 provider 的交付模式放大了：`provider/execution.rs` 明确支持 non-streaming 模式和“流式失败后回退到 non-streaming”。这些路径仍可能返回完整的 `response.content`，但不会产生可供前端累加的流式 delta。于是正文既不会在流式阶段显示，也不会在最终阶段补上，形成用户看到的“整个过程都没消息正文”的现象。

**是否有多个根因**: 是

- **主根因**: 最终文本事件 `FinalAssistantMessage` 已被后端 UI 桥接层和持久化层忽略，最终内容没有落入 `message.timeline`
- **次根因**: 当前正文写入逻辑过度依赖 `ContentDelta/AssistantTextPreview`，没有对 non-streaming / fallback 返回的 `response.content` 做统一归并

## 4. 影响面

- **影响范围**: 不只影响报告截图中的单一场景；所有“正文未通过流式 delta 完整到达”的回复都会受影响
- **潜在受害模块**:
  - 普通聊天最终回复展示
  - tool loop 结束后的最后一条总结消息
  - 历史记录 hydrate（因为持久化 timeline 里本来就没写入正文）
  - 任何走 non-streaming 模式的 provider / 模型
  - 任何发生 streaming fallback 的请求
- **数据完整性风险**: 有。不是业务数据损坏，但聊天事实被错误持久化为“空最终消息”，会影响历史回放、引用回复、调试定位和后续分析
- **严重程度复核**: 维持 `P1`。这是核心聊天能力的严重可见性故障，且会跨 provider 交付路径出现；但它不直接破坏项目文件或业务数据，所以仍低于 P0

## 5. 修复方案

### 方案 A: 最小修补旧 final-message 通道

- **做什么**: 保留 `FinalAssistantMessage` 事件，在 `ui/backend/messaging.rs` 里补处理逻辑，把最终文本追加到目标 `message.timeline`；持久化层同步支持该事件
- **优点**: 改动最小，能快速恢复“最终消息不显示”
- **缺点/风险**: 继续保留已被架构文档废弃的旧事件名，协议债务还在；若同时又收到流式 delta，容易出现重复写入或双通道竞争
- **影响面**: 主要改 `crates/march-core/src/ui/backend/messaging.rs` 及相关事件类型，前端组件基本不动

### 方案 B: 统一正文入口到 message timeline（推荐）

- **做什么**: 按架构文档收口，废弃运行时对 `FinalAssistantMessage` 的依赖；在 `agent/runner.rs` / UI backend 桥接层中，保证最终 `response.content` 无论来自 streaming、non-streaming 还是 fallback，都会被归并成同一条 message 的文本 timeline（例如统一转成 `assistant_stream_delta { field: content }` 或等价的 persisted timeline 更新），然后再发 `message_finished`
- **优点**: 与 `ui-events.md` 保持一致，source of truth 单一；既修复最终消息，也修复 non-streaming / fallback 下的流式空白问题；后续前端无需理解两套正文来源
- **缺点/风险**: 改动比方案 A 大，需要仔细处理“已有 delta + 最终 content”的去重与拼接边界，避免重复文本
- **影响面**: `crates/march-core/src/agent/runner.rs`、`crates/march-core/src/ui/backend/messaging.rs`、可能还包括事件类型/测试；前端组件通常无需改动

### 方案 C: 用持久化快照在 turn 结束时兜底回填

- **做什么**: 保持现有事件流不大动，在 turn 结束或 round_complete 时，从 session/history 中提取最终 assistant 文本，再回填到 persisted timeline
- **优点**: 对运行时事件协议侵入较小，历史记录可被补全
- **缺点/风险**: 流式期间仍然看不到正文，用户体验只会“结束后补出来”；而且会重新引入 snapshot / runtime 双写，违反当前架构的单一写入源原则
- **影响面**: `agent/session.rs`、`ui/backend/messaging.rs`、持久化合并逻辑

### 推荐方案

**推荐方案 B**，理由: 它最直接地修复了真正的协议错位问题，也最符合 `easysdd/architecture/ui-events.md` 中“正文只通过 timeline 进入前端”的设计。相比方案 A，它不再依赖已废弃的 `FinalAssistantMessage` 事件；相比方案 C，它不会引入 snapshot 回填的第二写入源。只要在统一入口上处理好“delta 已到一部分、final content 再补齐”的去重规则，就能同时覆盖 streaming、non-streaming 和 fallback 三条路径。

## 6. 修复记录

- 已按**方案 B**实施修复
- 实际改动:
  - `crates/march-core/src/agent/runner.rs` — 在每轮 provider 返回后，把最终 `response.content` 与已流式送出的 `content_preview` 做前缀比对；若存在缺失后缀，则先补发同一条 `AssistantTextPreview`，再进入 `message_finished`
  - `crates/march-core/src/agent/runner.rs` — 停止让运行时 UI 依赖 `FinalAssistantMessage` 作为正文展示来源
  - `crates/march-core/src/agent/runner.rs` — 增加针对“全量补发 / 仅补缺失后缀 / 已一致 / 非前缀错位”的定向单测
- 验证情况:
  - `cargo check -p march-core` 通过
  - `cargo test -p march-core ...` 未能完成，因为仓库里已有测试夹具与当前 `TaskRecord` / `PersistedTask` 结构不一致，失败点在 `crates/march-core/src/agent.rs`
- 结论: 本次修复已把非流式与 fallback 场景下原本丢失的正文重新并入 message timeline，符合方案 B 的预期

> 顺手发现: `crates/march-core/src/agent.rs:467` 测试仍按旧 `TaskRecord/PersistedTask` 字段初始化，导致当前 `cargo test -p march-core` 无法通过。该问题不在本次修复范围内，可后续另开 issue。
