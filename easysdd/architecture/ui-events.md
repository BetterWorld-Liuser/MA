# UI 设计：聊天运行事件模型

> 本文定义聊天区与右栏联动所依赖的运行时事件模型。它是 [ui-chat.md](ui-chat.md) 中"Turn 内展示策略 / 折叠原子性"的实现规范，不替代 [tools.md](tools.md) 中的工具语义本身。
>
> 术语定义（Task / UserMessage / Turn / Message / ToolCall / Delta 等）以及完整的协议改动背景，见 → [聊天运行态改进方案](chat-runtime-改进方案.md)。

---

## 设计目标

事件模型服务三个目标：

- 让前端能准确渲染"当前轮正在发生什么"（含并发多 Turn 场景）
- 让聊天区、工具时间线、右栏联动共享同一份状态来源
- 让展示层只消费真实运行事件，而不是靠前端猜测状态

同时要避免另一种常见失败模式：**把整个 UI 绑定到一份粗粒度 task snapshot 上一起重刷。**

- task snapshot 适合作为"当前任务事实"的同步载体（历史消息基线、open files、notes、debug trace 已落盘部分）
- 聊天区滚动位置、Turn 折叠状态、右栏展开/折叠、当前 debug tab、输入框草稿等都属于前端本地视图状态，不应因为 snapshot 某一小部分变化而整体重置
- 事件模型的目标是"让各自的 store 基于同一运行事实做增量更新"，不是"让所有组件一起刷新"

---

## 边界

这套事件是 **March 后端推给前端 UI 的显示事件**，不等于：

- provider 的原始 SSE 协议
- 数据库持久化结构
- AI 上下文里的 tool result 文本

---

## 总体结构

中栏数据源是 `chatRuntimeStore`，形态为 `Record<taskId, TaskTimelineEntry[]>`。`TaskTimelineEntry` 是 `UserMessage | Turn` 的判别联合。

所有运行时事件经由 `chatEventReducer(timeline, event) => timeline` 处理——纯函数，可单测，**所有路由均基于 id 查找，禁止任何"最后一个 streaming Turn / Message"的隐式假设**。

前端按职责拆成多个 store，互不干扰：

| Store | 职责 |
|-------|------|
| `chatRuntimeStore` | `Record<taskId, TaskTimelineEntry[]>`，中栏唯一 source of truth |
| `taskSnapshotStore` | 任务级事实基线（open files、notes、debug trace 已落盘部分）；task 切换 / 显式 resync 时更新 |
| `contextPanelUiStore` | 右栏本地交互状态（折叠目录、当前 debug tab 等），不被 snapshot 普通刷新重置 |
| `composerUiStore` | 输入框草稿、mentions、图片附件、模型/工作目录选择器临时状态 |

设计约束：

- 聊天区不应因为 debug trace 追加一轮而重建整条 timeline
- 右栏 debug 区不应因为聊天区流式输出而重置当前 tab
- 任一 store 的局部更新都不应触发与其无关的过渡动画或滚动跳变
- 聊天区只允许一个最终消息写入源（事件流），不能与 snapshot hydrate 双写
- runtime event 抵达时若基线 source 尚未 hydrate，不允许静默丢弃；写入暂存队列，hydrate 完成后回放

---

## 前端数据模型

```ts
type TaskTimelineEntry = UserMessage | Turn

type UserMessage = {
  kind: 'user_message'
  user_message_id: string
  content: string
  mentions: string[]            // 直接 @ 点名的 agent_id 列表
  replies: ReplyRef[]           // 引用的历史气泡
  ts: number
  seq: number
}

type ReplyRef =
  | { kind: 'turn'; id: string }
  | { kind: 'user_message'; id: string }

type Turn = {
  kind: 'turn'
  turn_id: string
  agent_id: string
  trigger:
    | { kind: 'user'; id: string }   // 被用户消息激活
    | { kind: 'turn'; id: string }   // 被父 turn 结束后的链式触发激活
  state: 'streaming' | 'done' | 'failed' | 'cancelled'
  error_message?: string
  messages: AssistantMessage[]
  seq: number
}

type AssistantMessage = {
  message_id: string
  turn_id: string               // 冗余归属引用，校验用
  state: 'streaming' | 'done'
  reasoning: string             // 单段累加
  timeline: TimelineEntry[]     // 按事件到达顺序
}

type TimelineEntry =
  | { kind: 'text'; text: string }
  | { kind: 'tool'; tool_call_id: string; tool_name: string; arguments: string;
      status: 'running' | 'ok' | 'error'; preview?: string; duration_ms?: number }
```

---

## 事件公共字段

所有聊天运行事件都带以下公共字段：

```ts
type UiRunEventBase = {
  task_id: number
  seq: number    // per-task 单调递增，承担断点续传 cursor、晚到事件去重
  ts: number
}
```

- `seq`：per-task 单调递增整数。reducer 入口比对 seq，小于该 task 已知最大 seq 的事件直接丢弃（去重）；也用作 `subscribe_task(since_seq)` 的断点续传 cursor
- `task_id`：事件路由到正确 task 的 timeline
- `ts`：事件产生时间戳，毫秒级 unix timestamp

---

## 事件列表

### `user_message_appended`

后端确认用户消息已接收，UserMessage 进入 timeline 的唯一入口。

```ts
type UserMessageAppendedEvent = UiRunEventBase & {
  type: 'user_message_appended'
  user_message_id: string
  content: string
  mentions: string[]            // 直接 @ 点名的 agent_id 列表
  replies: Array<
    | { kind: 'turn'; id: string }
    | { kind: 'user_message'; id: string }
  >
}
```

前端行为：

- 若 timeline 末尾有乐观插入的临时 UserMessage，用正式 `user_message_id` 替换临时 id，补上 `seq`
- 若无乐观条目，直接 push 新 UserMessage
- 紧接着会按"mentions 先、replies.turn 后、去重"的激活列表收到若干 `turn_started`

---

### `turn_started`

一个新 turn 被激活。

```ts
type TurnStartedEvent = UiRunEventBase & {
  type: 'turn_started'
  turn_id: string
  agent_id: string
  trigger:
    | { kind: 'user'; id: string }   // user_message_id
    | { kind: 'turn'; id: string }   // parent_turn_id
}
```

前端行为：

- 在 `task.timeline` 末尾 push 一个空 Turn 条目，`state='streaming'`、`messages=[]`
- 激活器串行发出 turn_started，seq 顺序 = 激活列表顺序，视觉顺序确定
- 不代表立即开始接收 message 事件；turn 一旦激活即并发执行

---

### `message_started`

一次 LLM API 调用开始流式返回。

```ts
type MessageStartedEvent = UiRunEventBase & {
  type: 'message_started'
  turn_id: string
  message_id: string
}
```

前端行为：

- **按 `turn_id` 在 timeline 中查找目标 Turn**，在其 `messages` 数组末尾 push 一条新 Message（`state='streaming'`，`reasoning=''`，`timeline=[]`）
- 若找不到目标 Turn（事件早于 turn_started），写暂存队列，turn_started 到达后回放
- 不代表 turn 开始；不代表用户消息到达

---

### `assistant_stream_delta`

Message 内的流式增量，按 `field` 枚举分派。

```ts
type AssistantStreamDeltaEvent = UiRunEventBase & {
  type: 'assistant_stream_delta'
  turn_id: string
  message_id: string
  field: 'reasoning' | 'content' | 'tool_call_arguments'
  tool_call_id?: string   // field === 'tool_call_arguments' 时必填
  delta: string
}
```

前端行为（按 `message_id` 路由到目标 Message）：

- `field: 'reasoning'` → `message.reasoning += delta`
- `field: 'content'` → message.timeline 末尾若是 `text` 则追加，否则 push 新 `{ kind: 'text', text: delta }`
- `field: 'tool_call_arguments'` → 按 `tool_call_id` 找到 timeline 内对应 ToolCall，`arguments += delta`

不是"一条消息"，是消息内的增量片段。

---

### `tool_started`

某个工具开始执行。

```ts
type ToolStartedEvent = UiRunEventBase & {
  type: 'tool_started'
  turn_id: string
  message_id: string
  tool_call_id: string
  tool_name: string
  summary: string
  target_paths?: string[]
}
```

前端行为：

- 按 `message_id` 路由到目标 Message，向其 `timeline` 末尾 push 占位 ToolCall entry（`status: 'running'`）
- 若带有 `target_paths`，右栏对应文件短暂高亮

---

### `tool_finished`

某个工具执行结束。

```ts
type ToolFinishedEvent = UiRunEventBase & {
  type: 'tool_finished'
  turn_id: string
  message_id: string
  tool_call_id: string
  status: 'ok' | 'error'
  preview?: string
  duration_ms: number
  target_paths?: string[]
}
```

前端行为：

- 按 `tool_call_id` 路由到目标 ToolCall，更新 `status` / `preview` / `duration_ms`
- 若涉及文件写入，触发右栏文件项刷新

---

### `message_finished`

当前这条 Message 的流式输出结束。

```ts
type MessageFinishedEvent = UiRunEventBase & {
  type: 'message_finished'
  message_id: string
}
```

前端行为：

- 按 `message_id` 路由到目标 Message，`state → 'done'`

**不代表 Turn 结束。折叠、摘要、分割线都不由它触发。**

---

### `turn_finished`

某个具体 Turn 的结束信号，也是该 Turn 折叠的**唯一触发时机**。

```ts
type TurnFinishedEvent = UiRunEventBase & {
  type: 'turn_finished'
  turn_id: string
  reason: 'idle' | 'failed' | 'cancelled'
  error_message?: string
}
```

前端行为：

- 按 `turn_id` 路由到目标 Turn，`state → reason`（`idle` → `done`，其余同名）
- `reason === 'idle'`：执行折叠——识别最终 Message（最后一条），将之前的 intermediate 内容折叠为「N 个动作 ▸」摘要，画 `── 最终消息 ──` 分割线，展开最终 Message
- `reason === 'failed' | 'cancelled'`：展示折叠摘要 + `error_message`，无分割线
- N 的计算：`turn.messages.slice(0, -1).flatMap(m => m.timeline).filter(e => e.kind === 'tool').length`
- **并发场景下，不同 Turn 的 turn_finished 独立到达，各自独立触发折叠，互不影响**

---

### `task_working_changed`

全局轻量通道广播，用于左栏 task 列表的旋转图标和未读标记。

```ts
type TaskWorkingChangedEvent = {
  type: 'task_working_changed'
  task_id: number
  working: boolean    // = 该 task 内存在 ≥1 个 state='streaming' 的 Turn
}
```

- 走独立订阅通道，所有 task 常驻订阅
- 第一个 `turn_started` 触发 `working=true`，最后一个 `turn_finished` 触发 `working=false`
- 中栏 reducer **不消费**此事件，此事件不承载消息内容

---

## 废弃事件

以下事件从协议中删除：

| 废弃事件 | 替代 |
|---------|------|
| `assistant_text_preview` | `assistant_stream_delta`（带 `field` 枚举的真正增量） |
| `final_assistant_message` | 已被 stream delta + `message_finished` 完整覆盖 |
| `assistant_message_checkpoint` | `message_started`（AI 开新一段消息时发出） |
| `task_idle` | `turn_finished { reason: 'idle' }` |
| `task_failed` | `turn_finished { reason: 'failed' }` |
| `task_cancelled` | `turn_finished { reason: 'cancelled' }` |
| `turn_failed`（旧独立事件） | 合并进 `turn_finished { reason: 'failed' }` |
| `context_built` | 右栏数据走独立通道事件，不再搭车消息事件 |
| `assistant_status` | 不再提供前端猜测的状态文案切换，由 tool 事件驱动实际状态 |

---

## 事件路由规则

```
task_id  → chatRuntimeStore 中找到对应 Task 的 timeline
turn_id  → timeline 中找到对应 Turn
message_id → Turn.messages 中找到对应 Message
tool_call_id → Message.timeline 中找到对应 ToolCall
```

所有路由 O(1)（Map / 索引查找）。**绝不用"当前 streaming Turn"或"最后一条 Message"作为隐式上下文**——并发场景下这些假设都会错。

---

## 多 task 订阅

### 双通道职责

| 通道 | 内容 | 订阅范围 | 频率 |
|------|------|---------|------|
| 全局轻量通道 | `task_working_changed`、未读标记 | 常驻订阅 | 低频 |
| per-task delta 通道 | 完整运行时事件（`turn_*` / `message_*` / `tool_*` / `assistant_stream_delta` / `user_message_appended`） | 当前 task，切换时切换 | 高频 |

### Cursor 设计

每个 task 维护 per-task 单调递增的 `seq`，所有 delta 通道事件都带 `seq` 字段，同时承担：

- 断点续传的 cursor
- 晚到事件去重
- 历史回放与 live 流的拼接边界

### 接口形态

```
get_task_history(task_id) -> { timeline: TaskTimelineEntry[], last_seq: number }
subscribe_task(task_id, since_seq) -> stream of events with seq > since_seq
                                    | { error: 'gap_too_large' }
unsubscribe_task(task_id)
```

### 切换 task 流程

1. `unsubscribe_task(oldTaskId)`
2. 若新 task 在前端 store 中无数据 → `get_task_history(newTaskId)`，记下 `last_seq`
3. 若之前看过 → 直接复用 store 中的 `last_seq`
4. `subscribe_task(newTaskId, last_seq)`，开始接 delta
5. 收到 `gap_too_large` → 丢弃本地缓存，回到 step 2

### 前端 store 缓存策略

切走的 task 按 LRU 保留最近 N 个（建议 5 个），绝大多数"2-3 个 task 间切换"场景体验上是瞬间切回。超出 LRU 的 task 在下次访问时重走 `get_task_history`。

---

## 与右栏联动规则

事件模型不只服务聊天区，也服务右栏的轻量联动：

- `tool_started.target_paths`：短暂高亮对应文件
- `tool_finished.target_paths`：刷新文件时间戳、新旧感、锁定状态展示

联动是"共享事实、分开消费"，不是"共享整块视图模型一起刷新"：

- `debug_trace` 的增长只驱动 Debug 区自己的列表追加，不应导致聊天 timeline 重建
- `runtime.context_usage` 的变化只驱动右栏上下文用量区更新，不影响输入框或 Turn 折叠状态
- task 级事实刷新时，前端做字段级 merge，不整块替换后让所有子树重建
- 只有 task 切换、任务删除、工作区首次加载这类语义上确实换了整页的场景，才允许大范围重建

---

## 与持久化的关系

事件流是**运行时显示协议**。

- 运行中：前端消费实时事件，chatEventReducer 增量更新 TaskTimelineEntry[]
- 结束后：后端将轮次结束事实持久化（tool 摘要、最终消息内容等）
- 持久化快照只在 task 切换 / 初始化 / gap_too_large 时作为基线写入 chatRuntimeStore，之后只读

`runtime` / `debug_trace` 这类右栏数据走独立通道事件，若本次 payload 未提供，语义默认是沿用旧值，不被对象 spread 隐式清空。

---

## 与工具设计的关系

[`tools.md`](tools.md) 定义"AI 能调用哪些工具"和"工具在 agent 循环中的语义"。

本文定义"这些行为如何映射成用户能看到的事件"：

- `open_file` / `write_file` / `replace_lines` 等工具，映射成 `tool_started` / `tool_finished`
- 流式文本输出映射成 `assistant_stream_delta { field: 'content' }`
- 思考内容映射成 `assistant_stream_delta { field: 'reasoning' }`
- 工具参数的流式拼接映射成 `assistant_stream_delta { field: 'tool_call_arguments' }`
- Turn 真正走向 `turn_finished { reason: 'idle' }` 的信号，是 provider 不再返回新的 tool calls

UI 事件层不改变工具本身，只规定展示协议。
