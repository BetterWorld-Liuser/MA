# 上下文管理

> 从 [DESIGN.md](DESIGN.md) 延伸：文件系统作为 Source of Truth，AI 上下文永远反映磁盘真实状态。

## 上下文分层结构

AI 每轮收到的上下文按稳定性从高到低排列，越靠上越不易变，越靠下越频繁重建：

```
[system_core]        ← 固定不变，深度缓存
[injections]         ← skills、MCP 等，session 内稳定
[tools]              ← 工具定义，API 独立参数
[open_files]         ← AI 通过 open_file / close_file 控制
[notes]              ← AI 通过 write_note / remove_note 管理
[recent_chat]        ← 只含人类↔AI 的聊天内容，每轮更新
```

**原则**：变化越频繁的内容越靠后，保护前面稳定内容的 prefix cache。

```
[system_core]        ← 最稳定，几乎不变
[injections]         ← session 启动时确定，session 内不再变
[tools]              ← API 独立参数，同样参与 prefix cache
[open_files]         ← AI 通过 open_file / close_file 控制
[notes]              ← AI 通过 write_note / remove_note 管理
[recent_chat]        ← 最近 3 轮对话，每轮更新
```

---

## 内部与外部的分离

Ma 把"执行环境"和"聊天环境"严格分开：

- **Ma 内部**：agent 循环、工具调用、执行结果。这些不进入聊天记录，也不跨轮保留（除非 AI 主动 move 进 Notes）。
- **Ma 外部**：AI 与人类的对话。AI 通过调用工具（如 `reply`）向用户发送消息，聊天窗口是独立的交互界面。

AI 上下文里的"3轮对话历史"只包含这个外部聊天的内容，执行历史不在其中。工具执行结果在当前轮内可见，轮次结束后自动丢弃，除非被 AI 写入 Notes。

---

## Notes：AI 的工作记忆

Notes 是 AI 在上下文中唯一可以跨轮主动管理的持久区域。AI 可以用它来：

- 记录当前任务目标（例如 id `"target"`）
- 保存有价值的命令执行结果（例如 id `"build_output"`）
- 列出分步计划、记录中间状态等

ID 由 AI 自己约定，Ma 不做语义区分，统一存储。系统提示词会充分说明这块区域的用途，引导 AI 合理使用。

```
write_note(id, content)   ← 新建或覆盖
remove_note(id)           ← 清除不再需要的条目
```

---

## 数据结构

```rust
/// AI 实际收到的上下文（每轮重新构建）
struct AgentContext {
    system_core: String,                         // 核心行为指令，固定不变
    injections: Vec<Injection>,                  // skills、MCP 说明等，session 启动时加载
    tools: Vec<ToolDefinition>,                  // 工具定义，通过 API tools 参数独立传递
    open_files: IndexMap<PathBuf, FileSnapshot>, // 保序，影响 prefix cache
    notes: IndexMap<String, String>,             // 保序，id → content
    recent_chat: Vec<ChatTurn>,                  // 最近 3 轮人类↔AI 对话
}

struct Injection {
    id: String,       // 例如 "mcp:filesystem"、"skill:git"
    content: String,
}

/// 文件快照，由 watcher 实时更新
struct FileSnapshot {
    path: PathBuf,
    content: String,
    last_modified: SystemTime,
    last_modified_by: ModifiedBy,
}

enum ModifiedBy {
    Agent,
    User,
    External,
}

/// 用户看到的完整聊天记录（独立存储，不参与 AI 上下文构建）
struct ConversationHistory {
    turns: Vec<DisplayTurn>,
}

struct DisplayTurn {
    role: Role,
    content: String,
    tool_calls: Vec<ToolSummary>,   // 例如："修改了 foo.py 第3-10行"
    timestamp: SystemTime,
}
```

---

## 文件管理：open / close

- AI 通过 `open_file` 将文件纳入上下文，watcher 开始实时追踪该文件
- AI 通过 `close_file` 释放文件，从上下文中移除
- 没有 `read_file`——打开即追踪，上下文里的内容永远是磁盘真实状态
- Ma 也可以根据 Notes 大小、文件访问频率等自动触发 close，AI 的 `close_file` 只是一个额外的主动信号

### Prefix Cache 与文件顺序

文件列表在 System prompt 之下、Notes 之上，属于相对稳定的层。close_file 会使其下方所有层的缓存失效，但下方的 Notes 和对话历史本身每轮都在变，缓存代价可以接受。

批量 close（积累多个后一次性移除并重排）可以进一步减少缓存重建频率。

### Prefix Cache 机制说明

- Anthropic 的 cache 按**内容 hash** 存储，不是按请求顺序
- 只要前缀内容字节完全一致，即可命中缓存
- 文件内容变了 → hash 不同 → 自动新的 cache entry
