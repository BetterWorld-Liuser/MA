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

## 上下文管理：Anthropic Thinking Block 透传

**核心约束**：Anthropic 的 thinking block 带 `signature` 校验，历史消息中已出现的 thinking block **不能被删除或修改**，否则 API 返回错误。

**结论：March 的整条截断策略与该约束不冲突。**

具体推导：

1. March 的 `recent_chat` 截断以**整条消息**为粒度——超出窗口的消息整条不携带
2. 未被截断的消息，其 content blocks（包括 thinking block + signature）原样进入本轮上下文
3. Anthropic 禁止的是"携带了某条消息但剥离了其中的 thinking block"；整条不携带不违反该规则

因此只需保证：**截断时不做 content block 级别裁剪**，不单独剥离 thinking block 而保留 text block。

**Token 预算影响**：

thinking block 通常远大于普通文字（数百至数千 token），`recent_chat` 保留轮次数 N 在有 reasoning 的对话中实际消耗更重。上下文预算系统应以 **token 数**而非消息条数为截断依据。这与现有上下文压力管理设计一致，无需额外处理。

---

## Wire 层归一化

WireAdapter 将各家格式解析为内部 `ReasoningBlock`，供上层消费：

```rust
struct ReasoningBlock {
    /// 可见的 reasoning 文本；OpenAiHidden 时为 None
    content: Option<String>,
    /// Anthropic thinking block 的原始 signature，透传时需随 content 一起携带
    signature: Option<String>,
    /// 本次 reasoning 消耗的 token 数（各家均有，来源字段名不同）
    tokens_used: Option<u32>,
}
```

`AnthropicWire` 在构建历史消息时，assistant 消息的 content 保留 thinking block 的原始结构（`{type: "thinking", thinking: "...", signature: "..."}`），不做任何转换。

`InlineTag` 风格：WireAdapter 优先读 `reasoning_content` 字段，字段不存在时 fallback 到从 content 中提取 `<think>…</think>` 标签，提取后从 content 中移除该标签，避免主回复里出现残留标记。

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

### 输入框运行参数

在现有"运行参数"入口中增加 reasoning 控制，仅当 `ModelCapabilities.reasoning` 不为 `None` 时展示：

**Anthropic / Gemini 风格**（`AnthropicThinking` / `GeminiThought`）：

```
Reasoning
  [✓] 启用思考              思考预算  [8192    ] tokens
```

**OpenAI o 系列**（`OpenAiHidden`）：

```
Reasoning Effort
  [○ none  ○ low  ● medium  ○ high  ○ xhigh]
```

分段选择器只展示 `supported_efforts` 中声明的档位，不展示当前模型不支持的值。

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

| 模型 | Style | 默认 budget | 最大 budget | Supported Efforts |
|------|-------|-------------|-------------|-------------------|
| claude-opus-4-6 | AnthropicThinking | 8,000 | 32,000 | — |
| claude-sonnet-4-6 | AnthropicThinking | 8,000 | 16,000 | — |
| o3 | OpenAiHidden | — | — | low, medium, high |
| o4-mini | OpenAiHidden | — | — | low, medium, high, xhigh |
| deepseek-r1 | InlineTag | — | — | — |
| gemini-2.0-flash-thinking | GeminiThought | 8,000 | 24,576 | — |

注：OpenAI 新模型（如 GPT-5 系列）随 API 更新可能扩展 `none` / `xhigh` 支持，应在内置表中及时跟进。
