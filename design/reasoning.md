# Reasoning 模型支持

> 从 [provider.md](provider.md) 和 [ui-events.md](ui-events.md) 延伸：各家 reasoning 格式的归一化、上下文透传规则、task 级运行参数、UI 展示。

---

## 各家 Wire Format 差异

不同 provider 的 reasoning 实现差异很大，不能在上层统一抹平：

| Provider | 触发方式 | Reasoning 内容是否可见 | 关键约束 |
|---|---|---|---|
| **Anthropic** | `thinking: {type: "enabled", budget_tokens: N}` | 可见，独立 `thinking` content block，带 `signature` | **历史中的 thinking block 必须原样透传**，不能删除或修改，否则 API 报错 |
| **OpenAI o 系列** | `reasoning_effort: "none\|low\|medium\|high\|xhigh"` | 不可见（hidden reasoning），只暴露 `reasoning_tokens` 用量 | 不同模型支持的 effort 子集不同；`none` 表示模型内部关闭推理 |
| **DeepSeek-R1 / QwQ（OpenAI compat）** | 无需特殊触发，模型自动输出 | 可见，通过 `reasoning_content` 字段，或 `<think>…</think>` 标签包在 content 里 | 非标准字段，各中转站实现不一 |
| **Gemini Flash Thinking** | `thinkingConfig: {thinkingBudget: N}` | 可见，独立 thought content block | 有独立的 `thoughtsTokenCount` 计数 |

WireAdapter 层负责把这四种格式归一化为内部 `ReasoningBlock`，供上下文管理层和 UI 事件层稳定消费。

---

## 数据模型

### ReasoningCapability

```rust
/// 模型支持的 reasoning 能力描述，随 ModelCapabilities 一起在 session 初始化时解析
struct ReasoningCapability {
    style: ReasoningStyle,
    /// Anthropic / Gemini 用：最小思考预算（tokens）；Anthropic 要求至少 1024
    min_budget_tokens: Option<u32>,
    /// Anthropic / Gemini 用：默认思考预算（tokens）
    default_budget_tokens: Option<u32>,
    /// Anthropic / Gemini 用：最大思考预算（tokens）
    max_budget_tokens: Option<u32>,
    /// OpenAI o 系列用：该模型实际支持的 effort 档位子集
    supported_efforts: Vec<ReasoningEffort>,
}

enum ReasoningStyle {
    /// Anthropic extended thinking：thinking block + signature，需在历史中原样透传
    AnthropicThinking,
    /// OpenAI o 系列：推理不可见，只有 token 计数；用 reasoning_effort 控制强度
    OpenAiHidden,
    /// DeepSeek-R1 / QwQ 等：reasoning_content 字段或 <think> 标签，OpenAI compat 协议
    InlineTag,
    /// Gemini Flash Thinking：独立 thought content block
    GeminiThought,
}

enum ReasoningEffort {
    None,    // 明确关闭推理（部分新模型支持）
    Low,
    Medium,
    High,
    Xhigh,   // 超高强度，部分新模型支持
}
```

`ReasoningStyle` 区分两个关键维度：reasoning 内容是否对用户可见（决定 UI 展示），以及历史消息中是否需要透传 reasoning block（决定上下文管理策略）。

### ModelCapabilities 变更

在现有 `ModelCapabilities` 中追加：

```rust
struct ModelCapabilities {
    // ...现有字段不变...
    /// None 表示该模型不支持 reasoning
    reasoning: Option<ReasoningCapability>,
}
```

### TaskRunParams 变更

`reasoning` 是 task 级运行参数，和 `temperature`、`max_output_tokens` 同层管理，不写回 provider 的模型能力表：

```rust
struct TaskRunParams {
    // ...现有参数...
    reasoning_enabled: bool,
    /// Anthropic / Gemini：思考预算 token 数；None 时使用模型默认值
    reasoning_budget_tokens: Option<u32>,
    /// OpenAI o 系列：推理强度；None 时不传该字段（使用模型默认行为）
    reasoning_effort: Option<ReasoningEffort>,
}
```

`reasoning_enabled: false` 与 `reasoning_effort: Some(None)` 语义不同：
- `reasoning_enabled: false`：不在请求中传 reasoning 相关字段，沿用模型默认行为
- `reasoning_effort: Some(ReasoningEffort::None)`：明确传 `"none"`，要求模型内部关闭推理（仅 OpenAI 部分新模型支持）

---

## 上下文管理：Thinking Block 不进入 recent_chat

**核心原则：Thinking block 是轮内瞬态，不写入 `recent_chat`。**

### 问题根源

如果把 thinking block 存入 `recent_chat`，会引发两个根本问题：

1. **堆积问题**：agentic 对话中每轮都可能产生 thinking block（数百至数千 token），`recent_chat` 随对话增长越来越重，与 March "上下文不退化"的核心理念直接矛盾
2. **模型切换兼容问题**：`recent_chat` 里如果含有 Anthropic thinking block（带 signature），切到 DeepSeek / OpenAI 时这些 block 对其他 provider 的 API 是非法字段，无法透传

### 正确的分层

```
轮内（agent loop 进行中）        历史（recent_chat）
─────────────────────────       ────────────────────
第1次 API 调用                   本轮结束后写入：
  ← [thinking, text, tool_use]     只有 final_text（纯文本）
第2次 API 调用                      不含任何 thinking block
  → 携带上次 [thinking, text,
     tool_use] + tool_result
  ← [thinking, text, tool_use]
...最终输出 final_text
轮结束，所有轮内中间状态丢弃
```

轮内的多轮 API 调用需要原样传递 thinking block（Anthropic 的约束仅在"单次 API 请求的 message 序列"内成立：若该序列中包含了某条带 thinking block 的历史 assistant 消息，就不能剥离其 thinking block）。但轮内中间状态本来就是 March 设计中"轮结束后整体丢弃"的内容，写入 `recent_chat` 的永远只有 `final_text`。

**Thinking block 从不进入 `recent_chat`，Anthropic 的透传约束自然满足，且不影响任何 provider 的兼容性。**

### 与 UI 展示的分离

用户在聊天区看到的 reasoning 折叠块，是 UI 层从流式事件（`assistant_stream_delta` with `content_type: 'reasoning'`）实时接收并存储在 **UI message store** 里的——这是纯展示数据，与 AI 上下文完全独立，不参与任何 provider API 调用。

UI message store 以 `turn_id` 为 key 存储 reasoning 块，与 chat 消息共享同一 turn 生命周期。**回退一轮时，reasoning 块随对应消息一同被移除**，两者在同一个 store action 里原子完成，不存在消息已撤销但 reasoning 孤立残留的中间态。详见 → [Turn 快照与回退](turn-snapshot.md)。

### 模型切换兼容性

轮间切换模型（在两条消息之间）：`recent_chat` 只含纯文本，任何 provider 都能直接消费，无需任何转换。

轮内切换模型：March 设计中同一 task 的 agent 轮次串行执行，正常流程下不会在轮内发生模型切换，此场景无需额外处理。

---

## Wire 层归一化

WireAdapter 将各家格式解析为内部 `ReasoningBlock`，仅用于两处消费：
1. 轮内 API 调用时作为历史 assistant 消息的一部分原样透传（仅 Anthropic 需要）
2. 提取 `content` 和 `tokens_used` 推送给 UI 事件层

```rust
struct ReasoningBlock {
    /// 可见的 reasoning 文本；OpenAiHidden 时为 None
    content: Option<String>,
    /// Anthropic thinking block 的原始 signature；仅用于轮内 API 透传，不持久化
    signature: Option<String>,
    /// 本次 reasoning 消耗的 token 数（各家均有，来源字段名不同）
    tokens_used: Option<u32>,
}
```

`AnthropicWire` 在构建**轮内历史消息**时，assistant 消息的 content 保留 thinking block 的原始结构（`{type: "thinking", thinking: "...", signature: "..."}`）。构建 `recent_chat` 时只取 text block，thinking block 不进入。

`InlineTag` 风格的流式处理分两条路径：

- **`reasoning_content` 字段存在时**（DeepSeek 官方 API 等）：streaming chunk 中 `delta.reasoning_content` 非空即 emit `content_type: 'reasoning'`，`delta.content` 非空即 emit `content_type: 'text'`。两者在 chunk 层面互斥，无需额外状态。
- **fallback 到 `<think>` 标签时**：WireAdapter 维护 `in_think: bool` 状态，保留一个不超过 8 字节的尾部 buffer 处理跨 chunk 的标签边界。进入 `<think>` 切换为 reasoning 流，出 `</think>` 切回 text 流，标签本身不 emit 给 UI。完整 response 接收后，同样从 content 中移除该标签，避免持久化时残留。

---

## UI 展示

### 事件模型扩展

`assistant_stream_delta` 增加 `content_type` 字段，区分 reasoning 流和正文流：

```ts
type AssistantStreamDeltaEvent = UiRunEventBase & {
  type: 'assistant_stream_delta'
  content_type: 'reasoning' | 'text'
  delta: string
}
```

对 `OpenAiHidden`，不产生 `content_type: 'reasoning'` 的 delta；reasoning token 用量通过 `turn_finished` 事件携带，在右侧面板的上下文用量区展示。

`turn_finished` 补充 reasoning 用量字段：

```ts
type TurnFinishedEvent = UiRunEventBase & {
  type: 'turn_finished'
  assistant_message_id: string
  final_text: string
  reasoning_tokens_used?: number   // 新增，各家均可填，OpenAiHidden 时这里才有意义
}
```

### 聊天区消息结构

Reasoning 内容以折叠块形式出现在消息主体上方，默认折叠：

```
┌──────────────────────────────────────────────┐
│ March  14:32                                 │
│ ▶ 思考过程  (1,247 tokens · 3.2s)            │  ← 默认折叠
│                                              │
│ 好的，我先看一下认证逻辑……                     │  ← 主回复正文
│ ┌─ read_file src/auth.rs                    │
│ └─ replace_lines 24-51                      │
└──────────────────────────────────────────────┘
```

展开后：

```
│ ▼ 思考过程  (1,247 tokens · 3.2s)            │
│ ┌────────────────────────────────────────┐  │
│ │ 用户想把 auth 模块拆开。先看现有结构…      │  │
│ │ 发现 auth.rs 混杂了 token 验证、会话管     │  │
│ │ 理、权限检查三种职责，应该分三个文件……      │  │
│ └────────────────────────────────────────┘  │
```

视觉规则：
- 折叠块背景色比主回复更低调（更低 opacity 或不同底色），明确区分"推理过程"与"最终回复"
- 流式输出期间：reasoning 流先于 text 流输出，两段有明确分界，不并行渲染
- 历史消息中 thinking 块默认收起，用户可手动展开，刷新后恢复默认折叠态
- `OpenAiHidden`：不显示折叠块，只在右侧面板 token 用量中展示 `reasoning: N tokens`

### 输入框底栏：Reasoning 控件

Reasoning 控件位于输入框底栏，模型选择器旁边。**仅当当前模型的 `ModelCapabilities.reasoning` 不为 `None` 时显示。** 三种 style 对应三种不同形态：

---

**`AnthropicThinking` / `GeminiThought`（预算型）**

收起态：显示当前状态的紧凑标签，点击展开浮层。

```
底栏：  [📁 src] [⚡ claude-sonnet-4-6 ∨] [💡 思考 8K ∨] [↵发送]
```

展开浮层：

```
┌─────────────────────────────────────┐
│ 思考                          [✓ 启用] │
│                                     │
│ 思考预算                             │
│ ●━━━━━━━━━━━━━━━━━━━━━━━━○  [8192  ] │
│ 1K                          32K     │
│                                     │
│ 预算越高，思考越深入，消耗 token 越多    │
└─────────────────────────────────────┘
```

- 关闭"启用"后，预算滑块灰显不可操作，本轮不发送 `thinking` 字段
- 滑块最小值为模型 `min_budget_tokens`（Anthropic 要求至少 1024），最大值为 `max_budget_tokens`
- 数字输入框与滑块双向联动；手动输入时超出范围自动 clamp
- 收起态标签显示当前预算：`思考 8K`（禁用时显示 `思考 关`）

---

**`OpenAiHidden`（档位型）**

收起态：显示当前档位，点击展开浮层。

```
底栏：  [📁 src] [⚡ o4-mini ∨] [💡 medium ∨] [↵发送]
```

展开浮层（只展示 `supported_efforts` 中声明的档位）：

```
┌──────────────────────────────────┐
│ Reasoning Effort                 │
│  ○ low   ● medium   ○ high   ○ xhigh │
└──────────────────────────────────┘
```

- 不提供"关闭"选项，除非 `supported_efforts` 包含 `None`
- 若包含 `None`，在最左侧增加 `○ off` 档位，选中时收起态显示 `思考 关`
- 各档位 hover 时可附短说明（快 / 均衡 / 深入 / 最深入），不强制显示

---

**`InlineTag`（DeepSeek-R1 / QwQ，开关型）**

模型自动控制思考程度，无可配参数，只提供开关。

```
底栏：  [📁 src] [⚡ deepseek-r1 ∨] [💡 思考 ∨] [↵发送]
```

展开浮层：

```
┌──────────────────────────┐
│ 思考                [✓ 启用] │
│ 思考程度由模型自动决定       │
└──────────────────────────┘
```

- 禁用后收起态显示 `思考 关`；启用时只显示 `思考`（无预算数字）

> **Wire 层行为说明**：对 `InlineTag` 风格，"禁用"仅为展示层行为。WireAdapter 照常接收响应，reasoning 内容被丢弃，不产生 `content_type: 'reasoning'` 事件，也不存入 UI message store。Token 消耗不因此减少。如需真正节省 token，应切换到该模型的非 thinking 变体（如 `deepseek-chat`）。浮层中应附一行说明："关闭后思考内容不展示，但 token 消耗可能仍存在。"

---

**控件通用规则**

- 所有 reasoning 设置都是 **task 级**，切换 task 时恢复该 task 自己的设置
- 新建 task 时，reasoning 初始状态来自当前模型的 `ReasoningCapability.default_budget_tokens` 和默认 effort，`reasoning_enabled` 默认 `true`（模型支持时默认开启）
- 切换模型后，若新模型不支持 reasoning，底栏控件消失；若新模型支持但 style 不同，重置为新模型的默认值

---

## 设置页：能力编辑表单

在现有能力编辑表单（`[编辑能力]`）末尾追加 Reasoning 区：

```
Reasoning
  [ ] 支持 Reasoning        风格  [ Anthropic Thinking ∨ ]
  默认预算  [8192  ] tokens   最大预算  [32000 ] tokens
```

已知 provider 的已知模型（claude-sonnet-4-6、DeepSeek-R1 等）由 March 预填，用户可覆盖。自定义模型由用户手动勾选。

---

## 内置模型 Reasoning 能力表（已知）

| 模型 | Style | 最小 budget | 默认 budget | 最大 budget | Supported Efforts |
|------|-------|-------------|-------------|-------------|-------------------|
| claude-opus-4-6 | AnthropicThinking | 1,024 | 8,000 | 32,000 | — |
| claude-sonnet-4-6 | AnthropicThinking | 1,024 | 8,000 | 16,000 | — |
| claude-haiku-4-5 | **不支持** | — | — | — | — |
| o3 | OpenAiHidden | — | — | — | low, medium, high |
| o4-mini | OpenAiHidden | — | — | — | low, medium, high, xhigh |
| deepseek-r1 | InlineTag | — | — | — | — |
| gemini-2.0-flash-thinking | GeminiThought | 0 | 8,000 | 24,576 | — |
| gemini-2.5-pro | GeminiThought | 0 | 8,000 | 32,768 | — |
| gemini-2.5-flash | GeminiThought | 0 | 8,000 | 24,576 | — |

注：
- 未列出的模型默认 `reasoning: None`；用户可在设置页能力编辑表单手动覆盖。
- OpenAI 新模型随 API 更新可能扩展 `none` / `xhigh` 支持，应在内置表中及时跟进。
