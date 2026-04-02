# Provider 设计

> 从 [DESIGN.md](DESIGN.md) 延伸：March 自己管上下文构建，provider 层只负责把构建好的上下文发出去、把结果收回来。

---

## 选型：genai

使用 [`genai`](https://crates.io/crates/genai) 作为 provider 抽象层。

**选择理由：**
- 直接实现各家原生协议，不套壳其他 SDK
- 支持 Anthropic `cache_control`（按 message 级别，多种 TTL），March 的 prefix cache 优化依赖这个
- 14+ providers 开箱即用，OpenAI 兼容格式也覆盖
- 不管 agent 循环和上下文——这正是 March 自己要做的事

**排除 Rig 的原因：**
Rig 的核心是帮你管 agent 上下文，与 March 自管上下文的设计直接冲突。用 Rig 只能绕开其 Agent 抽象只用底层 CompletionModel，价值损耗太大。

---

## 与上下文管理的分工

```
AgentContext（March 自建）
    │
    │  每轮构建完毕后
    ▼
genai::ChatRequest（翻译层）
    │
    │  发出请求 / 收 stream
    ▼
Provider（Claude / GPT / Gemini / ...）
```

March 的 `AgentContext` 决定内容和顺序，翻译层负责把它映射到 `genai` 的类型，`genai` 负责处理各家 wire format 差异。

---

## Cache Control 映射

March 的 `CacheHint` 在翻译到 `genai::ChatRequest` 时，对相应 message 设置 `cache_control`：

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
    Anthropic,
    OpenAI,
    Gemini,
    OpenAICompat,   // 自定义端点：本地模型、第三方代理等
}

struct ProviderModel {
    provider_id:  i64,
    model_id:     String,  // API 实际使用的 ID，如 "claude-sonnet-4-6"
    display_name: String,  // 界面展示名；genai 已知 provider 由 March 内置，compat 由用户填写
}

struct DefaultModel {
    provider_id: i64,
    model_id:    String,
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
    provider_type TEXT    NOT NULL,   -- 'anthropic' | 'openai' | 'gemini' | 'openai_compat'
    api_key       TEXT    NOT NULL,
    base_url      TEXT,               -- openai_compat 必填，其余可选（覆盖默认端点）
    created_at    INTEGER NOT NULL
);

-- genai 已知 provider 的模型列表由 March 内置，不存表
-- openai_compat 的可用模型由用户手动维护
CREATE TABLE provider_models (
    id           INTEGER PRIMARY KEY,
    provider_id  INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    model_id     TEXT    NOT NULL,
    display_name TEXT    NOT NULL
);

-- 全局键值设置
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
-- 用到的 key：
--   default_provider_id  → provider.id
--   default_model_id     → model_id string
```

---

## 模型列表的来源

| Provider 类型 | 模型列表来源 |
|--------------|------------|
| Anthropic / OpenAI / Gemini | March 内置常用模型列表，不需要用户填写 |
| OpenAICompat | 用户在设置页手动添加 model_id |

内置列表随 March 版本更新维护，不做自动 API 拉取（避免冷启动依赖网络）。

---

## 运行时模型解析

session 启动时，按以下优先级确定使用的模型：

```
1. 用户在输入框模型选择器里的选择（session 级）
2. 全局默认（settings.default_provider_id + default_model_id）
3. 硬编码 fallback：提示用户去设置页配置
```

模型选择器的下拉列表 = 所有已配置 provider 下的所有可用模型，按 provider 分组展示：

```
Anthropic
  ✓ claude-sonnet-4-6
    claude-opus-4-6
Local Ollama
    qwen2.5-coder:32b
```

---

## genai 客户端初始化

每个 provider 在 session 启动时初始化对应的 genai 客户端：

```rust
fn build_genai_client(provider: &ProviderConfig) -> genai::Client {
    let mut builder = genai::Client::builder();

    // 注入 API key
    builder = builder.with_auth_resolver(
        StaticAuthResolver::new(provider.api_key.clone())
    );

    // openai_compat 覆盖 base_url
    if provider.provider_type == ProviderType::OpenAICompat {
        builder = builder.with_service_url(provider.base_url.as_deref().unwrap());
    }

    builder.build()
}
```

---

## 设置页 UI

从左栏标题点击进入，覆盖整个窗口。

### Provider 列表

```
[Provider 配置]

  Anthropic          claude-sonnet-4-6   [编辑] [删除]
  Local Ollama       qwen2.5-coder:32b   [编辑] [删除]

  [+ 新增 Provider]

[默认模型]
  Anthropic / claude-sonnet-4-6   [修改]
```

### 新增 / 编辑 Provider

```
类型    [Anthropic ∨]
名称    [Anthropic        ]
API Key [sk-ant-...       ]  [显示/隐藏]
Base URL[                 ]  （可选，留空使用默认端点）

                  [测试连通性]  [取消]  [保存]
```

- 保存前必须通过连通性测试，或用户显式跳过
- 连通性测试：发一个最小 API 请求（如单 token completion），验证 key 有效
- 删除 provider 前：若有 session 正在使用该 provider，弹出确认提示

### OpenAICompat 额外字段

类型选 `OpenAI 兼容` 时，额外展示：

```
Base URL [http://localhost:11434/v1]  （必填）

可用模型
  qwen2.5-coder:32b    [删除]
  [+ 添加模型 ID]
```

---

## 待决策

- [ ] `genai` tool calling 当前完善程度是否满足 `run_command` 的需求？需要跑 spike 验证。
- [ ] tool schema / tool prompt 如何携带"本轮会话探测到的可用 shell 列表"？这部分应由 March 注入运行时信息，而不是写死在静态提示词中。
