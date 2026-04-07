# Provider / Model 架构重构变更说明

> 本文档描述对 [provider.md](provider.md) 的计划变更，实现完成后合并入主文档。

---

## 变更动机

现有设计以 Provider 为主要配置实体，Model 作为 Provider 的子配置。但用户的心智模型是"我想用某个模型"，而不是"我想配置某个 Provider"。此次重构将 Model 提升为运行时一级实体，Provider 降级为模型的来源通道（凭证 + 端点）。

---

## 核心概念变化

### 之前

```
Provider（一级实体，用户直接配置）
  └── Model（子配置，挂在 Provider 下）
```

### 之后

```
Provider（来源通道：凭证 + 端点）
    ↓ 扫描 /models，用户选模型，探测协议和能力
    ↓
Model（运行实体，扁平列表，用户日常面对的）
```

两者关系是**来源 vs 实体**，不是父子层级。Model 记录自己从哪个 Provider 激活，但运行时只看 Model。

---

## 新增：WireFormat 类型

```rust
/// 独立出来作为 Model 级别的确认字段
/// 原来隐含在 ProviderType 里，现在显式持有
enum WireFormat {
    OpenAI,      // OpenAI / OpenAI-compat
    Anthropic,
    Gemini,
}
```

`ProviderType` 保持不变，仍用于决定探测顺序和默认端点。`WireFormat` 是探测后写入 `ModelConfig` 的确认值，运行时翻译层按此选择 `WireAdapter`。

---

## 数据模型变更

### ProviderConfig（基本不变）

```rust
struct ProviderConfig {
    id:            i64,
    name:          String,
    provider_type: ProviderType,
    api_key:       String,
    base_url:      Option<String>,
}
```

### ModelConfig（原 ProviderModel，提升为一级实体）

```rust
struct ModelConfig {
    id:            i64,
    display_name:  String,           // 用户可重命名
    model_id:      String,           // API 调用实际使用的 ID
    provider_id:   i64,              // 来源通道
    wire_format:   WireFormat,       // 探测确认的协议
    capabilities:  ModelCapabilities,// 探测确认 + 用户可覆盖
    probed_at:     Option<i64>,      // 上次探测时间戳（Unix ms）
}
```

### DefaultModel（简化）

```rust
// 之前：(provider_id, model_id) 组合
// 之后：直接引用 ModelConfig.id
struct DefaultModel {
    model_config_id: i64,
}
```

### Storage Schema 变更

```sql
-- 原 provider_models 表重命名并扩展
CREATE TABLE model_configs (
    id               INTEGER PRIMARY KEY,
    provider_id      INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    model_id         TEXT    NOT NULL,
    display_name     TEXT    NOT NULL,
    wire_format      TEXT    NOT NULL,  -- 'openai' | 'anthropic' | 'gemini'
    context_window   INTEGER NOT NULL DEFAULT 131072,
    max_output       INTEGER NOT NULL DEFAULT 4096,
    supports_tool_use  INTEGER NOT NULL DEFAULT 0,
    supports_vision    INTEGER NOT NULL DEFAULT 0,
    supports_audio     INTEGER NOT NULL DEFAULT 0,
    supports_pdf       INTEGER NOT NULL DEFAULT 0,
    probed_at        INTEGER,           -- NULL 表示尚未探测
    UNIQUE(provider_id, model_id)
);

-- model_server_tools 表的 provider_id/model_id 对改为引用 model_config_id
-- 原：REFERENCES providers(id) + model_id TEXT
-- 新：
CREATE TABLE model_server_tools (
    id               INTEGER PRIMARY KEY,
    model_config_id  INTEGER NOT NULL REFERENCES model_configs(id) ON DELETE CASCADE,
    capability       TEXT    NOT NULL,
    format           TEXT    NOT NULL,
    UNIQUE(model_config_id, capability)
);

-- settings 表中 default_model 的值改为 model_config_id（整数字符串）
-- 原：default_provider_id + default_model_id 两个 key
-- 新：default_model_config_id 一个 key
```

---

## 新增：自动探测流程

### Wire Format 探测（Provider 级）

用户配置好 base URL 和 API key 后，按以下顺序尝试最小请求：

```
1. POST {base_url}/v1/chat/completions      → OpenAI-compat
2. POST {base_url}/v1/messages              → Anthropic
   (附带 x-api-key + anthropic-version header)
3. POST {base_url}/v1beta/models/{model}:generateContent → Gemini
```

请求体：`system: ""` + `user: "hi"` + `max_tokens: 1`，花费接近于零。

哪个格式返回合法响应即为支持的协议；多种都通时，Anthropic 系模型优先选 `AnthropicWire`，其余选 `OpenAiWire`。

### 模型列表扫描（Provider 级）

Wire format 确认后，调用对应端点拉取模型列表：

| Wire Format | 端点 |
|-------------|------|
| OpenAI-compat | `GET {base_url}/v1/models` |
| Anthropic | （无标准 list endpoint，返回内置模型列表） |
| Gemini | `GET {base_url}/v1beta/models` |

已知 provider（Anthropic / OpenAI / Gemini）直接展示内置模型列表，不需要网络请求。

### 能力探测（Model 级）

用户从列表中选择一个模型后，对该 `(provider, model_id)` 组合发起以下探测：

| 能力 | 探测方式 | 判断依据 |
|------|---------|---------|
| tool_use | 附带一个 `ping()` 空工具，user: "call ping" | 响应中是否有 tool_call |
| vision | 附带 1×1 透明 PNG（硬编码 base64），user: "describe" | 是否返回 4xx 或 unsupported error |
| streaming | 同一请求加 `stream: true` | 是否返回 SSE 事件流 |
| pdf | 附带最小 PDF base64，user: "describe" | 是否返回 4xx 或 unsupported error |

各项探测串行发出（避免频繁触发 rate limit），间隔约 200ms。总耗时预计 2–5 秒。

### 探测结果处理

```
探测完成
    ↓
展示结果供用户确认（可手动修改任意项）
┌──────────────────────────────────┐
│ 协议        Anthropic ✓          │
│ 工具调用    ✓                    │
│ 图片        ✓                    │
│ 流式        ✓                    │
│ PDF         ✗                    │
└──────────────────────────────────┘
    ↓ 用户确认
写入 model_configs 表，model 出现在[模型列表]中
```

探测失败时（网络错误、rate limit）提示用户，允许跳过探测并手动填写能力。

### 可靠性边界

- **接受但忽略 tool 定义的中转站**：响应为文本而非 tool call，探测结果为"不支持"（保守结果，可接受）
- **Wire format vs 模型能力混淆**：探测失败时无法区分是中转站不支持还是模型不支持，用户可手动覆盖
- **Rate limit**：串行探测 + 间隔，失败时回退到手动配置路径

---

## 设置页 UI 变更

### 之前（Provider 为主）

```
[Provider 配置]
  Anthropic          claude-sonnet-4-6   [编辑] [删除]
  Local Ollama       qwen2.5-coder:32b   [编辑] [删除]
  [+ 新增 Provider]

[默认模型]
  Anthropic / claude-sonnet-4-6   [修改]
```

### 之后（Model 为主）

```
[模型]
  Claude Sonnet 4.6     Anthropic · 工具 · 图片 · 搜索 · 代码执行   [重新探测] [删除]
  Qwen 2.5 Coder        中转站A · 工具                              [重新探测] [删除]
  [+ 从通道添加模型]

[连接通道]
  Anthropic             [扫描模型] [编辑] [删除]
  中转站A               [扫描模型] [编辑] [删除]
  [+ 添加通道]
```

**[从通道添加模型]** 流程：

```
1. 选择通道（或新建通道）
2. 扫描该通道的可用模型列表
3. 勾选要添加的模型
4. 对每个选中的模型执行能力探测
5. 确认探测结果后加入[模型]列表
```

**[重新探测]**：对已有模型重跑能力探测，更新 `capabilities` 和 `probed_at`。适用于中转站升级后刷新能力状态。

模型选择器下拉列表（聊天输入框）展示扁平的 `ModelConfig` 列表，不再按 Provider 分组：

```
✓ Claude Sonnet 4.6
  Qwen 2.5 Coder
  GPT-5.4
```

---

## 运行时变化

- `ProviderClient` 初始化时读取 `ModelConfig.wire_format` 选择 `WireAdapter`，不再从 `ProviderType` 推导
- `ModelCapabilities` 来源直接读 `ModelConfig`，解析优先级链简化为：
  ```
  1. 用户手动覆盖（通过设置页"重新探测"后修改）
  2. 探测结果（写入 model_configs 表）
  3. 保守默认值（未探测时）
  ```
  内置模型能力表保留，仅作为"已知 provider 的已知模型"在探测前预填默认值，探测后以探测结果为准

---

## 对现有代码的影响

| 模块 | 变化 |
|------|------|
| `storage/tasks.rs` | task 的 `selected_model` 改为引用 `model_config_id` |
| `provider/transport.rs` | `WireAdapter` 选择从 `ProviderType` 改为读 `ModelConfig.wire_format` |
| `provider/wire.rs` | 不变 |
| `storage.rs` / provider CRUD | 新增 `ModelConfig` CRUD，`provider_models` 表迁移为 `model_configs` |
| 前端 `SettingsPage` | 按新 UI 结构重组，Provider 和 Model 分开展示 |
| 前端模型选择器 | 数据源从按 Provider 分组改为扁平 ModelConfig 列表 |
