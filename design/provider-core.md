# Provider Core 自建方案

> 从 [provider.md](provider.md) 延伸：替换 `genai` 依赖，March 自己实现 provider wire format 层。

---

## 动机

`genai` 作为中间抽象层，在项目早期降低了对接多家 provider 的成本。但随着 March 对 provider 交互的控制粒度要求提高，genai 的抽象边界开始成为阻碍：

1. **server-side tools 无法注入原生 wire format** — `GenAiTool::with_config` 是 genai 内部概念，不等于直接写入 request body。各家 server-side tool 的注入位置和结构不同（Anthropic 放在 `tools[]` 里作为特殊 type、OpenAI 放在 `tools[]` 里带 `type` 字段、Gemini 放在 `tools[]` 的 `google_search_retrieval` / `code_execution`），genai 没有暴露这层控制。
2. **Anthropic `cache_control`** — genai 支持 message 级别的 cache hint，但 March 的上下文设计需要 content block 级别的精确控制（例如 system prompt 中的不同 block 分别标记不同 TTL）。
3. **翻译层冗余** — March 已经有自己的 `RequestMessage` 类型体系，到 genai 又翻译一遍成 `ChatMessage`，再由 genai 翻译成 wire format。两层翻译增加了调试成本和 bug 面。
4. **流式解析已经自建** — `StreamCollector` 已经在做 tool call 累积、content 拼接等核心工作，genai 的 `ChatStreamEvent` 只是多了一层包装。

---

## 现状盘点

### genai 实际提供的价值

| 能力 | March 已自建程度 | 替换难度 |
|------|----------------|---------|
| 多 provider 端点/认证配置 | `build_service_target` 已自建端点表 | 低 — 搬过来即可 |
| `ChatRequest` 构建 | `RequestMessage` 已是独立类型 | 低 — 直接序列化 |
| OpenAI 兼容 wire format | `RequestMessage` 本身就是 OpenAI 格式 | 极低 |
| Anthropic wire format | 无 | 中 — 需要写 messages → content blocks 转换 |
| Gemini wire format | 无 | 中 — 需要写 messages → contents/parts 转换 |
| SSE stream 解析 | `StreamCollector` 处理业务逻辑 | 低 — 加 `eventsource-stream` 解析原始 SSE |
| tool call 提取 | `StreamCollector` + `build_provider_response_from_chat_response` | 已自建 |
| 非流式响应解析 | `build_provider_response_from_chat_response` | 已自建 |

### March 自有类型体系（保持不变）

- `RequestMessage` — 请求消息
- `ProviderResponse` / `ProviderToolCall` — 响应
- `ProviderProgressEvent` / `ProviderToolCallDelta` — 流式事件
- `ToolDefinition` / `ToolParameter` — 工具定义
- `ServerToolConfig` / `ServerToolCapability` / `ServerToolFormat` — server-side tools
- `RuntimeProviderConfig` — 运行时 provider 配置

### 可直接保留的模块

- `delivery.rs` — stream 降级逻辑（`ProviderDeliveryMode`、capability cache）不依赖 genai
- `transport.rs` 中的端点表（`default_endpoint_for_provider`）和模型列表解析（`list_model_descriptors`）
- `title.rs` — 标题生成逻辑（改为走新的请求路径即可）
- `messages.rs` 中的上下文渲染函数（`render_context_body`、`render_injections` 等）

---

## 架构设计

### Wire Format 适配层

```
RequestMessage[] + ToolDefinition[] + ServerToolConfig[]
        │
   WireAdapter trait
        │
        ├── OpenAiWire
        │     直接序列化 RequestMessage，覆盖：
        │     OpenAI, OpenAICompat, DeepSeek, Groq, Together,
        │     Fireworks, xAI, Nebius, Mimo, Zai, BigModel,
        │     Cohere, Ollama
        │
        ├── AnthropicWire
        │     RequestMessage → Anthropic Messages API 格式
        │     system 提取、content blocks、cache_control、
        │     tool_result content block
        │
        └── GeminiWire
              RequestMessage → Gemini generateContent 格式
              contents/parts 结构
```

### WireAdapter 职责

```rust
trait WireAdapter {
    /// 构建完整的 HTTP 请求 body（JSON）
    fn build_request_body(
        &self,
        messages: &[RequestMessage],
        tools: &[ToolDefinition],
        server_tools: &[ServerToolConfig],
        options: &RequestOptions,
    ) -> Result<Value>;

    /// 构建请求 URL（基于 base_url 和 model）
    fn chat_endpoint(&self, base_url: &str, model: &str) -> String;

    /// 构建请求 headers（auth、content-type、provider 特有 header）
    fn request_headers(&self, api_key: &str) -> HeaderMap;

    /// 从非流式 JSON 响应中提取内容和 tool calls
    fn parse_response(&self, body: &Value) -> Result<WireResponse>;

    /// 从 SSE data 字段中提取流式 delta
    fn parse_stream_event(&self, data: &str) -> Result<Option<WireStreamDelta>>;

    /// 判断 SSE 流是否结束（OpenAI 用 [DONE]，Anthropic 用 message_stop event）
    fn is_stream_done(&self, data: &str) -> bool;
}
```

### 请求流程

```
ProviderClient::complete_context_with_events
    │
    │  1. 选择 WireAdapter（按 ProviderType）
    │  2. adapter.build_request_body(...)
    │  3. adapter.chat_endpoint(...) + adapter.request_headers(...)
    │
    ├── streaming path:
    │     reqwest POST with SSE
    │     eventsource-stream 解析原始字节流
    │     adapter.parse_stream_event(data) → WireStreamDelta
    │     StreamCollector 累积 → ProviderProgressEvent 回调
    │     adapter.is_stream_done(data) → 结束
    │
    └── non-streaming path:
          reqwest POST → JSON response
          adapter.parse_response(&body) → WireResponse
          转换为 ProviderResponse
```

### 中间类型

```rust
/// WireAdapter 返回的统一解析结果（非流式）
struct WireResponse {
    content: Option<String>,
    tool_calls: Vec<WireToolCall>,
}

struct WireToolCall {
    id: String,
    name: String,
    arguments_json: String,
}

/// WireAdapter 返回的流式增量
enum WireStreamDelta {
    ContentDelta(String),
    ToolCallDelta {
        index: usize,        // tool call 在数组中的位置
        id: Option<String>,  // 首个 chunk 才有
        name: Option<String>,
        arguments_fragment: String,
    },
    Done,
}
```

注意：`WireStreamDelta` 是 wire 层的原始 delta，粒度比 `ProviderProgressEvent` 更细。`StreamCollector` 负责把连续的 `arguments_fragment` 拼接成完整 JSON，再向上层发出 `ProviderProgressEvent::ToolCallsUpdated`。

---

## 三种 Wire Format 要点

### OpenAiWire

最简单的实现。`RequestMessage` 本身就是 OpenAI 兼容格式，加上 `#[derive(Serialize)]` 和少量字段调整即可直接作为 request body。

**请求结构：**
```json
{
  "model": "gpt-5.4",
  "messages": [ ... ],          // RequestMessage 直接序列化
  "tools": [ ... ],             // function tools + server-side tools
  "temperature": 0.2,
  "stream": true
}
```

**server-side tools 注入：**
```json
{
  "type": "web_search_preview"
}
```
直接追加到 `tools[]` 数组，与 function tools 平级。

**SSE 格式：** 标准 OpenAI SSE，`data: [DONE]` 结束。

**覆盖范围：** 所有 OpenAI 兼容 provider。各家差异主要在端点 URL 和认证 header，wire format 相同。

### AnthropicWire

**请求结构差异：**
```json
{
  "model": "claude-sonnet-4-6",
  "system": [                    // system 从 messages 提取，变成顶层字段
    {
      "type": "text",
      "text": "...",
      "cache_control": { "type": "ephemeral" }   // content block 级别
    }
  ],
  "messages": [
    {
      "role": "user",
      "content": [               // content 始终是 array of blocks
        { "type": "text", "text": "..." },
        { "type": "image", "source": { ... } }
      ]
    },
    {
      "role": "assistant",
      "content": [
        { "type": "text", "text": "..." },
        { "type": "tool_use", "id": "...", "name": "...", "input": { ... } }
      ]
    },
    {
      "role": "user",
      "content": [
        { "type": "tool_result", "tool_use_id": "...", "content": "..." }
      ]
    }
  ],
  "tools": [
    { "name": "run_command", "description": "...", "input_schema": { ... } },
    { "type": "web_search_20250305" },              // server-side tool
    { "type": "code_execution_20250522" }            // server-side tool
  ],
  "max_tokens": 16384
}
```

**关键转换点：**
- `role: "system"` 的 messages 提取合并到顶层 `system` 字段
- `role: "tool"` + `tool_call_id` → `role: "user"` + `type: "tool_result"` content block
- assistant 的 tool calls → `type: "tool_use"` content block（不是 `tool_calls` 字段）
- function tool schema 字段叫 `input_schema`（不是 `parameters`）
- 必须显式传 `max_tokens`

**SSE 格式：** Anthropic 自有格式，`event: message_stop` 结束。需要按 `event:` 行的类型分发：
- `content_block_start` — 新 content block 开始（text 或 tool_use）
- `content_block_delta` — 增量内容
- `content_block_stop` — block 结束

**cache_control：** 可以在 system blocks、message content blocks 上标记，精确控制缓存粒度。这是自建的核心收益之一。

**认证 header：**
```
x-api-key: {api_key}
anthropic-version: 2023-06-01
```

### GeminiWire

**请求结构差异：**
```json
{
  "system_instruction": {
    "parts": [{ "text": "..." }]
  },
  "contents": [
    {
      "role": "user",
      "parts": [{ "text": "..." }]
    },
    {
      "role": "model",              // 不是 "assistant"
      "parts": [
        { "text": "..." },
        {
          "functionCall": {          // 不是 tool_calls
            "name": "run_command",
            "args": { ... }
          }
        }
      ]
    },
    {
      "role": "user",               // tool result 也是 user role
      "parts": [{
        "functionResponse": {
          "name": "run_command",
          "response": { "result": "..." }
        }
      }]
    }
  ],
  "tools": [
    {
      "functionDeclarations": [     // function tools 包在这里
        { "name": "...", "description": "...", "parameters": { ... } }
      ]
    },
    { "googleSearch": {} },         // server-side tool
    { "codeExecution": {} }         // server-side tool
  ],
  "generationConfig": {
    "temperature": 0.2,
    "maxOutputTokens": 16384
  }
}
```

**关键转换点：**
- `assistant` → `model`
- `tool_calls` → `functionCall` parts
- `tool` role → `user` role + `functionResponse` part
- tools 结构：function tools 包在 `functionDeclarations[]` 里，server-side tools 是平级的独立对象
- 端点 URL 包含 model：`/models/{model}:generateContent` 或 `:streamGenerateContent`

**认证：** query parameter `?key={api_key}`（不是 header）。

**SSE 格式：** 每个 `data:` 行是一个完整的 JSON 对象，直接包含 `candidates[0].content.parts`。

---

## 依赖变化

### 移除

- `genai` — 整个 crate

### 新增

- `eventsource-stream` — SSE 字节流解析（将 `reqwest` 的 `bytes_stream()` 转成 SSE 事件迭代器）
- `reqwest` — 已有，保持不变

### 保持

- `serde` / `serde_json` — 已有
- `futures-util` — 已有（`StreamExt`）
- `anyhow` — 已有

---

## 实施路径

### Phase 1：OpenAI-compat wire（最大覆盖面）

1. 实现 `OpenAiWire`，`RequestMessage` 直接序列化为 request body
2. 实现 SSE 解析（`eventsource-stream` + `parse_stream_event`）
3. server-side tools 原生注入
4. 替换 `ProviderClient` 中 OpenAI-compat 路径的 genai 调用
5. 验证：OpenAI / DeepSeek / Groq 等 compat provider 跑通

### Phase 2：Anthropic wire

1. 实现 `AnthropicWire`，RequestMessage → Anthropic Messages API 转换
2. system 提取、content blocks、tool_use/tool_result 转换
3. cache_control content block 级别注入
4. Anthropic SSE 格式解析
5. server-side tools（`web_search_20250305`、`code_execution_20250522`）原生注入

### Phase 3：Gemini wire

1. 实现 `GeminiWire`，RequestMessage → Gemini generateContent 转换
2. role 映射、functionCall/functionResponse parts
3. Gemini SSE 格式解析
4. server-side tools（`googleSearch`、`codeExecution`）原生注入

### Phase 4：清理

1. 移除 `genai` 依赖
2. 清理 `transport.rs` 中的 `AdapterKind` 映射
3. `suggest_task_title` 和 `test_connection` 改走新路径

---

## 与 provider.md 的关系

本文档替换 `provider.md` 中的 **选型：genai** 一节。其余设计（数据模型、storage schema、设置页 UI、运行时模型解析、流式稳健性策略）保持不变，只是底层从 genai 换成自建 wire format 层。

`provider.md` 中的以下内容需要同步更新：
- "选型：genai" → 引用本文档
- "genai 客户端初始化" → 替换为 `WireAdapter` 选择逻辑
- 分工图中的 `genai::ChatRequest（翻译层）` → `WireAdapter（wire format 层）`
