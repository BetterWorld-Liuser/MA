# 聊天运行态改进方案

> 针对当前 `liveTurn` 双源架构与历史消息冗余 tools 字段的重构方案。
> 相关现状代码：`src/composables/useLiveTurns.ts`、`src/components/ChatMessageList.vue`。
> 相关设计文档：[ui-chat.md](ui-chat.md)、[ui-events.md](ui-events.md)。

---

## 零、术语约定

在进入问题诊断前先锁死词汇，避免 message / turn / delta / chunk / segment 混用。本节术语贯穿全文，出现分歧时以此为准。

### 层级

```
Task（任务 / 会话，持久化，task_id）
 └─ timeline: 按 seq 严格排序的条目列表 ─┐
       │                                  │
       ├─ UserMessage（用户消息，user_message_id）
       │     ├─ content（纯文本 / 附件）
       │     ├─ mentions: agent_id[]      ← 直接点名召唤的 agent 列表（@ 行为）
       │     └─ replies: ReplyRef[]       ← 引用的气泡列表（quote-reply 行为）
       │
       └─ Turn（一轮，turn_id）           ← 仅包含 AI 产出
             │  agent_id                  ← 该 turn 归属哪个 agent
             │  trigger: { kind, id, ... } ← 触发源指针
             │  state                     ← streaming / done / failed / cancelled
             └─ messages: Message[]
                   └─ Message（一次 LLM API 调用，message_id）
                        ├─ Reasoning（思考内容）
                        └─ Timeline（按事件到达顺序的混合序列）
                             ├─ TextChunk（正文片段）
                             └─ ToolCall（工具调用项，tool_call_id）
```

**关键关系**：
- **UserMessage 不属于任何 Turn，和 Turn 并列挂在 Task.timeline 下**。Turn 的语义是"某个 AI agent 在被触发后做的工作"，用户消息/父 turn 是触发器，不是工作本身。
- **Task.timeline 是按 `seq` 严格单调排序的扁平列表**，前端渲染时直接遍历。不做任何嵌套展示（子 turn 不缩进在父 turn 下）。
- Turn 通过 `trigger` 字段反向引用触发源。trigger 有两种类型：
  - `{ kind: 'user', id: user_message_id }` —— 被用户消息 @ 触发
  - `{ kind: 'turn', id: parent_turn_id }` —— 被另一个 turn 的最终回复中的 @ 触发
- **@ 不是工具，AI 无法在 turn 执行过程中主动调用**。只有当一个 turn 结束（`turn_finished`）后，系统扫描其最终回复文本中的 @ mention，才触发下一个 turn。因此 turn 内部纯粹是 AI 自己的思考过程（多轮 LLM 调用 + 工具执行），不会中途分叉出子 turn。
- Turn 的 `agent_id` 表示这一轮是哪个 agent 在工作。一个 task 内不同 turn 可归属不同 agent（teams / 多 agent 场景）。
- 一个 Task 累积多个 UserMessage 和多个 Turn；一个 Turn 含 1+ 条 Message；一条 Message 的内容由 reasoning + timeline 两部分构成。

### 激活与并发模型（Teams）

一条 UserMessage 可通过两条并列路径激活 agent：`mentions`（直接点名）和 `replies`（引用气泡）。激活器遵循以下规则：

- **激活列表 = mentions ∪ 从 replies 派生的 agent 列表**：
  1. 先取 `mentions` 数组中的所有 agent_id
  2. 再扫描 `replies`，每一项若 `kind === 'turn'` 则取其指向 Turn 的 `agent_id` 追加；`kind === 'user_message'` 的 reply 跳过（纯语境引用）
  3. 按上述顺序去重，得到最终激活列表
- 这样同一个 agent 既被 @ 又被 reply 只会激活一次；同时 mentions 优先于 replies 的顺序约定确保 seq 可预期
- **激活动作串行**：激活器是单线程的，按激活列表顺序逐个发出 `turn_started` 事件。这意味着 `turn_started` 的 seq 顺序与"mentions 先、replies 后"的派生顺序一一对应，**timeline 里 turn 卡片的视觉顺序是确定的**，不存在排序歧义。
- **执行并行 / 异步**：每个 turn 一旦激活即 fire-and-forget，独立运行。**任意时刻可能有多个 turn 同时处于 `streaming` 状态**，它们的 `message_*` / `tool_*` / `assistant_stream_delta` 事件在同一 task 的事件流里按 seq 交错到达。前端 reducer 必须靠事件自带的 `turn_id` / `message_id` 路由到正确的 turn，不能假设"当前只有一个 streaming turn"。
- **链式触发（后置，非嵌套）**：@ 不是工具，AI 在 turn 执行过程中无法主动触发其他 agent。链式触发发生在 turn 结束之后：某个 turn `turn_finished` 后，系统扫描其最终回复文本中的 @ mention，为每个被 @ 的 agent 发出新的 `turn_started`（trigger 指向父 turn）。新 turn 与当时仍在 streaming 的其他 turn 并发运行，但父 turn 自身已经结束。因此不存在"父 turn 的 tool call 等待子 turn"的场景。
- **折叠隔离**：每个 turn 的 `turn_finished` 独立触发自己的折叠。并发 streaming 的 turn 之间互不干扰。
- **取消粒度**：同时支持 `cancel_task(taskId)`（一刀切停掉该 task 内所有 streaming turn）和 `cancel_turn(turnId)`（只停指定 turn，其他 turn 不受影响）。每个 TurnGroup 卡片有自己的取消按钮；task 级取消入口在左栏 task 项或输入框底栏。
- **`task_working_changed` 语义**：working = "该 task 内存在 ≥1 个 `state='streaming'` 的 turn"。任何一个 turn 在跑，左栏指示就亮。

### 前端渲染约束

- **线性渲染**：`<ChatMessageList />` 按 `task.timeline` 顺序遍历，UserMessage 和 Turn 按 seq 交错。
- **不做并行可视化**：多个并发 streaming 的 turn 就是相邻几张独立卡片，各自 streaming 各自折叠。不引入 side-by-side、不引入嵌套缩进。
- **trigger 指针视觉化**：每个 TurnGroup 头部显示 "🤖 agent_name ← 来自 user / ← 来自 parent_agent"。点击指针可高亮/滚动到触发源条目，替代嵌套展示的"看清触发关系"需求。
- **UserMessage 的 replies 展示**：在用户消息气泡**顶部**内嵌渲染被引用气泡的缩略卡片（类似 Slack/Discord 的 reply-to 预览条），显示引用目标的类型、发送者、文本摘要。点击缩略卡片滚动并高亮定位到原气泡。多条 reply 纵向堆叠展示。
- **UserMessage 的 mentions 展示**：在用户消息气泡**content 文本内联**以高亮 `@agent_name` tag 形式展示（就是输入时的原始样式），不在气泡顶部另起区域。mentions 只指向 agent，不指向任何气泡，不需要缩略卡片。
- **引用/点名与触发的视觉联动**：`mentions` 和 `replies`（kind='turn'）共同驱动了下方相邻的几个 TurnGroup 卡片；UserMessage 气泡和这些 TurnGroup 之间可通过悬停共享高亮，帮助用户理解"我 @ 了谁 / 引用了谁 → 谁被激活"。

### 定义表

| 术语 | 定义 | 对应 OpenAI 字段 | 出现层 |
|---|---|---|---|
| **Task（任务 / 会话）** | 持久化的聊天会话。拥有稳定 `task_id`。包含一条 `timeline`——按 seq 严格单调排序的 UserMessage 与 Turn 扁平列表。 | — | 数据模型 + 协议 + 前端类型（持久化） |
| **UserMessage（用户消息）** | 用户提交的一次输入，平级挂在 Task.timeline 下，**不属于任何 Turn**。拥有稳定 `user_message_id`。带两个并列的激活字段：`mentions`（直接 @ 点名）与 `replies`（引用某条历史气泡）。见下 **Mention** / **Reply**。 | — | 数据模型 + 协议 + 前端类型 |
| **Mention（点名召唤）** | UserMessage 对某个 agent 的直接点名，字段 `mentions: agent_id[]`。语义是"我要这些 agent 响应我当前的消息"，**不绑定任何历史气泡**——用于冷启动（该 agent 之前没在 task 里出现过）或无需指定上下文气泡的场景。UI 触发形式是输入框里的 `@agent_name` 选择器。一条 UserMessage 可同时 mention 多个 agent。 | — | 数据模型 + 协议 + 前端类型 |
| **Reply（引用回复）** | UserMessage 对先前某条气泡的引用，语义上等同于 Slack / Discord 的 "reply to message"。类型 `ReplyRef = { kind: 'turn' \| 'user_message'; id: string }`。**绑定特定气泡**，用于"就这条展开讨论"的场景。UI 触发形式是在历史气泡里选中"引用回复"。一条 UserMessage 可同时引用多个气泡。`kind === 'turn'` 的 reply 驱动对应 agent 激活；`kind === 'user_message'` 的 reply 仅作语境引用，不驱动激活。 | — | 数据模型 + 协议 + 前端类型 |
| **Turn（一轮）** | 一次触发产生的完整 AI 工作周期。拥有稳定 `turn_id`、`agent_id`、`trigger` 三个字段：`agent_id` 表明归属哪个 agent；`trigger` 反向引用触发源（`{kind:'user', id}` 或 `{kind:'turn', id}`）。Turn 内部纯粹是 AI 自己的思考过程（多轮 LLM 调用 + 工具执行），不会中途分叉出子 turn。链式触发发生在 turn 结束后：系统扫描最终回复中的 @ 为被 @ 的 agent 创建新 turn。一轮内含 1+ 条 Message。**同时激活的多个 turn 并发执行，激活顺序由 seq 确定**。 | — | 数据模型 + 协议 + 前端类型（后端分配） |
| **Agent** | Teams 场景下的一个 AI 角色，拥有稳定 `agent_id`。一个 task 内可有多个 agent；同一 task 的不同 Turn 可归属不同 agent；同一 agent 可在同一 task 的多个 turn 里出现。**Turn 是 agent 工作的载体**，agent 自身配置走独立持久化结构，不在本方案讨论范围。 | — | 仅 `agent_id` 进聊天数据模型 |
| **Trigger（触发指针）** | Turn 的 `trigger` 字段，两种判别：`{kind:'user', id:user_message_id}` 或 `{kind:'turn', id:parent_turn_id}`。纯数据引用，不影响执行调度。链式触发（kind='turn'）发生在父 turn 结束后，系统扫描最终回复中的 @ 自动创建。**Turn 的 trigger 和 UserMessage 的 replies 是两个独立字段**——trigger 记录"谁激活了这个 turn"，replies 记录"这条用户消息引用了哪些气泡作为语境"，语义层级不同。 | — | 数据模型字段 |
| **Message（一条消息）** | **一次 LLM API 调用的完整输出**，属于某个 turn，拥有稳定的 `message_id` 和 `state`。同一条 message 可同时持有 reasoning、content、tool_calls 三块内容——这是 API 协议的自然边界，前端不得伪造 message 切分。 | 整个 assistant message | 数据模型 + 协议 + 前端类型 |
| **Reasoning** | message 的思考字段，仅 reasoning 系模型产出。单段累加，不与 tool 交错。 | `reasoning_content` | 数据模型字段 |
| **Timeline** | message 内**按事件到达顺序**存储的混合序列，元素为 `TextChunk` 或 `ToolCall`。content 的 delta 到达时若末尾是 `TextChunk` 则追加，否则新建一段；`tool_started` 直接 push 一个 `ToolCall` entry。渲染即按 timeline 顺序平铺，保证 tool 前后的正文各成一段，语义上就是两段。 | 派生自 `content` + `tool_calls` 的到达顺序 | 数据模型字段 |
| **ToolCall** | message timeline 内的一个工具执行项，有稳定 `tool_call_id`。 | `tool_calls[i]` | 数据模型 + 协议 |
| **Delta（增量片段）** | 单次流式推送的增量，**按字段枚举区分**归属哪一块。纯传输概念，不落库、不进前端类型。 | `delta.{reasoning_content,content,tool_calls[i].function.arguments}` | 仅协议事件 |

### 生命周期事件归属

事件名带 `task_` / `turn_` / `message_` 前缀，**前缀即归属层级**，不要混淆。每一层的起止事件职责分明，不得跨层借用。

| 事件 | 层级 | 含义 | 不是什么 |
|---|---|---|---|
| `user_message_appended` | UserMessage | 后端确认用户消息已接收。携带 `user_message_id`（正式 id）、`mentions: agent_id[]`、`replies: ReplyRef[]`。前端用正式 id 替换乐观插入时的临时 id，补上 seq。紧接着会按"mentions 先、replies 后、去重"的激活列表收到若干 `turn_started`。 | 不是 turn 开始 |
| `turn_started` | Turn | 一个新 turn 被激活。携带 `turn_id`、`agent_id`、`trigger: { kind, id }`。触发来源：用户消息的 @/reply（`kind:'user'`）或父 turn 结束后最终回复中的 @（`kind:'turn'`）。前端在 Task.timeline 末尾 push 一个空 Turn 条目，初始 `state='streaming'`、`messages=[]`。**激活器串行发出 turn_started，seq 顺序 = @ 顺序，视觉顺序确定**。 | 不代表立即开始接收 message 事件；turn 一旦激活即并发执行 |
| `turn_finished` | Turn | 某个具体 turn 的结束信号。携带 `turn_id`、`reason: 'idle' \| 'failed' \| 'cancelled'`、可选 `error_message`。**这是该 turn 折叠的唯一触发时机**。并发场景下，不同 turn 的 `turn_finished` 可按任意顺序到达，各自独立触发折叠，互不影响。 | 不代表其他 turn 也结束；不代表 task 空闲（task 空闲需所有 turn 都 finished） |
| `message_started` | Message | 一次 LLM API 调用开始流式返回。携带 `turn_id`——**reducer 按 turn_id 路由到对应 Turn**，不能假设"追加到最后一个 streaming turn"。在目标 Turn 的 `messages` 末尾 push 一条新 Message。 | 不代表 turn 开始；不代表用户消息到达 |
| `message_finished` | Message | 当前这条 message 的流式输出结束，`state → 'done'`。仅针对**单条 message**（一次 API 调用）。 | **不代表 turn 结束**；折叠 / 摘要 / 分割线**都不由它触发** |
| `tool_started` / `tool_finished` | ToolCall（隶属 Message） | 具体工具调用的起止。通过 `message_id` 路由到 Message，再追加/更新到其 timeline。 | — |
| `assistant_stream_delta` | Message 内字段增量 | 按 `field` 枚举分派到 reasoning / content / tool_call_arguments，通过 `message_id` 路由到目标 Message。 | 不是"一条消息"，是消息内的增量片段 |
| `task_working_changed` | Task（全局轻量通道） | 全局轻量通道广播 `{ task_id, working: bool }`，**working = 该 task 内存在 ≥1 个 state=streaming 的 turn**。任何一个并发 turn 开始或最后一个并发 turn 结束时触发边沿。用于左栏 task 列表的旋转图标和未读标记。走独立订阅通道，所有 task 常驻订阅。 | 不承载消息内容，中栏 reducer 不消费此事件 |

**废弃事件**（不再使用，从协议中删除）：

- ~~`task_idle`~~ → 被 `turn_finished { reason: 'idle' }` 取代
- ~~`task_failed`~~ → 被 `turn_finished { reason: 'failed' }` 取代
- ~~`task_cancelled`~~ → 被 `turn_finished { reason: 'cancelled' }` 取代

废弃理由：早期方案没有 turn 概念，借 `task_*` 事件表达 turn 结束。现在 turn 是一等公民且有 id，turn 的起止必须用 turn 级事件才能语义对称。Task 层只保留 `task_working_changed` 这个全局轻量通道事件。

**一图记住 `message_finished` vs `turn_finished`**：

```
user_message_appended  ───────→ Task.entries push UserMessage
turn_started                 ─→ Task.entries push 空 Turn（streaming）
  │
  ├─ message_started (LLM call #1)
  │    ├─ assistant_stream_delta × N
  │    ├─ tool_started / tool_finished × M
  │    └─ message_finished         ← 第一条 message 结束，turn 还没结束
  │
  ├─ message_started (LLM call #2，工具结果回传后的最终回复)
  │    ├─ assistant_stream_delta × N
  │    └─ message_finished         ← 第二条 message 结束，turn 也要结束了
  │
  └─ turn_finished { reason: 'idle' }   ← Turn 真正结束，触发折叠
```

`message_finished` 只说"这一次 API 调用的流式输出结束了"，不说"AI 完成了当前工作"。`turn_finished` 才说后者。折叠、「N 个动作」摘要、分割线——全部由 `turn_finished` 触发，不由任何 `message_finished` 触发。

### 关键推论

- **一次 API 调用 = 一条 Message**。一轮里调 3 次 LLM（例如 `tool_calls` → 工具执行 → 再调 LLM 拿最终回复）就是 3 条 Message，数据模型与调用次数一一对应。不再单独引入 `LLMInvocation` 概念。
- **"思考"不是独立消息**，是 message 内的 reasoning 字段。流式期间 reasoning delta 与 content delta 可能交替到达，前端按字段分别累加到同一条 message 上。
- **折叠单位是 Turn，不是 Message**。每个 Turn 内所有 intermediate message 的 reasoning / timeline 按事件时序平铺，视觉上是一条连续的"操作流水"。最终 message 由后验方式识别：`turn_finished` 到达时，该 turn 的最后一条 message 即为最终 message。折叠发生在 `turn_finished` 时刻——之前的 intermediate 内容整体折叠成一行摘要（"N 个动作 ▸"，N 只统计 ToolCall 个数），最终 message 保持展开在分割线下方。流式期间不做任何预测性分割，全部平铺。
- **Turn 之间折叠完全独立**。并发 streaming 的多个 turn 各自 streaming 各自 `turn_finished` 各自折叠，不存在跨 turn 的折叠单元。
- **"完整拼完的一条消息"不引入新术语**,就是 `message.state === 'done'` 的 Message。口语可说"完整消息"，文档/代码统一用 `Message` + state。

### 禁用词

以下词语不再在方案、协议、代码中使用，避免混淆：

- ~~chunk~~ —— 与 HTTP chunk 混淆
- ~~segment~~ —— 语义太宽
- ~~preview~~ —— 旧 `assistant_text_preview`，已被 stream delta 取代
- ~~liveTurn~~ —— 本方案要删除的中间结构
- ~~checkpoint~~ —— 旧 `assistant_message_checkpoint`，语义已被 `message_started` 取代
- ~~LLMInvocation~~ —— 与 Message 重复
- ~~intermediate message / final message~~ —— 不是模型字段，只是 turn 内的后验位置描述。代码里不要出现 `is_final` / `role_in_turn` 这类字段，由"该 message 是否为 turn 的最后一条"直接判断

---

## 一、问题诊断

### 1. 历史消息携带轮内工具运行结果，与运行时事件重复
- `final_assistant_message` 事件把 `assistant_message` 整条 append 到历史，其中 `tools` 字段在 `ChatMessageList.vue` 的 `<details class="message-tools">` 折叠面板里再次渲染。
- 同一轮的工具调用在 `liveTurn` 期间已经实时显示过，沉淀后又被塞进历史消息一次。
- 违反 ui-events.md「运行中消费实时事件，结束后只持久化简短摘要」的边界。

### 2. 流式语义错位：过程态与最终回复并排长期共存
- `liveTurn` 一旦有 `content`，工具列表与正文同时显示，没有「过程态收起 → 最终回复展开」的视觉切换。
- `assistant_message_checkpoint(intermediate)` 走的是「归档为独立历史条目」路径，把"过程态坍缩为最终态"误解成了"保留中间产物"。

### 3. liveTurn 架构本身的复杂度溢出
- chatRuntime 与 task snapshot 双写最终消息：`appendTaskChatMessage` + `mergeActiveTaskSnapshot` 同时写入，靠 `historyTurnKey` 字符串拼接做去重，脆弱。
- `closedLiveTurnIds` LRU 黑名单存在，是因为事件没有 `seq` 字段、晚到事件无法靠顺序判断。
- `transitionKey` 手工驱动 `<Transition>` 重渲染，是因为视图模型没有按 phase 派生。
- `useLiveTurns.ts` 单文件 535 行，承担 reducer / store / activity / 归档 / snapshot 合并 / 日志节流多种职责。
- `assistant_text_preview` 名义是 preview 实际是 snapshot 全量替换，命名违反 AGENTS.md 流式输出约束。

---

## 二、目标模型

**中栏只有一种数据结构：`TaskTimelineEntry[]`（= `UserMessage | Turn`）。没有 liveTurn。**

用户消息和 AI 工作周期平级挂在 Task.timeline 下，按 seq 严格排序。所有运行时事件直接 mutate timeline 中的对应条目：

| 事件 | 作用 |
|------|------|
| `user_message_appended` | timeline 末尾 push 一条 UserMessage |
| `turn_started` | timeline 末尾 push 一个空 Turn（state=streaming），带 agent_id、trigger |
| `message_started` | 按 turn_id 路由到目标 Turn，push 一条新 Message |
| `tool_started` | 按 message_id 路由到目标 Message，timeline 内 push 占位 ToolCall |
| `tool_finished` | 按 tool_call_id 路由到目标 ToolCall，更新 status / preview |
| `assistant_stream_delta` | 按 message_id 路由到目标 Message，按 field 分派到 reasoning / content / tool_call_arguments |
| `message_finished` | 按 message_id 路由到目标 Message，state → done |
| `turn_finished` | 按 turn_id 路由到目标 Turn，state → done / failed / cancelled。**这是该 turn 折叠的唯一触发时机**，并发 turn 互不影响 |

**核心约束**：
- 没有「沉淀动画」——Turn 从 `turn_started` 起就在 timeline 里，不存在"从临时结构移动到历史"的过渡。
- 没有双源——`TaskTimelineEntry[]` 是中栏唯一 source of truth。
- 没有最终消息双写——`final_assistant_message` 事件被废弃，运行时事件已经把内容写完了。
- **事件路由完全由 id 驱动**——reducer 不假设"当前只有一个 streaming turn"，并发场景下靠 turn_id / message_id / tool_call_id 路由到正确位置。

---

## 三、关键交互：「Turn 平铺流水，turn_finished 后折叠过程」

折叠单位是 **turn**，由前端视图层在 `turn_finished` 到达时后验完成。Turn 本身由后端分配 `turn_id`、`agent_id`、`trigger` 三个字段，前端 reducer 按 `turn_id` 路由事件。**并发场景下，同一 task 可同时存在多个 streaming turn，各自独立 streaming、各自独立折叠**。

### 流式期间（turn state = 'streaming'）

Turn 内所有消息的 `reasoning` 与 `timeline` 按事件时序**完全平铺**：

- TurnGroup 顶部 header 显示 `🤖 {agent_name} ← 被 @{trigger_label} 触发`，trigger label 可点击定位到触发源
- 每条 intermediate message 的 reasoning 作为一块灰色解释性文本显示
- 每条 intermediate message 的 timeline 展开渲染，`TextChunk` 和 `ToolCall` 按顺序交错呈现
- 用户看到的是一条连续的"操作流水"："已运行 cargo fmt / 已编辑 shells.rs / 前端类型检查已经过了... / 已运行 cargo check"
- 不做任何预测性分割——不画"最终消息"分割线，不折叠任何已完成的 message，不在 message 之间插入视觉边界

这一阶段的视觉模型就是一条无缝时间线，用户自然能读"这个 agent 做了这些事"。并发场景下，多个 TurnGroup 卡片各自独立完成上述渲染，互不干扰。

### Turn 结束（`turn_finished` 到达）

目标 turn 收到 `turn_finished { reason: 'idle' }` 时，标记该 turn 的最后一条 message 为最终 message，随后前端对这个 turn 执行折叠（不影响其他并发 turn）：

1. **最终 message** = 该 turn 内 `message_id` 最大的那条（即最后一条）
2. 最终 message 之前的所有 intermediate 内容（包括所有 intermediate message 的 reasoning / timeline）**折叠**为一行摘要：「N 个动作 ▸」
   - **N 只统计 ToolCall 个数**，reasoning 和 intermediate content 不计入
   - 点击可展开恢复为流式期间的平铺视图
3. 摘要与最终 message 之间画一条 `── 最终消息 ──` 分割线
4. 最终 message 保持展开显示

### 边界情况

- **Turn 内只有一条 message**（用户问候、简单问答）：没有 intermediate 内容，不画分割线、不出折叠摘要，整个 turn 就是一条普通 message。
- **最终 message 本身也含 tool_calls**：仍然按"最后一条即最终"处理。该 message 自己的 tool_calls 展开显示在分割线下方。（实际中这种情况罕见——如果 LLM 还在调工具就不会 `turn_finished { reason: 'idle' }`。）
- **Turn 被 `turn_finished { reason: 'failed' | 'cancelled' }` 收尾**：同样触发折叠，折叠摘要后方显示 `error_message`，无分割线、无"最终消息"概念。
- **历史 turn 加载时**：任何 `state !== 'streaming'` 的 turn 直接以折叠态呈现，不回放流式过程。
- **并发 turn 的折叠顺序**：多个并发 streaming turn 各自的 `turn_finished` 按实际完成顺序到达，各自独立折叠。例如用户 @A、@B、@C 三个 agent，若 C 先完成则 C 的卡片先折叠，A、B 保持 streaming。

### 为什么选后验折叠

之前考虑过"后端提示 `is_likely_final`"和"前端启发式"两种方案，均会在 chain-of-tools 场景下导致分割线闪烁回退。后验折叠代价仅是"流式期间无视觉分层"，但换来：
- 协议无需 `is_likely_final` 字段
- 前端无需启发式，无需撤回动画
- 流式过程观感就是"一条干净的操作流水"，用户注意力天然集中在最新动作上

---

## 四、后端事件协议改动

### 改名 / 语义对齐

| 旧 | 新 | 说明 |
|----|----|------|
| `assistant_text_preview` | `assistant_stream_delta` | 改为真正的增量 delta，按 `field` 枚举区分归属字段（reasoning / content / tool_call_arguments），前端按字段分别累加 |
| `final_assistant_message` | 删除 | 已被 stream delta + `message_finished` 完整覆盖 |
| `assistant_message_checkpoint` | `message_started` | AI 开新一段消息时发出，前端 append 一条空 ChatMessage |
| `task_idle` / `task_failed` / `task_cancelled` | 删除 | 均由 `turn_finished { reason }` 取代；Task 层只保留 `task_working_changed` 在全局轻量通道 |
| `turn_started` / `turn_finished` | **新增（保留）** | turn 级起止事件。`turn_started` 携带 `turn_id` / `agent_id` / `trigger`；`turn_finished` 携带 `turn_id` / `reason`。**turn_finished 是该 turn 折叠的唯一触发时机**；并发 turn 的 turn_finished 互不影响 |
| `user_message_appended` | **新增** | 用户消息事件，携带 `user_message_id` / `content` / `mentions: agent_id[]` / `replies: ReplyRef[]`；Task.timeline 写入一条 UserMessage 条目。激活器从 `mentions ∪ replies.turn` 去重派生 agent 激活列表 |
| `turn_id` | **保留** | 后端分配，`turn_started` 与 `message_started` 上携带；reducer 按 `turn_id` 路由事件到对应 Turn，**并发场景下这是事件归属的唯一依据** |
| `agent_id` / `trigger` | **新增** | `turn_started` 携带；数据模型 Turn 字段；支持 teams / 多 agent / 触发关系可视化 |
| (新增) | `seq` 字段 | per-task 单调递增，承担断点续传 cursor、晚到事件去重 |

### 事件最小集

```
user_message_appended  {
  task_id, seq, user_message_id, content, ts,
  mentions: agent_id[],              // 直接 @ 召唤的 agent 列表
  replies: Array<                    // 引用回复的气泡列表
    | { kind: 'turn'; id: turn_id }
    | { kind: 'user_message'; id: user_message_id }
  >
}
turn_started           {
  task_id, seq, turn_id, agent_id,
  trigger: { kind: 'user', id: user_message_id }
         | { kind: 'turn', id: parent_turn_id }
}
message_started        { task_id, seq, turn_id, message_id }
tool_started           { task_id, seq, turn_id, message_id, tool_call_id, tool_name, summary }
tool_finished          { task_id, seq, turn_id, message_id, tool_call_id, status, preview, duration_ms }
assistant_stream_delta {
  task_id, seq, turn_id, message_id,
  field: 'reasoning' | 'content' | 'tool_call_arguments',
  tool_call_id?: string,   // field === 'tool_call_arguments' 时必填
  delta: string
}
message_finished       { task_id, seq, message_id }
turn_finished          { task_id, seq, turn_id, reason: 'idle' | 'failed' | 'cancelled', error_message?: string }
```

`assistant_stream_delta` 的 `field` 枚举覆盖一条 message 内全部三类流式内容，前端 reducer 按字段分派：

- `reasoning` → `message.reasoning += delta`
- `content` → `message.content += delta`
- `tool_call_arguments` → `message.tools[tool_call_id].arguments += delta`（tool_call 的参数也是流式拼的，落在对应 tool 项上；`tool_started` 负责先把该 tool 项以占位形式 push 进 `message.tools`）

设计要点：
- **事件路由完全由 id 驱动**：所有事件都携带 `turn_id` + `message_id`（tool/delta 事件两者都带），reducer 先按 `turn_id` 定位 Turn 再按 `message_id` 定位 Message，O(1) 路由。绝不能用"当前 streaming turn"或"最后一条 message"作为隐式上下文——并发场景下这些假设都会错
- `turn_finished` 是**该 turn 折叠的唯一触发时机**。多个并发 turn 的 turn_finished 独立到达、独立折叠
- `task_working_changed` 的边沿由"≥1 streaming turn"派生——第一个 turn_started 触发 `working=true`，最后一个 turn_finished 触发 `working=false`
- `user_message_appended` 是 UserMessage 进入 Task.timeline 的唯一入口，通常紧跟若干 `turn_started`（激活数 = `mentions ∪ replies.turn.agent_id` 去重后的大小）
- `runtime` / `debug_trace` 这类右栏数据走独立通道事件，不再搭车在消息事件上

---

## 五、前端改动清单

### 删除
- `LiveTurn` 类型及其所有引用
- `useLiveTurns.ts` 中：`closedLiveTurnIds`、`transitionKey`、`mergeActiveTaskSnapshot`、`mergeHistoryTurns`、`mergeActiveTask`、`historyTurnKey`、`sealLiveTurn`、`clearLiveTurn`、`ensureLiveTurn`、`upsertLiveTurn`、`archiveFailedTurn`、`archiveIntermediateTurn`
- `useArchivedTurns` 两个 store（failed / intermediate）
- `ChatMessageList.vue` 中独立的 `<article v-if="liveTurn">` 渲染块（约 103-174 行）以及 `liveTurnPresentation` / `buildLiveTurnPresentation` / `resolveLiveTurnEmptyText`
- 历史消息里的 `<details class="message-tools">` 折叠面板（折叠单位从 message 升级到 turn，见下 `<TurnGroup />`）

### 新增 / 调整
- `chatRuntimeStore`：维护 `Record<taskId, TaskTimelineEntry[]>`，是中栏唯一写入源。`TaskTimelineEntry` 是 UserMessage 或 Turn 的判别联合。
- `chatEventReducer.ts`：纯函数 `(timeline, event) => timeline`，可单测。**所有事件路由均基于 id 查找，禁止任何"最后一个 streaming X"的隐式假设**，因为并发 turn 场景下同时可能有多个 streaming turn。核心职责：
  - `user_message_appended` → Task.timeline 末尾 push 一个 UserMessage 条目（含 `mentions` 和 `replies`）
  - `turn_started` → Task.timeline 末尾 push 一个空 Turn 条目（`state='streaming'`、`messages=[]`、带 `agent_id` 与 `trigger`）
  - `message_started` → **按 `turn_id` 在 timeline 中查找目标 Turn**，在其 `messages` 数组末尾 push 新 Message。若找不到目标 Turn（事件早于 turn_started）则写暂存队列
  - `assistant_stream_delta { message_id, field }` → **按 `message_id` 在所有 Turn 的 messages 中查找目标 Message**，按 field 分派：
    - `field: 'content'` → timeline 末尾若是 `TextChunk` 则追加，否则 push 新 `TextChunk`
    - `field: 'reasoning'` → 累加到 `message.reasoning`
    - `field: 'tool_call_arguments'` → 累加到 timeline 内对应 `ToolCall` 项的 `arguments`
  - `tool_started { message_id, ... }` → 按 `message_id` 路由到目标 Message，向其 timeline 末尾 push 占位 `ToolCall` entry
  - `tool_finished { tool_call_id, ... }` → 按 `tool_call_id` 路由到目标 ToolCall，更新 status / preview
  - `message_finished { message_id }` → 按 `message_id` 路由到目标 Message，置为 `done`
  - `turn_finished { turn_id, reason }` → 按 `turn_id` 路由到目标 Turn，置为 `done` / `failed` / `cancelled`。**这是该 turn 折叠的唯一触发时机**；其他并发 turn 不受影响
- 数据模型类型：
  ```ts
  type TaskTimelineEntry = UserMessage | Turn

  type UserMessage = {
    kind: 'user_message'
    user_message_id: string
    content: string
    mentions: string[]               // 直接 @ 点名的 agent_id 列表
    replies: ReplyRef[]              // 引用的历史气泡
    ts: number
  }

  type ReplyRef =
    | { kind: 'turn'; id: string }            // 引用某条 Turn 气泡（kind='turn' 的 reply 驱动对应 agent 激活）
    | { kind: 'user_message'; id: string }    // 引用某条 UserMessage 气泡（纯语境引用，不驱动激活）

  type Turn = {
    kind: 'turn'
    turn_id: string
    agent_id: string                 // 这个 turn 归属哪个 agent
    trigger:
      | { kind: 'user'; id: string }
      | { kind: 'turn'; id: string }
    state: 'streaming' | 'done' | 'failed' | 'cancelled'
    error_message?: string
    messages: AssistantMessage[]
  }

  type AssistantMessage = {
    message_id: string
    turn_id: string                  // 冗余归属引用，校验用
    state: 'streaming' | 'done'
    reasoning: string                // 单段累加
    timeline: TimelineEntry[]        // 按事件到达顺序
  }

  type TimelineEntry =
    | { kind: 'text'; text: string }
    | { kind: 'tool'; tool_call_id: string; tool_name: string; arguments: string;
        status: 'running' | 'ok' | 'error'; preview?: string; duration_ms?: number }
  ```
  要点：
  - **UserMessage 与 Turn 平级**挂在 Task.timeline 下，UserMessage 不属于任何 Turn
  - Turn 的 `agent_id` 与 `trigger` 字段支持 teams / 多 agent / DAG 触发关系
  - AssistantMessage 不再有分离的 `content: string` 和 `tools: ToolCall[]` 字段——合并进 `timeline`，tool 前后的正文自然成段
  - AssistantMessage 的 `state` 只有 `streaming` / `done`；失败/取消在 Turn 层表达
- **新增 `<TurnGroup />` 组件**：渲染单个 Turn 条目，承担 turn 级渲染与折叠。
  - 顶部 header：`🤖 {agent_name} ← 被 @{trigger_label} 触发`，点击 trigger label 可滚动/高亮触发源条目
  - `turn.state === 'streaming'`：遍历 turn 内所有 message，按顺序平铺 reasoning + timeline，不画分割线
  - `turn.state === 'done'` 且 turn 含 ≥2 条 message：顶部渲染「N 个动作 ▸」折叠摘要（点击展开恢复平铺），中部画 `── 最终消息 ──` 分割线，下方展开最后一条 message
  - `turn.state === 'done'` 且 turn 仅 1 条 message：直接渲染该 message，无摘要、无分割线
  - `turn.state === 'failed' | 'cancelled'`：展示折叠摘要 + `error_message`，无分割线
  - N 的计算：`turn.messages.slice(0, -1).flatMap(m => m.timeline).filter(e => e.kind === 'tool').length`
  - **每个 TurnGroup 独立维护自己的 streaming/done 状态**，并发 turn 各自渲染互不干扰
- **新增 `<UserMessageBubble />`** 组件：渲染 Task.timeline 中的 UserMessage 条目。
  - 气泡**顶部**：内嵌渲染 `replies` 的缩略卡片列表（类似 Slack/Discord 的 reply preview），显示被引用气泡的类型、发送者、文本摘要。多条纵向堆叠。缩略卡片点击滚动并高亮定位到原气泡；悬停与其驱动的 TurnGroup 联动高亮（仅对 `kind === 'turn'` 成立）
  - 气泡**content 内联**：`mentions` 中的 agent 在文本里以 `@agent_name` 高亮 tag 形式显示，不在气泡顶部另起区域。悬停 tag 与其驱动的 TurnGroup 联动高亮
- `<TimelineRenderer />`：单条 AssistantMessage 的 timeline 渲染组件，顺序遍历 entries，`text` 走 markdown、`tool` 走 tool 卡片。被 `<TurnGroup />` 复用。
- `<ChatMessageList />`：按 `task.timeline` 顺序遍历，为每个条目渲染 `<UserMessageBubble />` 或 `<TurnGroup />`。**不做任何并发可视化**（side-by-side / 嵌套缩进），多个并发 streaming 的 turn 就是相邻几张独立卡片。
- task 切换时，从 task snapshot 一次性灌入 chatRuntimeStore，之后保持只读；snapshot 后续刷新不再写入聊天列表

### 边界处理
- **用户消息乐观插入**：用户点发送后，前端立即在 timeline 末尾 append 一条带临时 id 的 UserMessage（不等后端确认）。收到 `user_message_appended` 事件后，用正式 `user_message_id` 替换临时 id，补上 `seq`。若后端返回错误（如 task 已删除），移除该乐观条目并提示用户。
- **task 切换时正在 streaming 的 turn**：`get_task_history` 必须包含尚未结束 turn 的当前内容（state 仍为 streaming），切回时由 `subscribe_task(since_seq)` 接上后续 delta。
- **事件早于首次 hydrate 到达**：按 ui-events.md 要求，写入暂存队列，hydrate 完成后回放，不静默丢弃。
- **晚到事件**：reducer 入口比对 `seq`，小于该 task 已知最大 seq 的事件直接丢弃。

---

## 六、推进顺序

1. **协议正名**：后端把 `assistant_text_preview` 改成真正的 `assistant_stream_delta`（带 `field` 枚举），加 per-task `seq` 字段；废弃 `final_assistant_message`、`task_idle` / `task_failed` / `task_cancelled`。前端先适配新事件名，行为保持。
2. **引入 turn 级事件**：后端新增 `user_message_appended` / `turn_started` / `turn_finished`，携带 `turn_id` / `agent_id` / `trigger`；前端 reducer 按 turn_id 路由事件。
3. **数据模型切换**：前端 chatRuntimeStore 从 `ChatMessage[]` 切到 `TaskTimelineEntry[]`（= `UserMessage | Turn`）。新增 `<TurnGroup />`、`<UserMessageBubble />`、`<TimelineRenderer />` 组件，`<ChatMessageList />` 改为遍历 timeline。
4. **删除旧结构**：删 `LiveTurn` 类型、`useLiveTurns.ts` 大半内容、`<article v-if="liveTurn">` 渲染块、`archivedFailedTurns` / `archivedIntermediateTurns` store、`mergeActiveTaskSnapshot` / `mergeHistoryTurns` 双写路径、`closedLiveTurnIds` / `transitionKey`。
5. **折叠交互**：`<TurnGroup />` 实现 turn_finished 后验折叠——intermediate 内容折叠为「N 个动作 ▸」摘要 + 分割线 + 最终 message 展开。
6. **多 task 订阅**：接入 per-task `subscribe_task(since_seq)` 通道 + 全局 `task_working_changed` 轻量通道。

每一步都是独立可发布的，不必一次性翻底。

---

## 七、多 task 与订阅模型

前端定位是展示层，不应在内存里维护所有 task 的运行态。多 task 并行通过**双通道订阅 + cursor 续传**实现。

### 双通道职责划分

| 通道 | 内容 | 订阅范围 | 频率 |
|------|------|---------|------|
| 全局轻量通道 | `task_working_changed { task_id, working }`、未读标记 | 常驻订阅 | 低频 |
| per-task delta 通道 | 完整运行时事件（`turn_*` / `message_*` / `tool_*` / `assistant_stream_delta` / `user_message_appended`） | 当前 task，切换时切换 | 高频 |

左侧 task 列表的"旋转图标 / 未读点"由全局轻量通道驱动，与中栏 delta 流完全解耦。中断按钮在非当前 task 上也可用——直接调 `cancel_task(taskId)`（停掉该 task 所有 streaming turn）或 `cancel_turn(turnId)`（只停指定 turn），不需要先切过去。

### Cursor 设计

每个 task 维护 per-task 单调递增的 `seq`，所有 delta 通道事件都带 `seq` 字段。`seq` 同时承担三件事：
- 断点续传的 cursor
- 晚到事件去重
- 历史回放与 live 流的拼接边界

不使用 timestamp / message_id 作为 cursor：timestamp 可能撞或乱序，message_id 不能表达"同一条消息内的第 N 个 delta"。

### 接口形态

```
get_task_history(task_id) -> { timeline: TaskTimelineEntry[], last_seq: number }
subscribe_task(task_id, since_seq) -> stream of events with seq > since_seq
                                    | { error: 'gap_too_large' }
unsubscribe_task(task_id)
```

### 切换 task 流程

1. `unsubscribe_task(oldTaskId)`
2. 若新 task 在前端 store 中无数据 → `get_task_history(newTaskId)` 拉基线，记下 `last_seq`
3. 若之前看过 → 直接复用 store 中的 `last_seq`
4. `subscribe_task(newTaskId, last_seq)`，开始接 delta
5. 收到 `gap_too_large` → 丢弃本地缓存，回到 step 2

### 后端 ring buffer 与 gap_too_large

后端为每个 task 维护一个事件 ring buffer，用于响应 `subscribe_task(since_seq)` 的回放请求。

- **下界**：必须覆盖当前活跃 turn 的全部事件——最常见的断点场景就是用户切走又切回，某个 turn 还没结束
- **上界**：保守容量（建议 1000 条事件），超出则 `subscribe_task` 返回 `gap_too_large`，前端走"重新拉历史"兜底
- ring buffer 是运行时缓存，不是持久化结构；进程重启后清空，前端重连只能走 `get_task_history`

### 前端 store 缓存策略

切走的 task 不立即丢弃，按 LRU 保留最近 N 个（建议 5 个）。绝大多数场景下用户在 2-3 个 task 间切换，体验上是"瞬间切回"。超出 LRU 的 task 在下次访问时重新走 `get_task_history`。

### 与 TaskTimelineEntry[] 模型的关系

前端 chatRuntimeStore 形态是 `Record<taskId, TaskTimelineEntry[]>`，reducer 只处理"当前 task delta 通道"推来的事件。LRU 中的非当前 task 数据是只读快照，不会被任何事件更新——它们要么在切回时通过 `subscribe_task(since_seq)` 接上 delta 继续更新，要么因 `gap_too_large` 被整体丢弃重拉。

---

## 八、和现有 design 文档的关系

本方案是对 [ui-chat.md](ui-chat.md) 「中栏数据源边界」「收尾原子性」与 [ui-events.md](ui-events.md) 「运行时显示协议 vs 持久化」两节的具体落地——把"chat 是单一 source of truth"从约束升级成"chat 是唯一容器，liveTurn 这层中间结构本不该存在"。

落地时如有与 ui-chat.md / ui-events.md 字面冲突，应同步修订那两份文档而不是在代码里偷偷绕过。
