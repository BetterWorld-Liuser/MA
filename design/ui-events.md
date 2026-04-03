# UI 设计：聊天运行事件模型（草案）

> 本文定义聊天区与右栏联动所依赖的运行时事件模型。它是 [ui-chat.md](ui-chat.md) 中“等待态 / 工具实时反馈”的实现草案，不替代 [tools.md](tools.md) 中的工具语义本身。

---

## 设计目标

事件模型服务三个目标：

- 让前端能准确渲染“当前轮正在发生什么”
- 让聊天区、工具时间线、右栏联动共享同一份状态来源
- 让展示层只消费真实运行事件，而不是靠前端猜测状态

因此，事件模型关注的是**轮次生命周期**，不是“某个 Vue 组件怎么刷新”。

---

## 边界

这套事件是 **Ma 后端推给前端 UI 的显示事件**，不等于：

- provider 的原始 SSE 协议
- 数据库持久化结构
- AI 上下文里的 tool result 文本

同一轮内部可以有很多底层细节，但 UI 只需要稳定、可消费、可重放的高层事件。

---

## 总体思路

以“一个用户输入触发一轮 agent 执行”为主线，每轮分成三层：

1. **轮次事件**：这一轮开始、结束、失败
2. **阶段事件**：上下文构建、模型等待、流式输出等
3. **工具事件**：某个工具开始、结束、失败，以及关联对象

前端不直接靠布尔值拼状态，而是维护一个 `TurnViewModel`，由事件流增量更新。

---

## 事件公共字段

所有聊天运行事件都建议带上以下公共字段：

```ts
type UiRunEventBase = {
  event_id: string
  task_id: number
  turn_id: string
  ts: number
}
```

字段说明：

- `event_id`：事件唯一 id，用于前端去重、调试、日志排查
- `task_id`：任务 id，支持多任务并行时把事件路由到正确会话
- `turn_id`：本轮执行 id，前端以它聚合当前轮消息、工具项、状态
- `ts`：事件产生时间戳，毫秒级 unix timestamp

如果后续需要断线恢复或事件重放，可以在此基础上再追加单调递增的 `seq`。

---

## 核心事件列表

建议 MVP 先收敛到 8 类事件：

- `turn_started`
- `context_built`
- `assistant_status`
- `tool_started`
- `tool_finished`
- `assistant_stream_delta`
- `turn_finished`
- `turn_failed`

这 8 类足够覆盖：

- 用户发出请求后的“已接单”
- 当前轮的等待态文案切换
- 工具实时出现与状态收口
- 最终 AI 文本流式输出
- 正常完成和异常中止

---

## 事件定义

### `turn_started`

表示用户消息已经被系统接收，并创建了新的 AI 轮次。

```ts
type TurnStartedEvent = UiRunEventBase & {
  type: 'turn_started'
  user_message_id: string
  user_text: string
}
```

前端行为：

- 立刻在聊天区插入用户消息
- 同时插入一条空的 AI 消息槽位
- 将 AI 消息初始状态设为 `pending`

---

### `context_built`

表示本轮 AI 上下文已经按最新状态构建完成。

```ts
type ContextBuiltEvent = UiRunEventBase & {
  type: 'context_built'
  open_file_count: number
  note_count: number
  hint_count: number
  token_estimate?: {
    used: number
    budget: number
  }
}
```

前端行为：

- 将 AI 消息状态切为 `running`
- 等待态文案可更新为 `正在调用模型`
- 刷新右栏上下文用量
- 必要时刷新“监控文件”“笔记”“Hints”区域的本轮快照

---

### `assistant_status`

表示当前轮进入了某个可见阶段，但还没有自然语言增量或工具结果。

```ts
type AssistantStatusEvent = UiRunEventBase & {
  type: 'assistant_status'
  phase: 'building_context' | 'waiting_model' | 'streaming' | 'running_tool'
  label: string
}
```

建议约束：

- `phase` 是机器可消费枚举
- `label` 是直接给 UI 展示的短文案，如 `正在整理上下文`

前端行为：

- 更新当前 AI 消息顶部状态文案
- 如果用户尚未看到任何文本输出，就显示轻量等待态动画

这个事件的意义是：把“等待态文案切换”从前端猜测改为后端显式通知。

---

### `tool_started`

表示某个工具开始执行。

```ts
type ToolStartedEvent = UiRunEventBase & {
  type: 'tool_started'
  tool_call_id: string
  tool_name: string
  summary: string
  target_paths?: string[]
  display_meta?: {
    shell?: string
  }
}
```

字段说明：

- `tool_call_id`：工具调用实例 id，用于前后事件配对
- `tool_name`：机器名称，如 `open_file`、`run_command`
- `summary`：给 UI 的紧凑摘要，如 `run_command cargo test`
- `target_paths`：可选，供右栏文件高亮联动使用
- `display_meta`：可选显示信息，不放完整原始参数

前端行为：

- 在当前 AI 消息下创建一条工具项
- 工具项状态设为 `running`
- 若带有 `target_paths`，右栏对应文件短暂高亮

---

### `tool_finished`

表示某个工具执行结束。

```ts
type ToolFinishedEvent = UiRunEventBase & {
  type: 'tool_finished'
  tool_call_id: string
  status: 'success' | 'error'
  duration_ms: number
  summary?: string
  preview?: string
  target_paths?: string[]
}
```

字段说明：

- `status`：决定 UI 上显示成功还是失败
- `duration_ms`：用于工具耗时展示
- `summary`：可选，用于覆盖开始时的摘要
- `preview`：可选的短输出摘要，不是全文
- `target_paths`：用于结束后的右栏联动刷新

前端行为：

- 将对应工具项状态从 `running` 收口为 `success` 或 `error`
- 展示耗时与短摘要
- 如果涉及文件写入，触发右栏文件项刷新

MVP 不要求这个事件携带原始 input/output；展开详情时可以从本地缓存或单独的详情字段中读取。

---

### `assistant_stream_delta`

表示 AI 正在流式输出自然语言内容。

```ts
type AssistantStreamDeltaEvent = UiRunEventBase & {
  type: 'assistant_stream_delta'
  delta: string
}
```

前端行为：

- 将 AI 消息状态切为 `streaming`
- 把 `delta` 追加到当前消息正文
- 如果视图停留在底部，则自动跟随滚动

---

### `turn_finished`

表示本轮正常结束。

```ts
type TurnFinishedEvent = UiRunEventBase & {
  type: 'turn_finished'
  assistant_message_id: string
  final_text: string
}
```

前端行为：

- 将 AI 消息状态收口为 `done`
- 停止等待态动画
- 持久化当前轮展示内容到聊天记录

说明：

- 即使前面已经通过 `assistant_stream_delta` 拼出了完整文本，`final_text` 仍值得保留，便于校验与持久化

---

### `turn_failed`

表示本轮异常结束。

```ts
type TurnFailedEvent = UiRunEventBase & {
  type: 'turn_failed'
  stage: 'context' | 'tool' | 'provider' | 'internal'
  message: string
  retryable: boolean
}
```

前端行为：

- 将 AI 消息状态设为 `error`
- 在消息槽位中显示错误摘要
- 若 `retryable = true`，提供“重试本轮”入口

---

## 推荐状态机

前端可以按下面的单向状态机理解 AI 消息：

```text
pending
  -> running
  -> streaming
  -> done

pending
  -> running
  -> error

streaming
  -> error
```

状态解释：

- `pending`：轮次已创建，等待后端推进
- `running`：后端正在工作，但正文尚未开始流式输出
- `streaming`：正文输出中
- `done`：完成
- `error`：失败

`assistant_status` 和工具事件不会改变终态，只负责丰富 `running` 阶段的可见性。

---

## 前端聚合模型

建议前端维护类似下面的聚合结构：

```ts
type TurnViewModel = {
  turn_id: string
  task_id: number
  assistant_state: 'pending' | 'running' | 'streaming' | 'done' | 'error'
  status_label?: string
  assistant_text: string
  tools: ToolItemViewModel[]
  started_at: number
  finished_at?: number
  error?: {
    stage: string
    message: string
    retryable: boolean
  }
}

type ToolItemViewModel = {
  tool_call_id: string
  tool_name: string
  summary: string
  state: 'running' | 'success' | 'error'
  started_at: number
  finished_at?: number
  duration_ms?: number
  preview?: string
  target_paths?: string[]
}
```

这样聊天区组件只依赖聚合后的 UI 模型，不直接处理复杂协议细节。

---

## 与右栏联动规则

事件模型不只服务聊天区，也服务右栏的轻量联动：

- `context_built`：刷新上下文用量、notes/hints/open files 快照
- `tool_started.target_paths`：短暂高亮对应文件
- `tool_finished.target_paths`：刷新文件时间戳、新旧感、锁定状态展示

注意：

- 右栏只响应“与用户理解相关”的事件
- 不应因为每个底层 watcher 抖动都触发显著动画

---

## 与持久化的关系

事件流是**运行时显示协议**，聊天记录表中的 `tool_summaries` 是**轮次结束后的归档摘要**。

两者关系建议如下：

- 运行中：前端消费实时事件，构建正在进行的消息 UI
- 结束后：后端或前端从 `TurnViewModel` 归纳出简短 `tool_summaries`
- 持久化时只存摘要，不存整条事件流

这样既保留运行时反馈，又不会把数据库变成事件日志仓库。

---

## 与工具设计的关系

[`tools.md`](tools.md) 定义“AI 能调用哪些工具”和“工具在 agent 循环中的语义”。

本文定义的是“这些行为如何映射成用户能看到的事件”。例如：

- `open_file` / `write_file` / `replace_lines` 这类工具，会映射成 `tool_started` / `tool_finished`
- 轮内出现的阶段性 assistant 文本，更适合映射成 `assistant_stream_delta` 或中间文本更新
- 本轮真正走向 `turn_finished` 的信号，是 provider 不再返回新的 tool calls

UI 事件层不改变工具本身，只规定展示协议。

---

## MVP 建议

MVP 可以先不做得太重，按下面顺序实现：

1. 先打通 `turn_started` / `tool_started` / `tool_finished` / `assistant_stream_delta` / `turn_finished` / `turn_failed`
2. 再补 `context_built`，让右栏用量和等待态更准确
3. 最后再细化 `assistant_status` 的阶段文案与更丰富的联动

这样最早就能把“发出去以后界面完全没动静”的问题解决掉，同时保留后续细化空间。
