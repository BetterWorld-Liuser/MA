# Provider 设计

> 从 [DESIGN.md](DESIGN.md) 延伸：March 自己管上下文构建，provider 层只负责把构建好的上下文发出去、把结果收回来。

---

## 核心概念

```
Provider（来源通道：凭证 + 端点 + 协议类型）
    ↓ 激活或挂接模型
    ↓
ModelConfig（运行实体，扁平列表，用户日常面对的）
```

两者关系是**来源 vs 实体**，不是父子层级。ModelConfig 记录自己从哪个 Provider 激活，运行时只看 ModelConfig。用户的心智模型是"我想用某个模型"，而不是"我想配置某个 Provider"。

---

## 选型：自建 wire format 层

早期使用 `genai` 作为 provider 抽象层，后因 server-side tools 原生注入、Anthropic cache_control content block 级别控制等需求超出 genai 的抽象边界，改为自建 wire format 层。自建层按 provider 协议族分为 OpenAiWire / AnthropicWire / GeminiWire 三个适配器，直接用 reqwest 发 HTTP 请求。

---

## 与上下文管理的分工

```
AgentContext（March 自建）
    │
    │  每轮构建完毕后
    ▼
WireAdapter（wire format 层）
    │
    │  reqwest 发出请求 / SSE 收 stream
    ▼
Provider（Claude / GPT / Gemini / ...）
```

March 的 `AgentContext` 决定内容和顺序，`WireAdapter` 负责把 `RequestMessage` 序列化为各家 wire format 并处理响应解析。

`ProviderType` 同时承担"来源通道类型"和"运行协议选择"两层语义。运行时翻译层按 `ProviderType` 选择 `WireAdapter`，不再单独持久化 `WireFormat` 字段：

```
Anthropic                  → AnthropicWire（Anthropic messages API）
Gemini                     → GeminiWire（generateContent API）
其余所有类型（OpenAi、OpenAiCompat、Fireworks、Together、Groq …）
                           → OpenAiWire
    ├─ OpenAi              → /responses 端点（OpenAI Responses API）
    └─ 其余                → /chat/completions 端点
```

`OpenAiWire` 内部还会根据是否注入了需要 `/responses` 格式的 server-side tool 来决定最终端点（`should_use_openai_responses_api` 检查）。

同样是"OpenAI 风格"的 server-side tool，也需要区分 `/responses`（`ServerToolFormat::OpenAiResponses`）和 `/chat/completions`（`ServerToolFormat::OpenAiChatCompletions`）两套注入格式。这个分流属于 provider 兼容职责，不应上浮到 AgentContext。

---

## Cache Control 映射

```
[system prompt]           ← cache_control: Ephemeral1h
[未修改的文件们]           ← cache_control: Ephemeral1h
[被修改过的文件]           ← 无 cache（变化频繁，缓存无意义）
[对话历史]                ← 无 cache
[最新 user message]       ← 无 cache
```

---

## Provider 配置数据模型

### 数据结构

```rust
struct ProviderConfig {
    id:            i64,
    name:          String,         // 用户自定义显示名，如 "Anthropic"、"Local Ollama"
    provider_type: ProviderType,
    api_key:       String,         // 明文存储（见安全说明）
    base_url:      Option<String>, // openai_compat 必填，其余可覆盖默认端点
}

enum ProviderType {
    // 协议原生：各有专属 WireAdapter
    Anthropic,      // → AnthropicWire
    Gemini,         // → GeminiWire
    OpenAi,         // → OpenAiWire，走 /responses 端点
    // 以下均走 OpenAiWire + /chat/completions 端点
    OpenAiCompat,   // 自定义端点（必须填 base_url）
    Fireworks,
    Together,
    Groq,
    Mimo,
    Nebius,
    Xai,
    DeepSeek,
    Zai,
    BigModel,
    Cohere,
    Ollama,         // 本地，默认 http://localhost:11434/v1，不需要 api_key
}

/// ModelConfig 是运行时一级实体，扁平列表，用户日常面对的
struct ModelConfigRecord {
    id:               i64,
    display_name:     Option<String>,   // 用户可重命名；None 时显示 model_id
    model_id:         String,           // API 调用实际使用的 ID，如 "claude-sonnet-4-6"
    provider_id:      i64,              // 来源通道
    context_window:   usize,
    max_output_tokens: usize,
    supports_tool_use: bool,
    supports_vision:   bool,
    supports_audio:    bool,
    supports_pdf:      bool,
    probed_at:        Option<i64>,      // 最后一次能力探测时间戳（Unix 秒）；None 表示从未探测
    server_tools:     Vec<ServerToolConfig>,
}

/// 模型能力描述，session 初始化时一次性解析，各模块按需消费
struct ModelCapabilities {
    context_window: u32,         // 最大输入 token 数
    max_output_tokens: u32,      // 最大输出 token 数
    supports_tool_use: bool,     // 工具调用（function calling）
    supports_vision: bool,       // 图片输入
    supports_audio: bool,        // 音频输入（预留）
    supports_pdf: bool,          // PDF 原生输入（预留）
    server_tools: Vec<ServerToolConfig>,  // provider 原生 server-side tools
}

/// Provider 原生的 server-side tool 配置
/// 这类工具由 provider 侧执行，March 只负责在请求中注入 tool 定义，不介入执行
struct ServerToolConfig {
    capability: ServerToolCapability,
    format: ServerToolFormat,     // 已知 provider 自动匹配；compat 由用户选择
}

enum ServerToolCapability {
    WebSearch,       // 联网搜索
    CodeExecution,   // 沙箱代码执行
    FileSearch,      // 文件/向量检索（目前仅 OpenAI）
}

/// 各家对同一能力的 wire format 不同，翻译层按此枚举注入对应的 tool 定义
enum ServerToolFormat {
    Anthropic,              // DB: "anthropic"
    OpenAiResponses,        // DB: "openai_responses"
    OpenAiChatCompletions,  // DB: "openai_chat_completions"
    Gemini,                 // DB: "gemini"
}

/// 直接引用 ModelConfig.id，不再是 (provider_id, model_id) 组合
struct DefaultModel {
    model_config_id: i64,
}
```

### 安全说明

API key 明文存储在 `~/.march/settings.db`，文件权限设为 `600`（仅所有者可读写）。

OS keychain 集成是 MVP 后的优化项，当前不做。

---

## Storage Schema

存储在 `~/.march/settings.db`（用户级，不进 git）：

```sql
CREATE TABLE providers (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL,
    provider_type TEXT    NOT NULL DEFAULT 'openai_compat',
    api_key       TEXT    NOT NULL,
    base_url      TEXT    NOT NULL DEFAULT '',  -- 空字符串 = 使用该类型的默认端点
    created_at    INTEGER NOT NULL
);

-- 所有用户已激活/接入的模型
-- 已知 provider（Anthropic/OpenAI/Gemini）内置能力由 March 预填写入此表
CREATE TABLE model_configs (
    id               INTEGER PRIMARY KEY,
    provider_id      INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    model_id         TEXT    NOT NULL,
    display_name     TEXT    NOT NULL DEFAULT '',  -- 空字符串 = 显示时使用 model_id
    context_window   INTEGER NOT NULL DEFAULT 131072,  -- tokens
    max_output       INTEGER NOT NULL DEFAULT 4096,    -- tokens（注意列名是 max_output）
    supports_tool_use INTEGER NOT NULL DEFAULT 0,
    supports_vision   INTEGER NOT NULL DEFAULT 0,
    supports_audio    INTEGER NOT NULL DEFAULT 0,
    supports_pdf      INTEGER NOT NULL DEFAULT 0,
    probed_at         INTEGER,                         -- Unix 秒，NULL = 从未探测
    UNIQUE(provider_id, model_id)
);

-- provider 原生 server-side tools，按 model_config_id 引用
CREATE TABLE model_server_tools (
    id               INTEGER PRIMARY KEY,
    model_config_id  INTEGER NOT NULL REFERENCES model_configs(id) ON DELETE CASCADE,
    capability       TEXT    NOT NULL,
    -- 'web_search' | 'code_execution' | 'file_search'
    format           TEXT    NOT NULL,
    -- 'anthropic' | 'openai_responses' | 'openai_chat_completions' | 'gemini'
    UNIQUE(model_config_id, capability)
);

-- agent profiles（角色），绑定到 model_config
CREATE TABLE agent_profiles (
    id              INTEGER PRIMARY KEY,
    name            TEXT    NOT NULL UNIQUE,
    display_name    TEXT    NOT NULL,
    description     TEXT    NOT NULL DEFAULT '',
    system_prompt   TEXT    NOT NULL,
    avatar_color    TEXT    NOT NULL DEFAULT '#64748B',
    model_config_id INTEGER REFERENCES model_configs(id) ON DELETE SET NULL,
    created_at      INTEGER NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- 全局键值设置
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- 用到的 key：
--   default_model_config_id  → model_configs.id
--   custom_system_core       → 自定义系统提示词内容
--   use_custom_system_core   → '1' | '0'
```

---

## Provider 配置路径

### 内置 Provider（Anthropic / OpenAI / Gemini）

March 内置这些 provider 的协议适配、可用模型列表、模型能力、server-side tools。用户只需填写 API key（Ollama 不需要）。

内置模型列表：用户在"管理模型"界面勾选激活，勾选后写入 `model_configs` 表。能力字段由 March 预填，**默认只读**，提供 **[覆盖]** 入口供手动修正，覆盖后可一键恢复内置值。内置列表支持 **[+ 添加自定义 Model ID]**，用于内置列表里尚未收录的新模型（如预览版）。

### 预置第三方 Provider（Fireworks / Together / Groq 等）

这些 provider 使用 `/chat/completions` 协议，March 内置了默认端点（见 `ProviderType::default_base_url()`），用户只需填写 API key，无需填写 base URL。

### 自定义 Provider（OpenAICompat）

用户必须填写 base URL 和 API key。协议族（OpenAI-compatible，走 `/chat/completions`）由 provider type 决定，不做自动探测。

### Ollama

默认端点 `http://localhost:11434/v1`，不需要 API key，走 `/chat/completions`。

---

## 接入与确认流程

### 连通性测试（Provider 级）

`ProviderClient::test_connection()` 向配置的 provider 发送一个最小探测请求，按 `ProviderType` 选择对应 `WireAdapter` 和端点：

- **Anthropic**：`POST {base_url}/messages`
- **Gemini**：`POST {base_url}/models/{model}:generateContent`
- **其余（OpenAI 协议族）**：`POST {base_url}/chat/completions`（或 OpenAi 的 `/responses`）

探测请求：`user: "Return exactly \`MARCH_OK\` and nothing else."` + `max_tokens: 16`。

探测模型（probe model）选择规则：
1. 优先使用 `RuntimeProviderConfig.model`（不为空时直接用）
2. `OpenAiCompat` / `Ollama`：若 model 为空，则先调用 `GET {base_url}/models` 取第一个可用模型
3. 其余类型：model 为空则报错

结论只有"可连通 / 不可连通"，**不承担协议探测职责**。

### 候选模型发现（Provider 级，可选）

`ProviderClient::list_models()` 调用 `GET {base_url}/models`：

- **Anthropic**：无标准 list endpoint，直接返回空列表（前端使用内置模型列表）
- **Gemini**：`GET {base_url}/v1beta/models`
- **其余（OpenAI 协议族）**：`GET {base_url}/models`

候选模型列表只是"辅助选择"；拿不到列表时，仍然允许用户手动填写 `model_id`。

### 能力确认（Model 级）

用户选择或手填 `model_id` 后，进入能力确认表单。按来源类型走两条路径：

- **已知 provider 的已知模型**：由 March 预填能力与 server-side tools，用户可按需覆盖
- **自定义 provider，或已知 provider 下的自定义 `model_id`**：给出保守默认值，由用户手动勾选

能力也可以通过显式探测确认（`probe_tool_use_support` / `probe_vision_support`），这两个方法发送最小测试请求来验证单项能力，需要由用户主动触发，不会自动运行。探测完成后 `probed_at` 写入时间戳。

| 字段 | 来源 | 默认值策略 |
|------|------|-----------|
| tool_use | 内置表或用户输入 | 未知模型默认关闭 |
| vision | 内置表或用户输入 | 未知模型默认关闭 |
| audio | 内置表或用户输入 | 未知模型默认关闭 |
| pdf | 内置表或用户输入 | 未知模型默认关闭 |
| context_window | 内置表或用户输入 | 未知模型给保守默认值 |
| max_output | 内置表或用户输入 | 未知模型给保守默认值 |

Server-side tools 按 provider type 给出候选项，由用户确认（默认全部关闭）：

| Provider Type | 可能支持的 server-side tools |
|---------------|---------------------------|
| Anthropic | web_search、code_execution |
| OpenAI | web_search_preview、code_interpreter、file_search |
| Gemini | google_search、code_execution |

---

## 模型能力解析

模型能力统一收敛到 `ModelCapabilities`，在 session 初始化时一次性解析好，写入 session 状态，各模块按需消费。

### 解析优先级

```
1. 用户手动覆盖（通过设置页"编辑能力"修改，存入 model_configs 表）
2. March 内置的模型能力表（已知 provider 的已知模型）
3. 保守默认值（无工具调用、纯文本、128K context、4K output、无 server-side tools）
```

不再通过 provider `/models` 接口做 best-effort 能力解析，目的是把"模型能做什么"收敛为一份稳定配置，而不是引入昂贵且不可靠的探测流水线。

### 消费方

| 消费方 | 使用的能力字段 |
|--------|-------------|
| 上下文预算（右侧面板 context usage） | `context_window` |
| agent loop 可用性 | `supports_tool_use` → 不支持时降级为纯对话模式，不注入任何工具定义 |
| 工具集动态裁剪 | `supports_vision` → 决定是否注入 `view_image` 工具 |
| 图片输入通道 | `supports_vision` → 决定是否允许粘贴/拖入图片、`@` 引用图片文件 |
| 输出截断 | `max_output_tokens` |
| server-side tools 注入 | `server_tools` → 翻译层按 format 注入对应的 provider 原生 tool 定义 |

**工具集不是固定的，而是根据当前模型能力动态裁剪。** 模型不支持图片时，`view_image` 不出现在 tools 列表里，聊天框的图片粘贴入口也应禁用或隐藏。

### Server-side Tools 格式映射

翻译层按 `(capability, format)` 查表注入对应的 tool 定义：

| Capability | Anthropic 格式 | OpenAI `/responses` | OpenAI-compatible `/chat/completions` | Gemini 格式 |
|------------|---------------|---------------------|--------------------------------------|-------------|
| WebSearch | `type: "web_search_20250305"` | `type: "web_search"` | `type: "web_search_preview"` | `google_search` retrieval |
| CodeExecution | `type: "code_execution_20250522"` | `type: "code_interpreter"` | `type: "code_interpreter"` | `code_execution` |
| FileSearch | — | `type: "file_search"` | `type: "file_search"` | — |

注意：各家的 tool type 字符串带版本后缀（如 Anthropic 的 `web_search_20250305`），March 内置表需要跟随 provider API 版本更新维护。

---

## 运行时模型解析

任务创建时，按以下优先级把运行入口写入该 task：

```
1. 当前默认运行配置（settings.default_model_config_id）
2. 环境变量 fallback（开发态 / 无设置页配置时）
3. 硬编码 fallback：提示用户去设置页配置
```

进入聊天运行时，优先级改为：

```
1. task 持久化的 selected_model_config_id（+ selected_model 字符串，两者均存储）
2. 仅对历史旧 task 做兼容时，回退到当前默认运行配置
3. 环境变量 fallback
```

`TaskRecord` 同时存储 `selected_model_config_id: Option<i64>` 和 `selected_model: Option<String>`（model_id 字符串）。两者并存：`model_config_id` 用于正常运行时查找完整配置，`selected_model` 字符串保留是为了在 model_config 被删除等边缘情况下仍能显示模型名称。

这意味着：

- 输入框模型选择器是 **task 级** 状态，不是 session 级临时状态
- 温度、输出上限等运行参数也属于 **task 级** 状态；同一模型在不同任务里允许使用不同参数
- 用户在设置页修改"默认运行配置"时，只影响**之后新建的任务**
- 已存在任务的模型绑定保持稳定，不应被新的全局默认值回刷

模型选择器的下拉列表是扁平的 `ModelConfig` 列表，不再按 Provider 分组：

```
✓ Claude Sonnet 4.6
  Qwen 2.5 Coder
  GPT-5.4
```

## 流式稳健性策略

provider 层默认仍然是"流式优先"，因为 UI 需要即时增量输出；但 March 不应把"某家 provider 的 stream 兼容性不好"直接升级成整个回合失败。

运行时策略：

```
首次请求
  └─ 先尝试 stream
       ├─ 成功：记录该 provider/model 可稳定流式，后续继续走 stream
       └─ 失败：自动降级到非流式 exec_chat
                并把该 provider/model 标记为本进程内优先非流式
```

设计约束：

- 能力探测是 runtime 行为，不写死在静态 provider 类型判断里
- 探测粒度至少包含 `provider_type + base_url + model`，避免把一个兼容端点的失败误伤到所有 provider
- 降级逻辑收敛在 provider 翻译层，agent loop 只消费统一的 `ProviderResponse`
- debug 信息里要保留本次实际走的是 `streaming`、`non_streaming_cached` 还是 `non_streaming_fallback`

---

## Provider 客户端初始化

每个 provider 在 session 启动时初始化 `ProviderClient`，内部按 `ProviderType` 选择对应的 `WireAdapter`（OpenAiWire / AnthropicWire / GeminiWire），配合 reqwest HTTP 客户端完成请求。详见 [provider-core.md](provider-core.md)。

---

## 设置页 UI

从左栏标题点击进入，覆盖整个窗口。设置页明确分成两个一级入口：**模型** 和 **供应商**：

- **供应商**负责"这条通道怎么连、属于哪类协议通道、凭据和连通性"
- **模型**负责"这个运行实体有什么能力、如何默认使用"

### 供应商页

```
[供应商]

  Anthropic        ✓ 已配置    [管理模型] [编辑 Key] [删除]
  中转站A · OpenAI-compatible  [管理模型] [编辑]     [删除]

  [+ 添加通道]
```

### 新增 / 编辑供应商

```
类型    [Anthropic ∨]
名称    [Anthropic        ]
API Key [sk-ant-...       ]  [显示/隐藏]
Base URL[                 ]  （可选，留空使用默认端点）

                  [测试连通性]  [取消]  [保存]
```

- 供应商类型由用户显式选择，连通性测试不承担协议探测职责
- 保存前必须通过连通性测试，或用户显式跳过
- 删除 provider 前：若有 session 正在使用该 provider，弹出确认提示

### 管理模型（官方 Provider）

```
Anthropic 可用模型

  [✓] claude-sonnet-4-6    200K · 16K · 工具 · 图片 · 搜索 · 代码执行   [覆盖]
  [✓] claude-opus-4-6      200K · 32K · 工具 · 图片 · 搜索 · 代码执行   [覆盖]
  [ ] claude-haiku-4-5     200K · 8K  · 工具 · 图片
  ─────────────────────────────────────────────────────
  [+ 添加自定义 Model ID]
```

- 勾选即激活，写入 `model_configs` 表
- 能力字段由 March 预填，**默认只读**，**[覆盖]** 入口供手动修正；覆盖后显示标记，可一键恢复内置值
- **[+ 添加自定义 Model ID]**：用于内置列表里尚未收录的新模型

### 管理模型（中转站）流程

```
1. 按已选 provider type 做连通性测试
2. 若端点支持，则拉取候选模型列表；否则直接进入手动填写 model_id
3. 用户选择或填写要添加的模型
4. March 按 provider type 给出能力表单默认值
5. 用户确认能力与 server-side tools 后加入[模型]列表
```

### 模型页

```
[模型]

  Claude Sonnet 4.6     Anthropic · 工具 · 图片 · 搜索 · 代码执行   [编辑能力] [删除]  [设为默认]
  Qwen 2.5 Coder        中转站A · OpenAI-compatible · 工具          [编辑能力] [删除]
```

- 默认模型直接在模型卡片上标记，不再有单独的"默认运行"页面
- 已设为默认的模型显示"默认"标记
- 默认标记只决定之后新建任务的初始模型，已存在任务不受影响

### 能力编辑表单

**[编辑能力]** 对所有模型都可用。已知 provider 的内置模型以 March 预填值为起点，自定义 provider 的模型直接编辑当前确认值：

```
模型 ID      [qwen2.5-coder:32b    ]
显示名称     [Qwen 2.5 Coder 32B   ]  （可选，留空则用 model_id）
来源供应商   [ 中转站A · OpenAI-compatible ∨ ]
上下文窗口   [131072                ]  tokens
最大输出     [8192                  ]  tokens

能力
  [✓] 工具调用  [✓] 图片    [ ] 音频    [ ] PDF

Server-side Tools                        格式
  [✓] Web Search      [OpenAI (web_search_preview) ∨]
  [ ] Code Execution   —
  [ ] File Search      —
```

- 来源供应商字段展示协议类型（如 `益丰 · OpenAI-compatible`），协议信息属于来源通道的派生属性，切换来源供应商时协议显示随之更新，不在模型页单独编辑
- Server-side tool 勾选后需选择格式（即背后实际对接的 provider 协议）
- 格式下拉列表按能力过滤：Web Search 可选 Anthropic / OpenAI / Gemini，File Search 仅 OpenAI

---

## 待决策

- [ ] tool schema / tool prompt 如何携带"本轮会话探测到的可用 shell 列表"？这部分应由 March 注入运行时信息，而不是写死在静态提示词中。
