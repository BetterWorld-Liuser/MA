# Subconscious

> 从 [DESIGN.md](DESIGN.md) 延伸：后台运行的辅助进程，在主 agent 工作间隙自动执行记忆整理、模式识别等任务。与 Agent（前台人格）互补，Subconscious 是 March 的后台认知层。

---

## 核心概念

### 与 Agent 的区别

Agent 是前台人格——接管对话，直接和用户交流，用户能看到它的每一句话。

Subconscious 是后台进程——旁观主 agent 的工作过程，在幕后产生副作用（写记忆、注入 hint），用户感知不到它在运行，只能感知它产生的效果。

```
Agent        → 前台，接管对话，用户可见
Subconscious → 后台，旁观对话，用户不可见（副作用可见）
```

### 为什么需要 Subconscious

主 agent 在忙着完成任务时，很少有精力做"元认知"工作——整理记忆、识别反复出现的模式、提醒自己容易遗忘的事项。这些工作需要一个独立的后台进程来承担。

类比人的意识与潜意识：意识负责当下的注意力和行动，潜意识负责后台整理、关联、巩固。

---

## 上下文结构

Subconscious 复用主 agent 的完整上下文前缀，追加主 agent 的轮内历史和自己的任务说明：

```
┌─────────────────────────────────────┐
│ [system_core]                       │
│ [injections]                        │
│ [tools]                             │  ← 与主 agent 完全相同的前缀
│ [open_files]                        │     命中 prefix cache
│ [notes]                             │
│ [memory_index]                      │
│ [session_status]                    │
│ [runtime_status]                    │
│ [hints]                             │
│ [recent_chat]                       │
├─────────────────────────────────────┤
│ [turn_history]                      │  ← 主 agent 刚完成的完整轮内历史
│                                     │     tool_calls、tool_results、
│                                     │     中间 assistant 消息，全部原样
├─────────────────────────────────────┤
│ [subconscious_prompt]               │  ← 任务说明 + 工具限制声明
└─────────────────────────────────────┘
```

### Prefix Cache 最大化

前缀与主 agent **字节级一致**——包括 `[tools]` 层。Subconscious 的工具限制不通过过滤工具定义实现，而是：

1. `[subconscious_prompt]` 中声明哪些工具不可用
2. 运行时如果 Subconscious 仍然调用了被禁工具，执行层直接返回错误

这样 `[tools]` 层不动，整个前缀（从 `system_core` 到 `recent_chat`）的 cache 完整命中。增量成本只有 `turn_history` + `subconscious_prompt` 的输入 tokens 和输出 tokens。

### Turn History 的价值

`recent_chat` 只记录轮间外层对话，看不到主 agent 具体做了什么工具操作。而 `turn_history` 包含主 agent 的完整工作过程：

- 读了哪些文件
- 跑了什么命令、输出是什么
- 中间推理了什么
- 最终改了哪些代码

对记忆整理来说，"刚才具体做了什么"比"用户和 AI 聊了什么"更有价值。

Subconscious 在主 agent 轮结束后、轮内历史被丢弃前运行，天然处于可以读取完整轮内历史的时间窗口。

---

## 权限模型

Subconscious 的权限是**架构级写死**的，不需要用户配置：

| 能力 | 权限 | 说明 |
|------|------|------|
| 读取主 agent 上下文 | **允许** | 完整前缀 + 轮内历史，只读 |
| 操作 Memory | **允许** | memorize、update_memory、forget_memory、recall_memory |
| 注入 Hints | **允许** | 设置 TTL，主 agent 下轮感知 |
| 修改主 agent 的 Notes | **禁止** | Notes 是 agent 的私有工作记忆 |
| Open/Close 主 agent 的文件 | **禁止** | 会打断主 agent 对上下文的预期 |
| 执行命令、写入文件 | **禁止** | 后台进程不应产生文件系统副作用 |
| 向用户发送消息 | **禁止** | Subconscious 对用户不可见 |

**为什么写死**：Subconscious 的安全通道只有 Memory 和 Hints。这两者的设计初衷就是"外部写入、主 agent 被动感知"：

- **Memory**：跨 agent 共享的长期知识，主 agent 通过 `memory_index` 自然看到变化
- **Hints**：外部注入的临时通知，自带 TTL 自动过期

其他操作（写 notes、open/close 文件、执行命令）都会直接侵入主 agent 的工作状态，风险远大于收益。把这个边界写死，用户写 subconscious 时不需要思考权限问题，March 也不需要做复杂的权限校验逻辑。

---

## AI + 程序混合执行

Subconscious 不一定每次都调用 LLM。执行流分两个阶段：

```
触发
  │
  ├─ 程序阶段（pre_check，确定性逻辑，零 LLM 成本）
  │   ├─ 检查触发条件是否满足
  │   ├─ 执行程序化操作（更新 skip_count、计算 staleness 等）
  │   └─ 判断：是否需要进入 AI 阶段？
  │
  ├─ 条件不满足 → 跳过，零成本
  │
  └─ 条件满足 → AI 阶段（LLM 调用）
      ├─ 构建上下文（前缀 cache + turn_history + subconscious_prompt）
      ├─ AI 观察、分析、决策
      └─ AI 通过 memory/hints 工具执行操作
```

大部分时候只跑程序阶段。只有程序检测到"有活要干"时才启动 AI，控制 token 消耗。

### pre_check 条件

March 内置一组常用条件，用户在定义文件中组合使用。全部条件满足才进入 AI 阶段：

| 条件 | 含义 |
|------|------|
| `turn_has_tool_calls: true` | 主 agent 本轮确实做了工作（不是纯聊天） |
| `memory_topic_count_exceeds: N` | 某 topic 下记忆超过 N 条 |
| `memory_stale_count_exceeds: N` | 有超过 N 条高 skip_count 记忆 |
| `memory_total_exceeds: N` | 记忆总量超过 N 条 |

pre_check 是可选的。省略时，每次触发都进入 AI 阶段。

---

## 定义格式

Subconscious 定义文件存放在 `.march/subconscious/` 目录下（项目级）或 `~/.march/subconscious/` 目录下（用户级），项目级优先。

```markdown
---
name: memory-curator
order: 100
trigger: turn_end
frequency: 3
pre_check:
  - turn_has_tool_calls: true
  - memory_topic_count_exceeds: 5
---

你是 March 的记忆整理模块。你的任务是观察主 agent 刚刚完成的工作，
决定是否需要整理记忆。

你可以：
- 从对话中提取值得长期保留的事实、决策、模式
- 合并同 topic 下语义重复的记忆
- 清理过时的记忆
- 为高频使用的记忆补充更准确的 tags
- 注入 hint 提醒主 agent 注意某些事项

当前轮没有值得记忆的内容时，直接结束，不要勉强操作。
```

### Frontmatter 字段

| 字段 | 必填 | 说明 |
|------|------|------|
| `name` | 否 | 省略时用文件名（不含 `.md`） |
| `order` | 否 | 执行优先级，数字升序，默认 500 |
| `trigger` | 是 | 触发时机（见下方） |
| `frequency` | 否 | 每 N 次触发才真正执行一次，省略 = 每次执行 |
| `pre_check` | 否 | 程序阶段条件列表，全部满足才进入 AI 阶段 |

正文即 `subconscious_prompt`，March 在注入时会自动追加工具限制声明（"以下工具已禁用：run_command、write_file、open_file、close_file、write_note、remove_note……调用将返回错误"）。

### 触发时机

| trigger 值 | 含义 |
|------------|------|
| `turn_end` | 主 agent 一轮结束后 |
| `idle(N)` | 用户空闲 N 秒后 |
| `session_start` | session 初始化完成后 |
| `session_end` | session 结束前 |

---

## 多 Subconscious 编排

同一 trigger 下可能有多个 subconscious，按 `order` 字段升序线性执行：

```
turn_end 触发
  │
  ├─ memory-curator    (order: 100)  → pre_check → 跳过
  ├─ pattern-detector  (order: 200)  → pre_check → AI 阶段 → 写入记忆
  └─ hint-injector     (order: 300)  → pre_check → AI 阶段 → 注入 hint
```

线性执行的好处：前一个 subconscious 的副作用（比如 memory-curator 合并了记忆）在下一个执行时已经生效，天然形成管道效果。

`order` 间隔建议留大（100、200、300），方便后续插入。

---

## 与现有设计的关系

### 与 Agent 的关系

Agent 和 Subconscious 是正交的两个维度：

- Agent 决定"谁在前台工作"——切换人格、切换 system_prompt
- Subconscious 决定"后台在做什么"——记忆整理、模式识别、主动提醒

Agent 切换不影响 subconscious 的运行。无论前台是 March、reviewer 还是 architect，后台的 memory-curator 都照常工作。Subconscious 通过上下文前缀中的 `recent_chat` 自然感知到当前是哪个角色在工作。

### 与 Memory 的关系

Memory 设计文档中"AI 主动管理"的期望，实际上更适合由 subconscious 承担：

- **主 Agent**：工作中顺手 `memorize` 重要发现（即时写入）
- **Subconscious**：负责记忆的**维护**——合并、提炼、清理、补充 tags

两者互补：主 agent 是记忆的主要生产者，subconscious 是记忆的管理者。

### 与 Hints 的关系

Hints 的设计初衷是"外部工具注入接口"，Subconscious 是 hints 最正当的内部生产者：

- 外部工具（Telegram bot、CI 通知）通过本地 API 注入 hints
- Subconscious 通过工具调用注入 hints

两者走同一通道，主 agent 不区分来源，统一在 `[hints]` 层感知。

### 与上下文压力的关系

Subconscious 的执行不增加主 agent 的上下文压力——它有自己独立的 LLM 调用，turn_history 和 subconscious_prompt 不会累积到主 agent 的上下文中。

但 subconscious 产生的副作用（新增记忆 → memory_index 变大，注入 hint → hints 层多一条）会间接影响主 agent 的上下文大小。这通过 memory_index 的 token 预算（~500 tokens）和 hints 的 TTL 机制自然控制。

### 与 Prefix Cache 的关系

Subconscious 是 prefix cache 的最大受益者之一。主 agent 的一轮调用已经让 provider 缓存了从 `system_core` 到 `recent_chat` 的完整前缀，subconscious 紧随其后运行，几乎必然命中缓存。这是"共享前缀 + 追加后缀"设计的核心优势。
