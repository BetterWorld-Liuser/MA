# 上下文管理

> 从 [DESIGN.md](DESIGN.md) 延伸：文件系统作为 Source of Truth，AI 上下文永远反映磁盘真实状态。

## 数据结构

```rust
/// AI 实际收到的上下文（每轮重新构建）
struct AgentContext {
    system: String,                           // 系统提示（稳定，适合 prefix cache）
    watched_files: HashMap<PathBuf, FileSnapshot>, // watcher 维护的文件真实状态
    messages: Vec<Message>,                   // 压缩后的对话历史（只保留近几轮）
}

/// 文件快照，由 watcher 实时更新
struct FileSnapshot {
    path: PathBuf,
    content: String,
    last_modified: SystemTime,
    last_modified_by: ModifiedBy,             // 区分是谁改的
}

enum ModifiedBy {
    Agent,
    User,
    External,   // 其他程序
}

/// 用户看到的完整聊天记录（独立存储，不参与 AI 上下文构建）
struct ConversationHistory {
    turns: Vec<DisplayTurn>,
}

struct DisplayTurn {
    role: Role,
    content: String,
    tool_calls: Vec<ToolSummary>,  // 例如："修改了 foo.py 第3-10行"
    timestamp: SystemTime,
}
```

---

## 构建策略

### 文件排列顺序（prefix cache 最优）

```
[system prompt]          ← 永远不变，深度缓存
[未被修改的文件们]        ← 稳定，大概率命中缓存
[被修改过的文件]          ← 变化频繁，放尽量靠后
[压缩后的对话历史]        ← 每轮都在变
[最新 user message]      ← 最底部
```

**原则**：变化越频繁的内容越靠后，保护前面稳定内容的缓存。

### Prefix Cache 机制说明

- Anthropic 的 cache 按**内容 hash** 存储，不是按请求顺序
- 只要前缀内容字节完全一致，不管之前接的是什么，都能命中
- 并发请求、后台刷新不会互相污染缓存
- 文件内容变了 → hash 不同 → 自动新的 cache entry

### AI 管理 watch 列表

- AI 自己决定要 watch 哪些文件（通过工具调用）
- 不自动注入整个工作目录所有文件

---

## 待决策

- [ ] 对话历史压缩策略：保留几轮？摘要还是直接截断？
- [ ] watched files 在上下文里的具体格式：system prompt 还是每轮 user message 前插入？
