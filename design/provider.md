# Provider 设计

> 从 [DESIGN.md](DESIGN.md) 延伸：Ma 自己管上下文构建，provider 层只负责把构建好的上下文发出去、把结果收回来。

## 选型：genai

使用 [`genai`](https://crates.io/crates/genai) 作为 provider 抽象层。

**选择理由：**
- 直接实现各家原生协议，不套壳其他 SDK
- 支持 Anthropic `cache_control`（按 message 级别，多种 TTL），Ma 的 prefix cache 优化依赖这个
- 14+ providers 开箱即用，OpenAI 兼容格式也覆盖
- 不管 agent 循环和上下文——这正是 Ma 自己要做的事

**排除 Rig 的原因：**
Rig 的核心是帮你管 agent 上下文，与 Ma 自管上下文的设计直接冲突。用 Rig 只能绕开其 Agent 抽象只用底层 CompletionModel，价值损耗太大。

---

## 与上下文管理的分工

```
AgentContext（Ma 自建）
    │
    │  每轮构建完毕后
    ▼
genai::ChatRequest（翻译层）
    │
    │  发出请求 / 收 stream
    ▼
Provider（Claude / GPT / Gemini / ...）
```

Ma 的 `AgentContext` 决定内容和顺序，翻译层负责把它映射到 `genai` 的类型，`genai` 负责处理各家 wire format 差异。

---

## Cache Control 映射

Ma 的 `CacheHint` 在翻译到 `genai::ChatRequest` 时，对相应 message 设置 `cache_control`：

```
[system prompt]           ← cache_control: Ephemeral1h
[未修改的文件们]           ← cache_control: Ephemeral1h
[被修改过的文件]           ← 无 cache（变化频繁，缓存无意义）
[对话历史]                ← 无 cache
[最新 user message]       ← 无 cache
```

---

## 待决策

- [ ] `genai` tool calling 当前完善程度是否满足 `run_command` 的需求？需要跑 spike 验证。
- [ ] tool schema / tool prompt 如何携带“本轮会话探测到的可用 shell 列表”？这部分应由 Ma 注入运行时信息，而不是写死在静态提示词中。
