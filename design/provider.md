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

## 上下文窗口元数据

右侧的 context usage 预算应优先来自**当前模型的真实上下文窗口**，而不是固定常量。

运行时按以下优先级确定 `context_window`：

```
1. provider `/models` 返回的模型元数据（如 `context_window` / `max_input_tokens`）
2. March 内置的模型能力表（已知模型）
3. 环境变量手动覆盖（便于兼容非标准 provider）
4. 保守默认值
```

注意：不同 OpenAI-compatible provider 对 `/models` 的扩展字段并不统一，因此需要做 best-effort 解析，拿不到时明确走 fallback，而不是假装是 provider 官方值。

---

## 运行时模型解析

任务创建时，按以下优先级把运行入口写入该 task：

```
1. 当前默认运行配置（settings.default_provider_id + default_model_id）
2. 环境变量 fallback（开发态 / 无设置页配置时）
3. 硬编码 fallback：提示用户去设置页配置
```

进入聊天运行时，优先级改为：

```
1. task 持久化的 provider/model
2. 仅对历史旧 task 做兼容时，回退到当前默认运行配置
3. 环境变量 fallback
```

这意味着：

- 输入框模型选择器是 **task 级** 状态，不是 session 级临时状态
- 用户在设置页修改“默认运行配置”时，只影响**之后新建的任务**
- 已存在任务的 provider/model 应保持稳定，不应被新的全局默认值回刷

模型选择器的下拉列表 = 所有已配置 provider 的可用模型列表，按 provider 分组展示；当前 task 仍然持久化自己选中的 provider/model：

```
Anthropic
  ✓ claude-sonnet-4-6
    claude-opus-4-6

OpenAI
    gpt-5.4
    gpt-5.4-mini
```

切换模型时，如果用户点的是另一个 provider 分组下的模型，March 应同时更新该 task 的 `selected_provider_id` 与 `selected_model`。

## 流式稳健性策略

provider 层默认仍然是“流式优先”，因为 UI 需要即时增量输出；但 March 不应把“某家 provider 的 stream 兼容性不好”直接升级成整个回合失败。

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

这样做的目的不是追求“所有 provider 都完美支持 stream”，而是保证 March 在 provider 能力参差不齐时，仍能稳定完成一轮 agent loop。

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
Probe  [claude-sonnet-4-5 ∨]  （优先展示供应商模型列表，也支持手填）

                  [测试连通性]  [取消]  [保存]
```

- 保存前必须通过连通性测试，或用户显式跳过
- 连通性测试：对用户指定的 probe model 发一个最小 API 请求，只有拿到完整响应才算成功
- probe model 只用于测试，不等于全局默认模型，也不替代聊天页里的模型选择
- 若 provider 的 `/models` 有返回数据，Probe 字段应展示一个可搜索列表；用户仍可手动输入未出现在列表里的 model id
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
