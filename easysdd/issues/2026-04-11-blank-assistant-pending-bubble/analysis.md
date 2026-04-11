# AI 回复前出现空白 assistant 气泡 根因分析

> 阶段:阶段二（根因分析）
> 状态:草稿
> 分析日期:2026-04-11
> 关联问题报告:[report.md](./report.md)

## 1. 问题定位

历史 pitfall 检索结果：`easysdd/learnings/` 暂未搜到与“assistant 回复前空白占位气泡”直接对应的历史记录。

| 关键位置 | 说明 |
|---|---|
| `src/composables/chatRuntime/chatEventReducer.ts:26-44` | `turn_started` 会先创建一个 `state: 'streaming'` 且 `messages: []` 的 turn；紧接着 `message_started` 会插入一个 `timeline: []` 的空 assistant message。也就是说，在首个正文 delta 到达前，前端时间线里已经存在一个“合法但无内容”的 assistant 气泡数据。 |
| `src/components/chat/TurnGroup.vue:28-37` | assistant 气泡组件在 `entry.state === 'streaming'` 时会立刻渲染消息气泡和 `visibleMessages`；当 message 已存在但 `timeline` 为空时，这里不会显示任何文本、工具条目或占位内容。 |
| `src/components/chat/TimelineRenderer.vue:2-18` | `TimelineRenderer` 只渲染非空文本和 tool entry。`entries` 为空时会输出一个空容器，因此气泡内部保持空白。 |
| `src/components/chat/TurnGroup.vue:29-30` + `src/data/mock.ts:67` | UI 的确预留了 `statusLabel` 占位分支，但只有在 `!entry.messages.length` 时才会显示，而且项目内没有任何地方给 `statusLabel` 赋值，这个分支当前是死代码。 |
| `src/data/mock.ts:234-265` + `src/data/mock.ts:849-884` | 后端 timeline/snapshot 的 `turn` 结构里也没有 `status_label` 字段，前端的 `toTaskTimelineEntries()` 同样不会映射任何占位文案，说明这条链路从 schema 层就没有被真正打通。 |

## 2. 失败路径还原

**正常路径**：
用户发送消息 → 后端开始一个新 turn → 前端显示 assistant 正在回复的可感知占位（例如 typing / skeleton / “正在思考”）→ 首个正文 delta 到达 → 占位平滑切换成真实正文。

**失败路径**：
用户发送消息 → `turn_started` 先把一个 streaming turn 放进 timeline → `message_started` 再插入一个 `timeline: []` 的空 assistant message → `TurnGroup` 看到这是 streaming turn，于是立即渲染气泡外壳 → `TimelineRenderer` 收到空 `entries`，不会渲染任何内容 → 首个 `assistant_stream_delta` 到达前，用户看到一个空白小气泡。

**分叉点**：`src/composables/chatRuntime/chatEventReducer.ts:37-44` 与 `src/components/chat/TurnGroup.vue:33-37`

这里的数据状态和渲染策略发生了错配：
前者把“消息已开始但正文尚未到达”建模成一个空 message，后者又把这个中间态当作可直接展示的 assistant 内容气泡来渲染，但没有给出任何 loading fallback。

## 3. 根因

**根因类型**：状态建模 + 缺少防御

**根因描述**：
聊天运行时事件流把“assistant 消息已经开始，但还没有任何正文 / tool / reasoning 内容”表示成了一个空的 streaming message。这本身作为内部状态并不一定错误，但 `TurnGroup` 在渲染层没有区分“有 streaming message”和“有可展示内容”这两个概念，只要看到 streaming turn 就立刻渲染 assistant 气泡，而 `TimelineRenderer` 对空 timeline 又没有任何占位兜底，于是用户会在首个 delta 到达前看到一个空白气泡。

同时，代码里虽然已经为 `Turn.statusLabel` 预留了“无消息时显示状态文案”的接口，但这一字段既没有事件层赋值，也没有 snapshot schema 支持，导致本来可能承担占位职责的分支实际上永远不生效。这说明问题不是单一的样式缺失，而是“运行时状态建模”和“展示层 fallback”之间没有闭环。

**是否有多个根因**：是

1. **主根因**：`message_started` 把“尚无可展示内容”的 assistant message 提前放入 timeline，UI 将其直接展示为消息气泡。
2. **次根因**：`statusLabel` 占位链路只停留在类型和模板层，没有任何事件/数据来源去驱动它，导致空态没有 fallback。

## 4. 影响面

- **影响范围**：不仅影响报告里的场景，而是会影响所有基于当前事件流的 assistant 回复启动阶段；只要 `message_started` 先于首个正文 delta 到达，就会出现同样的空白气泡。
- **潜在受害模块**：聊天主时间线、turn 引用摘要（会短暂得到“1 条消息 / streaming”这类无信息摘要）、后续若复用 `TurnGroup`/`TimelineRenderer` 的其他 assistant 消息展示场景。
- **数据完整性风险**：无。问题是纯前端展示与状态建模错配，不会导致消息内容丢失或持久化损坏。
- **严重程度复核**：维持 `P2`。它不破坏核心消息收发，但稳定复现，影响每次对话的首屏感知，而且会让用户误判为 UI 渲染异常或空消息，属于明显可感知的交互质量问题。

## 5. 修复方案

### 方案 A：在 `TurnGroup` 为“空 streaming message”补一个显式占位态

- **做什么**：保留现有事件模型不变，只在 `TurnGroup` 增加一个判断：当 turn 处于 `streaming` 且当前所有 message 都还没有可展示内容（无 text / tool / reasoning）时，渲染一个占位内容，如 typing dots、骨架条或“正在思考”文案；一旦首个可展示条目到达，再切回 `TimelineRenderer`。
- **优点**：改动范围最小，直接命中用户可见问题；不需要改后端事件协议，也不影响历史 timeline/snapshot 结构。
- **缺点/风险**：会在组件层引入一段“是否有 renderable content”的额外判断；如果后面别的组件也消费同类空 message，仍可能各自重复写 fallback。
- **影响面**：主要是 `src/components/chat/TurnGroup.vue`，可能顺带抽一个判断 helper 到聊天渲染层。

### 方案 B：把占位语义正式纳入 turn/message 数据模型

- **做什么**：打通 `statusLabel`（或新增更明确的 pending phase 字段）这条链路，在事件 reducer 或后端 event schema 中显式表达“正在思考 / 正在启动 / 正在等待首个 token”，`TurnGroup` 统一根据该字段展示占位内容，而不是从空 message 反推。
- **优点**：语义最清晰，运行时状态与 UI 展示一致；后续如果要区分“思考中”“调用工具中”“等待模型首 token”也更容易扩展。
- **缺点/风险**：改动跨越类型、event reducer，若走后端 schema 还会涉及 Rust 事件结构和 snapshot 映射；相对方案 A 成本更高。
- **影响面**：`src/data/mock.ts`、`src/composables/chatRuntime/chatEventReducer.ts`、`src/components/chat/TurnGroup.vue`，以及可能的 `src-tauri` event/snapshot 结构。

### 方案 C：延迟渲染 assistant 气泡，直到出现首个可展示内容

- **做什么**：保持 timeline 内部仍可提前创建空 message，但 `TurnGroup`/`ChatMessageList` 在没有任何可展示内容前不渲染 assistant 气泡外壳；等首个 text/tool/reasoning 到达后再插入气泡。
- **优点**：可以完全消除“空白气泡”这一视觉问题。
- **缺点/风险**：用户在发送消息后的短时间内看不到 assistant 已开始工作的任何反馈，反而可能造成“没有响应”的感觉；与项目希望提供即时运行态反馈的方向不一致。
- **影响面**：`src/components/chat/TurnGroup.vue` 或 `src/components/ChatMessageList.vue` 的渲染门禁逻辑。

### 推荐方案

**推荐方案 A**，理由：它最直接命中当前根因中的“展示层缺少 fallback”这一主症状，改动范围最小，能最快修复用户可见问题，同时不需要先改后端协议。落地时建议顺手把“判断 turn 是否已有可展示内容”的逻辑抽成一个小 helper，避免以后在聊天区别处再重复判空。

如果你希望把这块运行时状态语义一次性做完整，而不是只修 UI 症状，那可以选 **方案 B**；但从当前 issue 的收敛性和风险控制看，A 更适合作为阶段三的修复方案。

## 6. 修复记录

- 2026-04-11：用户确认采用 **方案 A**。
- 实际修改：
  - `src/components/chat/TurnGroup.vue`：新增 `showStreamingPlaceholder` 与 `hasRenderableStreamingContent()`，当 streaming turn 还没有 reasoning / text / tool 内容时，显示“正在思考”占位，并阻止空 message 容器一起渲染。
  - `src/styles/main.css`：新增 assistant pending placeholder 样式与三点脉冲动画复用。
- 验证：
  - `npm run build` 通过（含 `typecheck`、`check:shadow-js`、`vite build`）。
  - 尚未做人工桌面端可视化验证；需要在实际聊天界面走一遍 report 里的复现步骤确认最终观感。
