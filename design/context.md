# 上下文管理

> 从 [DESIGN.md](DESIGN.md) 延伸：文件系统作为 Source of Truth，AI 上下文永远反映磁盘真实状态。

## 上下文分层结构

AI 每轮收到的上下文按稳定性从高到低排列，越靠上越不易变，越靠下越频繁重建：

```
[system_core]        ← 最稳定，几乎不变，深度缓存
[injections]         ← skills、MCP 等，session 启动时确定，session 内不再变
[tools]              ← 工具定义，API 独立参数，同样参与 prefix cache
[open_files]         ← AI 通过 open_file / close_file 控制，纯文件内容，无状态标注
[notes]              ← AI 通过 write_note / remove_note 管理
[system_status]      ← March 写入，AI 只读；当前被固定的文件列表等运行时状态
[hints]              ← 短命，每条独立 TTL，过期自动移除
[recent_chat]        ← 最近 3 轮人类↔AI 对话，每轮更新
```

**原则**：变化越频繁的内容越靠后，保护前面稳定内容的 prefix cache。`system_status` 置于 hints 之上、notes 之下，lock/unlock 操作只影响其下方层——而 hints 和 recent_chat 本身每轮都在变，不产生额外缓存损失。`open_files` 层不携带任何 lock 标注，保持内容稳定。

---

## 内部与外部的分离

March 把"执行环境"和"聊天环境"严格分开：

- **March 内部**：agent 循环、工具调用、执行结果。这些不进入聊天记录，也不跨轮保留（除非 AI 主动 move 进 Notes）。
- **March 外部**：AI 与人类的对话。AI 通过调用工具（如 `reply`）向用户发送消息，聊天窗口是独立的交互界面。

AI 上下文里的"3轮对话历史"只包含这个外部聊天的内容，执行历史不在其中。工具执行结果在当前轮内可见，轮次结束后自动丢弃，除非被 AI 写入 Notes。

---

## Notes：AI 的工作记忆

Notes 是 AI 在上下文中唯一可以跨轮主动管理的持久区域。AI 可以用它来：

- 记录当前任务目标（例如 id `"target"`）
- 保存有价值的命令执行结果（例如 id `"build_output"`）
- 列出分步计划、记录中间状态等

ID 由 AI 自己约定，March 不做语义区分，统一存储。系统提示词会充分说明这块区域的用途，引导 AI 合理使用。

```
write_note(id, content)   ← 新建或覆盖
remove_note(id)           ← 清除不再需要的条目
```

---

## Hints：外部工具注入接口

Hints 是为**外部自动化工具**暴露的上下文注入接口，定位于"来自外部世界的临时通知"。典型场景：Telegram bot 收到一条消息、Windows 系统弹出一条通知、CI 流水线报告构建结果——这些工具通过本地 API 把内容注入进来，AI 下一轮就能感知到。

与 Notes 的区别：

| | Notes | Hints |
|---|---|---|
| 写入方 | AI 主动写入 | 外部工具通过本地 API 注入 |
| 生命周期 | 手动 remove 前一直存在 | 按时间 TTL 自动过期 |
| 用途 | AI 的跨轮工作记忆 | 外部世界的临时事件通知 |

每条 Hint 同时带有**时间 TTL** 和**轮次 TTL**，两者取先到者：

- **时间过期**：注入时指定 `ttl_secs`，转换为绝对时间戳存入数据库。March 重启后直接用时间戳判断是否过期，无需额外逻辑。
- **轮次过期**：每轮上下文构建结束后将 `turns_remaining` 减一，归零即移除。适合"AI 看到 N 次后不再需要"的场景。

两者均为可选，至少指定一个。过期的 Hints 在每轮构建上下文前清理。

### 本地注入 API

March 启动后在本地监听一个 Unix socket（Windows 上为命名管道），外部工具通过它注入 Hints：

```
POST /hints
{
  "content": "Telegram: 用户 foo 问：部署好了吗？",
  "ttl_secs": 300,      // 可选，5 分钟后过期
  "ttl_turns": 3        // 可选，AI 看到 3 次后过期
}
```

接口只监听本地，不对外暴露。外部工具（Telegram bot、通知脚本等）与 March 部署在同一台机器上，直接调用即可。

---

## 数据结构

```rust
/// AI 实际收到的上下文（每轮重新构建）
struct AgentContext {
    system_core: String,                         // 核心行为指令，固定不变
    injections: Vec<Injection>,                  // skills、MCP 说明等，session 启动时加载
    tools: Vec<ToolDefinition>,                  // 工具定义，通过 API tools 参数独立传递
    open_files: IndexMap<PathBuf, FileSnapshot>, // 保序，影响 prefix cache；纯内容，无状态标注
    notes: IndexMap<String, NoteEntry>,          // 保序，id → content；AI 可读写
    system_status: SystemStatus,                 // March 维护，AI 只读
    hints: Vec<Hint>,                            // 短命注入，按 TTL 自动移除
    recent_chat: Vec<ChatTurn>,                  // 最近 3 轮人类↔AI 对话
}

struct SystemStatus {
    locked_files: Vec<PathBuf>,  // 被用户固定的文件，close_file 会被拒绝
}

/// 文件快照，由 watcher 实时更新
/// last_modified 仅供 watcher 内部判断是否重读，不渲染进上下文
enum FileSnapshot {
    Available { content: String, last_modified: SystemTime },
    Deleted,
    Moved { new_path: PathBuf },
}

struct Injection {
    id: String,       // 例如 "skills"、"mcp:filesystem"
    content: String,
}

struct NoteEntry {
    content: String,
}

struct Hint {
    content: String,
    expires_at: Option<SystemTime>,  // 绝对过期时间，由 now + ttl_secs 计算
    turns_remaining: Option<u32>,    // 剩余轮次，每轮递减，归零移除
    // 两者取先到者；重启后 expires_at 直接判断，turns_remaining 从数据库恢复
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

## 本地持久化

数据库文件位于工作目录下的 `.march/march.db`，每个工作目录独立一个。

### Schema

```sql
-- 任务列表
CREATE TABLE tasks (
    id          INTEGER PRIMARY KEY,
    name        TEXT    NOT NULL,
    created_at  INTEGER NOT NULL,  -- unix timestamp
    last_active INTEGER NOT NULL
);

-- 用户侧完整对话历史（不参与 AI 上下文构建）
CREATE TABLE conversation_turns (
    id             INTEGER PRIMARY KEY,
    task_id        INTEGER NOT NULL REFERENCES tasks(id),
    role           TEXT    NOT NULL,  -- 'user' | 'assistant'
    content        TEXT    NOT NULL,
    tool_summaries TEXT,              -- JSON，例如 ["修改了 src/auth.rs 第12-30行"]
    created_at     INTEGER NOT NULL
);

-- AI 工作记忆，顺序即上下文顺序
CREATE TABLE notes (
    task_id  INTEGER NOT NULL REFERENCES tasks(id),
    note_id  TEXT    NOT NULL,  -- AI 自己约定的 id，如 "target"、"plan"
    content  TEXT    NOT NULL,
    position INTEGER NOT NULL,  -- 显式维护顺序，影响 prefix cache
    PRIMARY KEY (task_id, note_id)
);

-- 监控文件列表，顺序即上下文顺序
CREATE TABLE open_files (
    task_id  INTEGER NOT NULL REFERENCES tasks(id),
    path     TEXT    NOT NULL,
    position INTEGER NOT NULL,  -- 显式维护顺序，影响 prefix cache
    locked   INTEGER NOT NULL DEFAULT 0,  -- boolean
    PRIMARY KEY (task_id, path)
);

-- 外部工具注入，跨 task 共享
CREATE TABLE hints (
    id              INTEGER PRIMARY KEY,
    content         TEXT    NOT NULL,
    expires_at      INTEGER,  -- unix timestamp，null 表示无时间限制
    turns_remaining INTEGER,  -- null 表示无轮次限制
    created_at      INTEGER NOT NULL
);
```

### 设计说明

**`open_files` 不存文件内容**：持久化的只是"哪些文件在被监控"和它们的顺序，内容永远由 watcher 从磁盘实时读取。

**`notes` 和 `open_files` 的 `position` 列**：SQLite 的 `rowid` 插入顺序不适合频繁重排的场景，用显式 `position` 整数维护顺序，重排时只更新受影响行的 `position` 值。

**`hints` 跨 task 共享**：外部通知与当前活跃任务无关，所有任务都能看到。March 启动时按 `expires_at` 清理已过期条目，运行中按轮次递减 `turns_remaining`。

**`tool_summaries` 用 JSON**：工具调用摘要是展示用的，不做查询，JSON 够用且省表。

---

## 文件管理：open / close

- AI 通过 `open_file` 将文件纳入上下文，watcher 开始实时追踪该文件
- AI 通过 `close_file` 释放文件，从上下文中移除
- 没有 `read_file`——打开即追踪，上下文里的内容永远是磁盘真实状态
- March 也可以根据 Notes 大小、文件访问频率等自动触发 close，AI 的 `close_file` 只是一个额外的主动信号

### Prefix Cache 与文件顺序

文件列表在 System prompt 之下、Notes 之上，属于相对稳定的层。close_file 会使其下方所有层的缓存失效，但下方的 Notes 和对话历史本身每轮都在变，缓存代价可以接受。

批量 close（积累多个后一次性移除并重排）可以进一步减少缓存重建频率。

### Prefix Cache 机制说明

- Anthropic 的 cache 按**内容 hash** 存储，不是按请求顺序
- 只要前缀内容字节完全一致，即可命中缓存
- 文件内容变了 → hash 不同 → 自动新的 cache entry

### Watcher 覆盖范围

`open_files` 里的文件不一定位于工作目录内（AI 可以打开任意路径的文件）。Watcher 维护两个监控集：

- **工作目录递归 watch**：捕获目录级批量操作
- **外部文件逐条 watch**：对工作目录外的 open 文件单独注册，随 `open_file` / `close_file` 动态增减

---

## System Status：运行时状态区域

`system_status` 是上下文中 March 维护、AI 只读的区域，位于 notes 之下、hints 之上。

当前存储的内容：**被用户固定（locked）的文件列表**。

```
[system_status]
固定文件（close_file 对以下文件无效）：
  - src/auth.rs
  - config/prod.toml
```

**为什么不放在 open_files 区域**：lock 状态变化会改变该层 hash，导致其下所有层的 prefix cache 失效。`system_status` 置于 hints 之上，而 hints 和 recent_chat 本身每轮都在变，lock/unlock 不产生额外缓存损失。

**lock 的语义是监控绑定，不是文件保护**：

| 操作 | 结果 |
|------|------|
| `close_file` locked 文件 | 被 March 拒绝 |
| AI 修改 locked 文件内容 | 允许 |
| AI 通过命令删除 locked 文件 | 允许执行；watcher 收到 `Remove` 事件后向用户发出警告 |

---

## Watcher 边界情况

文件被外部删除、移动或重命名时，March 的处理原则：**AI 应感知变化，不静默丢失信息**。

### 通知机制

所有非预期文件变化（删除、移出目录、目录级操作波及）触发两件事：

1. `FileSnapshot` 更新为 `Deleted` 或 `Moved { new_path }`，AI 上下文文件区域直接呈现状态
2. 注入一条 Hint，`ttl_turns: 2`，明确描述发生了什么

两者互补：文件区域是持续的状态标记，Hint 是"刚发生这件事"的一次性提醒，2 轮后自动清除。

### 各场景处理

| 场景 | FileSnapshot | Hint |
|------|-------------|------|
| 文件被删除 | `Deleted` | "src/foo.rs 已被删除" |
| 文件被重命名 | `Moved { new_path }`，路径同步更新 | "src/foo.rs → src/bar.rs，路径已更新" |
| 文件移到工作目录外 | `Deleted`（等同消失） | "src/foo.rs 已移出工作目录" |
| 目录被删除/重命名 | 批量扫描 open_files，逐条更新 | 合并为一条 Hint |

**目录级操作**不依赖子文件事件逐条到来——收到目录级 `Remove`/`Rename` 时主动扫描 `open_files` 批量处理，避免依赖平台行为差异。

**快速连续事件**（如 delete → recreate、rename → rename）：watcher 层对同路径事件做 300ms debounce，最终状态通过直接读磁盘确认，而非靠事件链推断。

---

## 任务切换与并发

每个任务有自己独立的 `open_files` 订阅集合。切换任务时，watcher 自然切换到新任务的文件集，不再推送旧任务的变更事件给上下文构建器。

**AI 仍在运行时切换任务**：旧任务的 agent 在自己的上下文快照里继续执行，与新任务完全隔离——它读到的文件内容、notes、recent_chat 都属于旧任务，不受切换影响。两个任务可以同时运行，互不干扰。

---

## 上下文压力管理

March 在每轮构建上下文后估算 token 用量。当用量超过阈值时，向 `system_status` 追加压力提示，由 AI 自行决定如何释放空间：

```
[system_status]
⚠ 上下文用量 87%，接近上限。请主动关闭不再需要的文件（close_file）或清理 Notes 中已无用的条目（remove_note）。
```

AI 收到这条信息后，可以在当前轮或下一轮工具调用中主动整理。March 不强制关闭任何内容，决策权留给 AI。压力提示随用量实时更新，用量下降后自动消失。
