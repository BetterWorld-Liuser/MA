# UI 调试观测面板设计

> 本文档描述 Ma UI 中“调试原始上下文 / Provider 输入输出”的观测能力设计。它是 [`../DESIGN.md`](../DESIGN.md) 中“用户视图 vs AI 上下文分离”理念在 UI 层的延伸：默认聊天界面保持干净，但系统必须提供足够直接的方式，让开发者观察 AI 每一轮实际看到了什么、发出了什么、收到了什么。

---

## 设计目标

当前 UI 已经具备任务列表、聊天区、上下文面板三栏骨架，但缺少一块对开发者非常关键的能力：

- 看 AI 当轮实际收到的上下文长什么样
- 看真正发给 provider 的请求体，而不只是人类可读摘要
- 看 provider 返回的原始响应
- 看每一轮工具调用及工具结果

如果没有这层观测能力，很多问题会很难定位：

- 为什么模型没看到某个文件
- 为什么它选错了工具
- 为什么它没有按预期自然结束或继续调用工具
- 为什么 provider 行为和我们“想象中的 prompt”不一致

因此需要在 UI 中加入一个开发者向的调试观测面板，但不能破坏日常聊天体验。

---

## 设计原则

### 1. 调试信息不污染主聊天流

聊天区的主职责仍然是承载“用户 ↔ AI”对话。

以下内容不应直接混入普通消息气泡：

- 完整上下文字符串
- Provider 请求 JSON
- Provider 原始响应 JSON
- 工具调用原始参数
- 工具结果原始文本

这些内容应进入独立的 debug 视图，而不是让聊天记录退化成日志窗口。

### 2. 调试信息以“最近一轮”为中心

开发者最常见的问题是：“刚才这一句发送出去后，系统到底做了什么？”

所以 UI 首先要能稳定展示最近一次 `send_message` 对应的完整 trace，而不是一上来做成长历史日志浏览器。历史归档可以通过本地落盘承担，UI 默认聚焦当前轮。

### 3. 区分“人类可读预览”和“真实 provider 输入”

这两者不能混为一谈：

- `context_preview`：用于人类理解的调试预览，便于快速看当前上下文拼接结果
- `provider_request_json`：真实发给 provider 的请求体，包含 `model`、`messages`、`tools`、`tool_choice` 等结构化字段

前者方便阅读，后者用于协议级排查。UI 里要明确分开展示，避免再次产生“工具是不是只以字符串传入”的误解。

### 4. 默认轻量，按需展开

调试面板默认应保持收敛，例如：

- 默认只展示最近一轮
- 默认折叠长文本块
- 每块支持展开 / 收起 / 复制

避免右栏长期被大段 JSON 或 prompt 占满。

---

## UI 形态

推荐形态：在右侧 `Context` 面板中新增一个 `Debug` 分区或独立 tab，`Debug` 内部再使用 tab 分栏承载不同类型的调试信息。

原因：

- 这块信息和“AI 当前上下文”天然相关
- 它属于开发者观测视图，不属于用户聊天正文
- 不需要打断中间聊天区的阅读节奏

### 推荐布局

右栏从上到下保持如下信息层次：

1. `Notes`
2. `Open Files`
3. `Hints`
4. `Context Usage`
5. `Debug`

如果后续右栏改为 tab，则建议：

- `Context`
- `Debug`

其中 `Debug` tab 内不直接堆叠所有原始数据，而是再细分为多个内部 tab，展示本轮 trace 的不同切面。

### Debug 内部 tab

推荐采用以下内部结构：

- `Overview`
- `Context`
- `Request`
- `Response`
- `Tools`

各 tab 的职责如下：

- `Overview`：展示最近一次消息的 round 摘要，例如轮次数、是否产生 tool call、是否已经自然结束、每轮工具数量
- `Context`：只展示 `context_preview`
- `Request`：只展示真实发给 provider 的请求 JSON
- `Response`：只展示 provider 原始响应
- `Tools`：展示 tool calls 与 tool results，并按轮次或执行顺序对应

如果初版希望更克制，也可以把 `Overview` 收敛为顶部摘要条，不单独做 tab，最小落地版本保留：

- `Context`
- `Request`
- `Response`
- `Tools`

---

## Debug 面板内容

### 1. Overview

如果 `Overview` 作为独立 tab，则展示最近一次 agent loop 的轮次列表：

- Round 1
- Round 2
- Round 3

每一轮显示：

- 是否产生 tool call
- 是否产生最终 assistant 文本
- 该轮包含多少个 tool calls

如果 `Overview` 不单独成 tab，则这些信息应作为 `Debug` 面板顶部摘要区域保留。

### 2. Context

`Context` tab 显示 `AgentSession::render_prompt()` 风格的上下文预览，定位是：

- 快速人工检查上下文构成
- 看 open files / notes / hints / recent chat 是否符合预期

这块应标注为：

`Context Preview (human-readable)`

避免用户误以为这就是完整 provider 请求。

### 3. Request

`Request` tab 显示真实请求 JSON，而不是摘要文本。

至少包含：

- `model`
- `messages`
- `tools`
- `tool_choice`
- `temperature`

这块应标注为：

`Provider Request (actual payload)`

这是排查 provider 协议问题的核心视图。

### 4. Response

`Response` tab 默认显示 provider 响应的结构化视图，而不是直接把 SSE 原始文本整段倾倒给用户。

如果 provider 返回的是普通 JSON 响应，则直接 pretty-print。

如果 provider 返回的是 SSE 流，则应在后端先把流式增量重组成“最终响应结构体”，而不是把 event 列表直接暴露给 UI，例如：

- 最终拼出的 `content`
- 最终拼出的 `tool_calls`

同时保留 `Raw` 子视图，便于协议级排查。

这块应标注为：

`Provider Response (structured)`

用于观察：

- 返回的是 assistant text 还是 tool calls
- tool call 的参数长什么样
- provider 有没有异常字段或意外格式

`Raw` 子视图则标注为：

`Provider Response (raw SSE / raw JSON)`

### 5. Tools

`Tools` tab 逐条展示：

- tool name
- tool call id
- arguments JSON

如果一轮里调用多个工具，应按执行顺序展示。

同时展示工具执行结果原文，例如：

- command stdout / stderr
- 文件编辑结果说明
- 最终 assistant 文本或中间文本片段

Tool result 要和 tool call 在视觉上对应，便于一眼看出“哪个调用产生了什么结果”。

---

## 数据结构设计

UI 不应自己重新拼装 debug 信息，而应由后端直接输出可消费的数据结构。

建议新增：

```rust
struct UiDebugTraceView {
    rounds: Vec<UiDebugRoundView>,
}

struct UiDebugRoundView {
    iteration: usize,
    context_preview: String,
    provider_request_json: String,
    provider_response_json: String,
    provider_response_raw: String,
    tool_calls: Vec<UiDebugToolCallView>,
    tool_results: Vec<String>,
}

struct UiDebugToolCallView {
    id: String,
    name: String,
    arguments_json: String,
}
```

### 挂载位置

短期建议挂在 `UiTaskSnapshot` 上：

```rust
struct UiTaskSnapshot {
    // ...
    debug_trace: Option<UiDebugTraceView>,
}
```

原因：

- 这份数据和“最近一次发送后的任务状态”强绑定
- 前端在 `send_message` 完成后可以直接刷新到最新 trace
- 不需要单独再发一次 debug 查询请求

### 生命周期

`debug_trace` 默认只保存最近一次 `send_message` 的结果，不进入长期持久化主数据模型。

原因：

- 体积较大
- 多数情况下只关心最近一轮
- 长期历史更适合落盘到 debug 文件

---

## 后端采集要求

当前 `AgentRunResult` 已有：

- `debug_rounds`
- `context_preview`
- `provider_raw_response`
- `tool_calls`
- `tool_results`

但还缺一个关键字段：

- `provider_request_json`

因此 provider 层需要在真正发请求前，把 `ChatCompletionRequest` 序列化为 JSON 字符串并保留下来，再进入 `DebugRound`。

建议调整后的 `DebugRound`：

```rust
struct DebugRound {
    iteration: usize,
    context_preview: String,
    provider_request_json: String,
    provider_raw_response: String,
    tool_calls: Vec<DebugToolCall>,
    tool_results: Vec<String>,
}
```

这样 UI 才能同时看到：

- 人类可读上下文
- 真实请求体
- 原始响应体

---

## 本地落盘

除了 UI 面板，还应保留本地 debug 落盘能力。

推荐目录：

```text
.ma/debug/
```

推荐内容：

- `context.txt` 或按轮分段的 context dump
- `provider-request.json`
- `provider-response.json`
- `tool-results.txt`

或者按一次发送聚合成单个 trace 文件。

### 落盘目的

- UI 只展示最近一轮，避免过载
- 调查复杂问题时，仍可回看完整细节
- 方便用户直接附带 trace 反馈 bug

---

## 交互细节

调试面板建议提供以下交互：

- 展开 / 收起每一轮
- 展开 / 收起每个代码块
- 在内部 tab 之间切换不同调试层
- `Copy context`
- `Copy request`
- `Copy response`
- `Copy tool result`

对于很长的内容：

- 默认限制高度并滚动
- 使用等宽字体
- 保留换行
- JSON 尽量 pretty print

---

## 与现有 UI 状态的关系

这项能力属于“聊天主链稳定性”和“工具调用详情视图”的中间层能力。

它不是面向最终普通用户的主交互，但对 Ma 的开发、调试、协议排查非常重要，应该尽快补齐。

在优先级上，建议：

- 高于纯视觉细节打磨
- 低于能否正常发消息、切任务、展示上下文这些基础闭环
- 可与“工具调用详情展开查看原始 input / output”一起设计，但不必绑定到同一时间实现

---

## 当前结论

UI 需要新增一个面向开发者的 `Debug` 观测面板。外层作为独立调试区域，内层再通过 tab 分栏展示最近一次消息处理的完整 agent trace。

推荐内部结构：

- `Overview`
- `Context`
- `Request`
- `Response`
- `Tools`

其中 `Request` 必须是真实请求 JSON，不能只展示渲染后的 prompt 文本。
