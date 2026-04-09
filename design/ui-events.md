# UI 设计：聊天运行事件模型（草案）

> 本文定义聊天区与右栏联动所依赖的运行时事件模型。它是 [ui-chat.md](ui-chat.md) 中“等待态 / 工具实时反馈”的实现草案，不替代 [tools.md](tools.md) 中的工具语义本身。

---

## 设计目标

事件模型服务三个目标：

- 让前端能准确渲染“当前轮正在发生什么”
- 让聊天区、工具时间线、右栏联动共享同一份状态来源
- 让展示层只消费真实运行事件，而不是靠前端猜测状态

同时要避免另一种常见失败模式：**把整个 UI 绑定到一份粗粒度 task snapshot 上一起重刷。**

- task snapshot 适合作为“当前任务事实”的同步载体，例如：历史消息、open files、notes、memories、当前 working directory、debug trace 的已落盘部分
- 但聊天区滚动位置、消息过渡动画、右栏展开/折叠状态、当前 debug tab、输入框草稿、live turn 等都属于前端本地视图状态，不应因为 snapshot 某一小部分变化而整体重置
- 因此，事件模型的目标不是“让所有组件一起刷新”，而是“让各自的 store 基于同一运行事实做增量更新”

其中右栏 `Debug` 的产品定位要明确：**它更像“本轮 agent trace viewer”**，用于检查当前这次 agent 运行内部每个 round 的上下文、请求、响应与工具结果；它不是跨多条聊天消息持续累积的长期调试日志视图。

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

但 `TurnViewModel` 也只是聊天运行态的一部分。围绕当前 task，前端应拆成多个职责明确的 store，而不是维护一个“一改全改”的巨型 `WorkspaceViewModel`：

1. `taskSnapshotStore`
   - 保存后端返回的任务级事实快照
   - 负责 task 切换、右栏上下文基线、debug trace 已完成轮次
2. `chatRuntimeStore`
   - 保存正式聊天消息、`liveTurn`、中间消息沉淀、失败轮归档、滚动跟随等聊天运行态
   - 消费 agent progress 事件，优先增量更新消息列表
3. `contextPanelUiStore`
   - 保存右栏本地交互状态，如折叠目录、当前 debug tab、response/raw 切换、大预览弹窗
   - 不应被 task snapshot 的普通刷新重置
4. `composerUiStore`
   - 保存草稿、mentions、图片附件、模型菜单和工作目录选择器的临时状态

设计约束：

- 聊天区不应因为 debug trace 追加一轮而重建整条消息列表
- 右栏 debug 区不应因为聊天区流式输出而重置当前 tab
- 任一 store 的局部更新都不应触发与其无关的过渡动画或滚动跳变
- 聊天区只允许一个“最终 assistant 消息”写入源，不能同时由事件 append 与 snapshot hydrate 双写
- 右栏上下文区、Debug 区这类同时消费 snapshot 与 runtime event 的区域，只允许维护一份任务 source 数据；视图层按需派生，不再额外维护同构 view cache 做双写
- runtime event 抵达时若基线 source 尚未 hydrate，不允许静默丢弃；至少要留下可检索的 debug 记录，必要时通过暂存/回放补齐

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

表示 AI 正在流式输出内容。`content_type` 区分 reasoning 流和正文流。

```ts
type AssistantStreamDeltaEvent = UiRunEventBase & {
  type: 'assistant_stream_delta'
  content_type: 'reasoning' | 'text'
  delta: string
}
```

前端行为：

- 将 AI 消息状态切为 `streaming`
- `content_type: 'text'`：把 `delta` 追加到消息正文；如果视图停留在底部则自动跟随滚动
- `content_type: 'reasoning'`：把 `delta` 追加到折叠块内的 reasoning 文本；折叠块默认折叠，流式期间可展开查看
- reasoning 流先于 text 流，两段之间有明确分界，不并行渲染

对 `OpenAiHidden` 风格的模型（reasoning 不可见），不会产生 `content_type: 'reasoning'` 的 delta；reasoning token 用量通过 `turn_finished.reasoning_tokens_used` 字段携带。

---

### `assistant_message_checkpoint`

表示 AI 产生了一个完整的阶段性输出，当前消息应当沉淀到历史，同时为后续内容创建新的消息槽位。

```ts
type AssistantMessageCheckpointEvent = UiRunEventBase & {
  type: 'assistant_message_checkpoint'
  message_id: string
  content: string
  checkpoint_type: 'intermediate' | 'final'
}
```

字段说明：

- `message_id`：该条阶段性消息的唯一 id
- `content`：完整的当前消息内容
- `checkpoint_type`：
  - `intermediate`：中间输出，后续还会有更多内容
  - `final`：本轮最终输出，等同于 `turn_finished` 的前置事件

前端行为：

- 触发消息沉淀动画：
  1. 当前 `liveTurn` 内容以淡出动画（~200ms）沉淀到 `chat` 历史末尾
  2. 清空 `liveTurn` 或填充新的内容，以淡入动画（~200ms）呈现
- 中间消息（`intermediate`）可以带有视觉区分样式，如略低的 opacity 或特殊边框
- 最终消息（`final`）沉淀后，本轮状态收敛到 `done`

使用场景：

- AI 先输出思考过程，然后输出正式回复
- AI 修正之前的输出，需要把旧版本固化到历史
- 长回复分段呈现，每段作为独立消息便于阅读

---

### `turn_finished`

表示本轮正常结束。

```ts
type TurnFinishedEvent = UiRunEventBase & {
  type: 'turn_finished'
  assistant_message_id: string
  final_text: string
  reasoning_tokens_used?: number  // 本轮 reasoning 消耗的 token 数；OpenAiHidden 时尤为有用
}
```

前端行为：

- 将 AI 消息状态收口为 `done`
- 停止等待态动画
- 持久化当前轮展示内容到聊天记录
- 若 `reasoning_tokens_used` 有值，刷新右侧面板上下文用量中的 reasoning token 计数

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

但联动是“共享事实、分开消费”，不是“共享整块视图模型一起刷新”。

具体要求：

- `debug_trace` 的增长只驱动 Debug 区自己的列表追加，不应导致聊天消息列表先卸载再挂回
- `runtime.context_usage` 的变化只驱动右栏上下文用量区更新，不应影响输入框或消息过渡状态
- task 级事实刷新时，前端应尽量做字段级 merge，而不是整块替换后让所有子树自行重建
- 只有 task 切换、任务删除、工作区首次加载这类“语义上确实换了一整页”的场景，才允许大范围重建
- 聊天区不消费 `active_task.history` 的常规刷新；`history` 只作为首次进入 task 或显式 resync 的基线来源
- `active_task` 这类混合对象在 merge 时必须逐字段声明语义：`history / notes / open_files / hints` 可按最新 snapshot 替换；`runtime / debug_trace` 若本次事件未提供，应继承旧值，而不是被对象 spread 隐式清空
- 多个事件分支若共享“忽略已关闭 turn / 标记 task working / 写入 runtime / 获取 liveTurn / 合并回写”这类前缀，应抽成单个 helper，确保所有分支遵守同一状态机入口与忽略规则

关于 Debug 区还应补一条心智约束：

- 默认关注“当前 task 最近一次运行形成的 round trace”；新一轮对话开始后，Debug 区可以被新的 trace 覆盖
- 是否保留更长时间跨度的历史 trace，是后续独立功能，不应隐式混入当前右栏 Debug 设计

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

同时，持久化快照与前端临时态要显式分层：

- 后端持久化层负责“这一轮结束后真实留下来的事实”
- 前端运行态负责“这一轮收敛之前用户看到的过程”
- 两者在轮次结束时需要**原子收口**：最终 assistant 消息进入历史列表、对应 live turn 消失、debug trace 追加完成，应表现为一次连续更新，而不是先清空一部分临时态、下一帧再由持久化快照补回来
- 如果一次完成事件只带来“部分 task 字段更新”，前端必须按语义 merge 到当前 source 上，不允许因为一条字段缺失的完成态 payload 把右栏 runtime/debug 信息抹掉

如果收口被拆成两个前端更新，用户会看到消息闪烁、列表回弹、滚动位置抖动；这是应在事件层和 store 层共同避免的 UI 错误，而不是可接受的小瑕疵。

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
