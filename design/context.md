# 上下文管理

> 从 [DESIGN.md](DESIGN.md) 延伸：文件系统作为 Source of Truth，AI 上下文永远反映磁盘真实状态。

## 上下文分层结构

AI 每轮收到的上下文按稳定性从高到低排列，越靠上越不易变，越靠下越频繁重建：

```
[system_core]        ← 最稳定，几乎不变，深度缓存
[injections]         ← skills、MCP 等，session 启动时确定，session 内不再变
[tools]              ← 工具定义，API 独立参数，同样参与 prefix cache
[open_files]         ← AI 通过 open_file / close_file 控制；文件内容带行号，若有 LSP 诊断则附在文件内容后
[notes]              ← AI 通过 write_note / remove_note 管理
[session_status]     ← March 写入，AI 只读；session 级环境信息，低频变化
[runtime_status]     ← March 写入，AI 只读；锁定文件、当前时间、上下文压力等高频状态
[hints]              ← 短命，每条独立 TTL，过期自动移除
[recent_chat]        ← 最近 10 轮人类↔AI 对话（含每条消息时间），每轮更新
```

**原则**：变化越频繁的内容越靠后，保护前面稳定内容的 prefix cache。`session_status` 用来承载"对当前会话有帮助、但不属于文件内容和技能注入"的环境信息；`runtime_status` 只放每轮可能变化、且需要 AI 立即感知的状态。两层都位于 notes 之下，因此不会污染更稳定的 prefix；其中 `runtime_status` 继续置于 hints 之上，lock/unlock、时间刷新、上下文压力变化只影响其下方层。`open_files` 层的文件内容后若存在 LSP diagnostics（errors/warnings），则附在文件内容之后一起渲染；无诊断时不追加任何内容，保持 cache 行为与纯文件快照一致。

---

## 内部与外部的分离

March 把"执行环境"和"聊天环境"严格分开：

- **March 内部**：agent 循环、工具调用、执行结果。这些不进入聊天记录，也不跨轮保留（除非 AI 主动 move 进 Notes）。
- **March 外部**：AI 与人类的对话。用户最终看到的是 assistant 的自然语言输出，聊天窗口是独立的交互界面。

AI 上下文里的“最近 10 轮对话历史”只包含这个外部聊天的内容，并携带每条消息时间；执行历史不在其中。工具执行结果在当前轮内可见，轮次结束后自动丢弃，除非被 AI 写入 Notes。

---

## 轮次定义

March 里的**一轮（turn）**，指的是：

**从一条用户输入被系统接收开始，到 agent loop 自然收敛并结束**的完整工作周期。

这里的"结束"不是指 AI 是否中途调用过某个回复工具，而是指：**当前这轮 provider 返回的结果已经不再包含新的 tool calls，March 因此判定本轮工作完成**。

这一定义与底层发起了多少次模型请求无关。一次 turn 内，AI 可能会反复经历：

- 读取 / 打开文件
- 修改文件
- 运行命令
- 读取命令结果
- 根据中间结果再次请求模型
- 必要时调用浏览器、外部工具或检索能力

只要这些动作都服务于**同一次任务推进**，并且 agent loop 还没有自然结束，它们就都属于同一轮。

### 为什么要这样定义

如果把"一轮"理解成"一次 provider 请求"，会直接破坏 agent loop 的连贯性。AI 在完成一个任务时，经常需要先看文件、再跑命令、再根据结果继续推理；这些中间结果必须在同一轮内持续可见，直到 agent loop 真正结束。

因此，March 的边界是：

- **轮内保留完整执行上下文**：同一轮中的 tool_calls、tool_results、中间 assistant 消息，都会持续参与后续请求
- **轮间只保留必要摘要**：当这一轮真正结束后，轮内执行历史整体丢弃；跨轮只保留 `recent_chat`、`notes`、`open_files` 等结构化状态

### 对上下文构建的含义

`AgentContext` 是**一轮开始时**基于最新状态构建出来的外层上下文骨架，而不是每次底层 provider 请求都从零开始只带这一份内容。

进入一轮之后，March 实际发给模型的消息由两部分组成：

1. 这轮开始时构建出的稳定上下文骨架  
2. 本轮执行过程中不断追加的轮内历史（assistant 消息、tool calls、tool results 等）

所以，像"AI 刚修改了文件""上一步命令输出了什么""刚刚检索到了什么资料"这类信息，在**本轮结束前**都必须继续可见；只有当这一轮不再产生新的 tool calls、agent loop 自然收敛时，这些轮内历史才整体清空。

### 与用户可见回复、recent_chat 的关系

`recent_chat` 记录的是**轮与轮之间**的外层对话，不记录轮内中间步骤。

也就是说：

- 用户发来一条消息，触发一轮
- AI 在这一轮里可能进行多次内部请求和工具调用
- AI 在轮内可以产生阶段性用户可见输出，但这**不自动等于本轮结束**
- 只有当 provider 不再返回新的 tool calls 时，这一轮才算完成
- 完成后，`recent_chat` 追加一条外层对话记录，供下一轮使用

---

## Notes：AI 的工作记忆

Notes 是 AI 在上下文中唯一可以跨轮主动管理的持久区域。AI 可以用它来：

- 记录当前任务目标（例如 id `"target"`）
- 保存有价值的命令执行结果（例如 id `"build_output"`）
- 列出分步计划、记录中间状态等

ID 由 AI 自己约定，March 不做语义区分，统一存储。系统提示词会充分说明这块区域的用途，引导 AI 合理使用。这里最重要的约束是：`write_note` 对相同 `id` 的行为是**覆盖更新**，不是创建第二条记录。也就是说，note id 是一个稳定槽位，适合承载“当前版本”的事实。

因此：

- 发现已有 note 的 id 正好对应同一类信息时，应优先覆盖该 id
- 只有当信息确实需要和现有 note 并列长期保留时，才创建新的 id
- 对于“用户身份”“当前目标”“当前计划”“最近一次有效错误摘要”这类单槽信息，不应不断生成近似 id

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
    session_status: SessionStatus,               // March 维护，AI 只读；session 级环境信息
    runtime_status: RuntimeStatus,               // March 维护，AI 只读；每轮可能变化的运行时状态
    hints: Vec<Hint>,                            // 短命注入，按 TTL 自动移除
    recent_chat: Vec<ChatTurn>,                  // 最近 10 轮人类↔AI 对话（含消息时间）
}

struct SessionStatus {
    workspace_root: PathBuf,          // 当前工作目录
    platform: String,                 // 例如 "Windows" / "macOS" / "Linux"
    shell: String,                    // 当前默认 shell，例如 "powershell"
    available_shells: Vec<String>,    // 当前环境真实可用的 shell
    workspace_entries: Vec<String>,   // 工作目录下一层目录结构摘要，仅名字 + / 标记
}

struct RuntimeStatus {
    locked_files: Vec<PathBuf>,  // 被用户固定的文件，close_file 会被拒绝
    now: SystemTime,             // 当前本地时间，渲染时带时区
    context_pressure: Option<ContextPressure>,
}

struct ContextPressure {
    used_percent: u8,            // 例如 87
    message: String,             // 给 AI 的简短提示
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

### 自动 open 的触发场景

除了 AI 主动调用 `open_file`，以下三种情况 Ma 会自动将文件加入 `open_files`：

**AI 写入文件**：AI 通过 `write_file` 落盘的文件，若不在 `open_files` 中，Ma 自动加入并开始追踪。逻辑是：AI 写完之后往往还需要确认效果、继续修改，文件理应留在上下文里反映最新状态。

**用户 @ 引用文件**：用户在消息中以 `@path` 形式引用的文件，Ma 自动加入 `open_files`。这是明确的"我希望 AI 看到这个文件"的意图表达，无需 AI 再手动调用 `open_file`。

**session 初始化自动加入 `AGENTS.md`**：如果工作目录根下存在 `AGENTS.md`，Ma 在 session 初始化时自动将其加入 `open_files`，并默认设为 locked。它的定位是项目级规则文件，应像其他被追踪文件一样由 watcher 提供真实内容，但不依赖 AI 主动打开。

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

## Session Status：稳定环境信息层

`session_status` 是上下文中 March 维护、AI 只读的区域，位于 notes 之下、`runtime_status` 之上。

它承载的是**对当前 session 有帮助，但变化频率低于 runtime 状态**的信息，典型包括：

- 当前工作目录
- 当前系统 / 默认 shell / 可用 shell 列表
- 工作目录一层目录结构摘要

示例：

```
[session_status]
工作目录：D:\playground\MA
系统：Windows
默认 shell：powershell
可用 shell：powershell, cmd

工作目录一层结构：
  - design/
  - src/
  - Cargo.toml
  - README.md
```

**为什么不放进 `[injections]`**：这些内容虽然对 session 很重要，但本质上属于当前运行环境，不是 skill / MCP 这类"会话启动后固定的能力说明"。`[injections]` 应保持语义纯净。

**为什么不放进工具提示词或 recent_chat**：这些信息是 March 对上下文的系统性建模，而不是某个工具的私有补充，也不是用户和 AI 的对话内容。放入独立层更可复用，也更符合"用户视图 vs AI 上下文"分离原则。

**工作目录结构只放一层摘要**：目标是帮助 AI 在会话起点快速建立方位感，而不是复制文件树。更深层结构应由 AI 自己通过工具探索。

---

## Runtime Status：高频运行时状态层

`runtime_status` 是上下文中 March 维护、AI 只读的区域，位于 `session_status` 之下、hints 之上。

它承载的是**每轮都可能变化，且 AI 需要即时感知**的状态，当前包括：

- 被用户固定（locked）的文件列表
- 当前本地时间
- 上下文压力提示

示例：

```
[runtime_status]
当前时间：2026-04-03 14:20 Asia/Shanghai

固定文件（close_file 对以下文件无效）：
  - src/auth.rs
  - config/prod.toml
```

**为什么拆出这一层**：如果把时间、lock 状态、context pressure 都和 cwd/目录摘要混在一起，整个状态块会频繁变 hash，等于把原本可以稳定缓存的环境信息一起拖成高频变化层。拆分后，session 级环境仍保持稳定，只有真正高频的状态落在更靠后的位置。

**lock 的语义是监控绑定，不是文件保护**：

| 操作 | 结果 |
|------|------|
| `close_file` locked 文件 | 被 March 拒绝 |
| AI 修改 locked 文件内容 | 允许 |
| AI 通过命令删除 locked 文件 | 允许执行；watcher 收到 `Remove` 事件后向用户发出警告 |

`AGENTS.md` 默认属于 locked 文件：它的 lock 只用于保证规则文件持续留在上下文中，不意味着禁止编辑。

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

March 在每轮构建上下文后估算 token 用量。当用量超过阈值时，向 `runtime_status` 追加压力提示，由 AI 自行决定如何释放空间：

```
[runtime_status]
⚠ 上下文用量 87%，接近上限。请主动关闭不再需要的文件（close_file）或清理 Notes 中已无用的条目（remove_note）。
```

AI 收到这条信息后，可以在当前轮或下一轮工具调用中主动整理。March 不强制关闭任何内容，决策权留给 AI。压力提示随用量实时更新，用量下降后自动消失。
