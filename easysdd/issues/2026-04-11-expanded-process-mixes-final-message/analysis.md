# 展开过程后最终消息与过程消息视觉混在一起 根因分析

> 阶段: 阶段二(根因分析)
> 状态: 草稿
> 分析日期: 2026-04-11
> 关联问题报告: [report.md](/D:/playground/MA/easysdd/issues/2026-04-11-expanded-process-mixes-final-message/report.md:1)

## 1. 问题定位

历史 learnings 检索结果: 未找到同类 pitfall 记录。

| 关键位置 | 说明 |
|---|---|
| [src/components/chat/TurnGroup.vue:42](/D:/playground/MA/src/components/chat/TurnGroup.vue:42) | 展开态命中 `entry.state === 'streaming' \|\| expanded \|\| !canCollapse` 分支后，开始渲染 `visibleMessages` |
| [src/components/chat/TurnGroup.vue:99](/D:/playground/MA/src/components/chat/TurnGroup.vue:99) | `finalMessage` 被定义为 `props.entry.messages.at(-1)`，前端本来就把最后一条 message 当作最终消息 |
| [src/components/chat/TurnGroup.vue:102](/D:/playground/MA/src/components/chat/TurnGroup.vue:102) | `visibleMessages` 在 `expanded` 时直接返回 `props.entry.messages` 全量数组，而不是仅返回过程消息 |
| [src/components/chat/TurnGroup.vue:53](/D:/playground/MA/src/components/chat/TurnGroup.vue:53) | 折叠态会单独渲染“最终消息”区，说明组件本来承认“最终消息”和“过程消息”是两种展示语义 |
| [src/composables/chatRuntime/chatEventReducer.ts:37](/D:/playground/MA/src/composables/chatRuntime/chatEventReducer.ts:37) | 前端运行态会按 `message_started` 为同一个 turn 追加多条 `AssistantMessage`，而不是把所有内容混成一条 |
| [crates/march-core/src/ui/backend/messaging.rs:959](/D:/playground/MA/crates/march-core/src/ui/backend/messaging.rs:959) | 后端持久化时间线时，收到 `MessageStarted` 事件会向 turn 内 push 一条新的 `PersistedAssistantMessage` |
| [crates/march-core/src/agent/runner.rs:178](/D:/playground/MA/crates/march-core/src/agent/runner.rs:178) | agent loop 最后一轮无 tool call 时才生成最终消息，说明最终消息在后端语义上是独立 message，不是“过程字段的一部分” |
| [crates/march-core/src/storage/timeline.rs:80](/D:/playground/MA/crates/march-core/src/storage/timeline.rs:80) | `PersistedTurn` 明确持有 `messages: Vec<PersistedAssistantMessage>`，数据模型天然区分 turn 与其内部多条消息 |

## 2. 失败路径还原

**正常路径**:
用户触发一个会经历多轮 agent loop 的 turn -> 后端每轮通过 `MessageStarted` / `MessageFinished` 产生一条独立的 assistant message -> 前端折叠态把最后一条 message 视为 `finalMessage`，单独显示在“最终消息”区 -> 用户能理解“过程”和“最终结果”的边界。

**失败路径**:
用户触发同样的多轮 turn -> 后端仍然按多条 message 正常保存 -> 前端在折叠态边界正确 -> 但用户点击“X 个动作 ▸”后，`visibleMessages` 因为 `expanded === true` 直接返回整个 `props.entry.messages` -> 模板把最终消息和所有过程消息放进同一块展开区域连续渲染 -> “收起过程”按钮又挂在整个气泡最底部 -> 用户视觉上误以为最终消息也属于过程区。

**分叉点**: [src/components/chat/TurnGroup.vue:102](/D:/playground/MA/src/components/chat/TurnGroup.vue:102) — 展开态直接返回整个 `messages` 数组，导致“展开过程”在实现上变成了“展开整个 turn”。

## 3. 根因

**根因类型**: 逻辑错误

**根因描述**:
问题不在后端数据结构，而在前端展示语义。当前实现已经把 turn 内最后一条 message 约定为“最终消息”，并在折叠态按这个约定单独渲染；但展开态没有沿用同一语义边界，而是直接把 turn 下的所有 messages 全量展示。结果就是同一个组件在“折叠态”和“展开态”对 `messages` 数组采用了两套不一致的解释：折叠态把最后一条当最终消息，展开态却把它重新并入过程列表。这种前后展示语义不一致，造成了“最终消息被放进过程里”的视觉错觉。

**是否有多个根因**: 否。主根因就是 `TurnGroup.vue` 的展开态列表选择逻辑与组件自身的折叠态语义不一致。`收起过程` 按钮位于整块底部只是放大了这种错觉，不是独立根因。

## 4. 影响面

- **影响范围**: 影响所有 `messages.length > 1` 且 turn 已完成、因此可折叠的 assistant turn；尤其是先调用工具再给最终回复的常见场景
- **潜在受害模块**: 聊天主界面中所有复用 `TurnGroup.vue` 的 assistant turn 展示；如果未来引用摘要、导出聊天或回放 UI 复用同一“最终消息=最后一条 message”的约定，也容易被这类边界不一致影响
- **数据完整性风险**: 无。问题是展示语义错误，不会破坏持久化时间线或 turn/message 数据结构
- **严重程度复核**: 维持 P2。核心回复内容仍然存在，用户可以读到结果；但稳定误导用户理解“过程”和“最终回复”的关系，属于应修复的交互逻辑缺陷

## 5. 修复方案

### 方案 A: 展开态只展示过程消息，最终消息继续独立展示

- **做什么**: 在 `TurnGroup.vue` 里显式拆分 `processMessages = messages.slice(0, -1)` 与 `finalMessage = messages.at(-1)`；折叠态和展开态都使用同一分层语义。展开时只展开 `processMessages`，最终消息继续保留在独立“最终消息”区
- **优点**: 最贴合当前 UI 文案，“展开过程”名副其实；改动集中在前端展示层，直接命中根因；不需要改后端事件模型
- **缺点/风险**: 需要处理只有 1 条 message 的边界场景，避免空过程区；需要小心保持 streaming 态现有体验
- **影响面**: 主要触碰 [src/components/chat/TurnGroup.vue](/D:/playground/MA/src/components/chat/TurnGroup.vue:42)，可能少量调整相关样式或测试

### 方案 B: 展开态仍展示整条 turn，但给最终消息显式分区

- **做什么**: 保留展开态全量渲染，但在展开区域中为最后一条 `message` 加“最终消息”标题、分隔线或独立卡片，并把“收起过程”按钮移动到过程区尾部或标题旁
- **优点**: 视觉上改动较小，不需要改变当前 `visibleMessages` 的总体结构
- **缺点/风险**: “展开过程”实际上仍会展示最终消息，语义还是略别扭；实现上要在 `v-for` 中识别最后一条 message 并特判，模板更绕
- **影响面**: 主要是 [src/components/chat/TurnGroup.vue](/D:/playground/MA/src/components/chat/TurnGroup.vue:42) 的渲染分支与样式

### 方案 C: 改写交互语义，把按钮从“展开过程”改成“展开全部”

- **做什么**: 保留现有全量渲染逻辑，只把按钮文案和区块命名改成“展开全部 / 收起详情”等更符合现状的描述
- **优点**: 改动最小，几乎只动文案
- **缺点/风险**: 没有真正解决“过程”和“最终结果”边界不清的问题，只是把语义迁就实现；会弱化当前“最终消息”这一重要信息结构
- **影响面**: 文案和少量样式调整，功能逻辑几乎不动

### 推荐方案

**推荐方案 A**，理由: 它最直接修正当前的根因，也最符合组件已经存在的语义约定。后端和前端运行态都已经把 turn 内最后一条 message 当作最终消息，折叠态也按这个规则工作；因此最稳妥的做法不是重新解释数据，而是让展开态和折叠态遵守同一边界。这样改动范围小、副作用少，也不会牵动后端协议。

## 6. 修复记录

- 2026-04-11: 用户确认采用方案 A。
- 已在 [src/components/chat/TurnGroup.vue](/D:/playground/MA/src/components/chat/TurnGroup.vue:42) 落地定点修复：
  - 展开态改为只渲染 `processMessages`
  - `finalMessage` 继续在独立“最终消息”区渲染
  - 折叠态与展开态现在共享同一条“最后一条 message = 最终消息”的展示语义
- 验证情况:
  - `npm run build` 通过
  - 尚未完成桌面界面的人工交互复现验证，因此浏览器/界面级验证仍待补
