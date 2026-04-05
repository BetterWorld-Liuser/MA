# Agents / Teams 设计

> 从 [DESIGN.md](DESIGN.md) 延伸：Agent 是人格配置，不是独立进程。多角色复用 March 已有的上下文分层架构，通过"共享层 + 私有层"实现即时继承与工作隔离的平衡。

---

## 核心概念

Agent 是一套**人格配置**，叠加在同一个 task 的上下文之上。切换角色不是启动新进程，而是更换 `system_prompt` 和私有工作空间，同时保留对共享上下文的可见性。

March（默认角色）也是一个普通 Agent，没有特殊待遇。用户可以在设置页自定义 March 的 system_prompt（即 `system_core`），并随时恢复默认。

### 交互语义：不是“切模式”，而是“某个人接话”

虽然底层实现是“激活某个 AgentProfile”，但对用户暴露的体验不应是“系统切到了某个模式”，而应是“团队里的某个角色接过当前问题继续往下做”。

这意味着：

- `@reviewer` 的语义更接近“把这件事交给 reviewer 处理”，不是“把 AI 切成 reviewer 模式”
- 角色首句应直接进入任务，最多做极轻量、自然而人格化的承接，不应汇报内部状态
- 用户看到的是“代码审查员开始说话了”，而不是“系统提示：已切换到代码审查员”

不推荐的开场：

```text
我已切到 reviewer 角色。
我现在是 reviewer（代码审查员）。
收到，已切换身份，下面开始审查。
```

推荐的开场：

```text
我先看 auth 这块的实现和测试覆盖，重点帮你找风险点。
先从并发和错误处理看起，这两块最容易藏问题。
这段我先按代码审查的标准过一遍，先看实现边界是否清楚。
```

换句话说，**角色感主要来自持续稳定的说话方式、关注点和判断标准，而不是自报“我现在切成谁了”**。

---

## 上下文分层：共享 + 私有

每个 Agent 实际看到的上下文由两层叠加而成：

```
agent 看到的 open_files = task 共享文件 ∪ 自己的私有文件
agent 看到的 notes      = task 共享笔记 ∪ 自己的私有笔记
agent 看到的 recent_chat = task 级（天然共享，包含所有角色发言）
agent 看到的 hints       = task 级（天然共享）
```

### 写入归属规则

| 操作来源 | 目标层 | 理由 |
|---------|--------|------|
| 用户 `@file` mention | **共享** | 用户主动放上桌面，是给整个讨论的 |
| 用户在右栏手动打开文件 | **共享** | 同上 |
| 用户在右栏创建/编辑 note | **共享** | 用户操作都是 task 级的 |
| `AGENTS.md` 自动加入 | **共享** + locked | 项目规则，所有角色遵守 |
| AI `open_file` | **私有** | 角色的工作文件，不污染其他角色 |
| AI `write_file` 后自动加入 | **私有** | 角色写的文件是自己工作的一部分 |
| AI `write_note` | **私有** | 角色自己的工作记忆 |

### 继承机制

不需要特殊的"上下文传递"。当 `@reviewer` 第一次被召唤：

```
reviewer 看到：
  共享 open_files  → 用户之前 @ 进来的文件、AGENTS.md
  私有 open_files  → 空（第一次来）
  共享 notes       → 用户手动写的背景信息
  私有 notes       → 空
  recent_chat      → 完整对话，包含之前所有角色发言
  hints            → 天然共享
```

共享层即继承。角色通过 `recent_chat` 了解讨论历史，通过共享文件看到当前焦点。角色工作中自己 `open_file` 的文件、`write_note` 的笔记都是私有的，不影响其他角色的工作空间。

如果角色 A 在聊天中提到某个文件路径，角色 B 看到后自然会 `open_file` 去查看——这模拟真实团队协作：你不会自动看到同事所有打开的文件，但他会告诉你关键发现。

### 上下文构建排列

共享在前、私有在后，保护 prefix cache：

```
[open_files]
  ── 共享文件（稳定在前）──
  AGENTS.md
  src/auth.rs
  ── 角色私有文件（靠后，换角色时只影响这部分）──
  src/security.rs
  tests/auth_test.rs

[notes]
  ── 共享笔记 ──
  background: ...
  ── 角色私有笔记 ──
  findings: ...
```

---

## 数据模型

```rust
struct AgentProfile {
    id: i64,
    name: String,              // "reviewer", "architect"
    display_name: String,      // "代码审查员", "架构师"
    description: String,       // 一句话职责摘要，供 roster / mention / 设置页展示
    system_prompt: String,     // 角色行为指令
    avatar_color: String,      // 聊天区视觉区分色
    provider_id: Option<i64>,  // 绑定的 provider，None = 跟随 task 当前模型
    model_id: Option<String>,  // 绑定的模型，None = 跟随 task 当前模型
    skills_config: Option<SkillsConfig>,  // 可选，大多数角色不设置
    created_at: i64,
    updated_at: i64,
}

struct SkillsConfig {
    mode: SkillsFilterMode,    // Only | Disable
    skills: Vec<String>,       // skill name 列表
}

enum SkillsFilterMode {
    Only,     // 白名单：只能用这些 skills
    Disable,  // 黑名单：禁用这些 skills
}
```

### 简短描述（description）

每个 Agent 除了完整的 `system_prompt`，还应有一条**简短描述**，长度控制在一句话内，回答“这个角色主要是干嘛的”。

这个字段的作用不是替代 `system_prompt`，而是服务于两个场景：

- 给用户看：设置页、右栏 Agents、`@` mention 面板里，需要快速理解这个角色的职责
- 给 AI 看：在 prompt 中显式注入“当前有哪些 agent 可用，以及各自负责什么”，降低 agent-to-agent handoff 的猜测成本

示例：

```text
reviewer 代码审查员
短描述：重点审查实现风险、边界条件、测试缺口和潜在回归。

architect 架构师
短描述：关注模块边界、抽象层次、长期演进成本和设计一致性。
```

约束：

- 简短描述应稳定、可扫描，避免写成长段 prompt
- 它描述的是“职责定位”，不是行为细则；具体行为仍由 `system_prompt` 决定
- 新建角色时建议必填；旧角色若缺失，可先回退为从 `system_prompt` 提炼出的首句摘要，但 UI 应鼓励补全

### 模型绑定

每个角色可以绑定特定的 provider/model。解析优先级：

```
1. 角色自身绑定的 provider_id + model_id（用户在角色设置中指定）
2. 当前 task 的 selected_provider + selected_model（聊天输入框选择器）
```

即：角色有自己的模型就用自己的，没有就跟随 task。

这允许用户把不同角色分配给不同模型——比如让简单的代码审查角色跑便宜快速的模型，让架构师角色跑更强的模型。默认不设置，所有角色共用 task 选择的模型。

### 默认角色（March）

March 是一个内置的 AgentProfile，`system_prompt` 即 `system_core`。用户可以在设置页自定义 March 的 system_prompt，March 存储自定义版本和内置默认版本两份：

- 用户编辑后，使用自定义版本
- 提供"恢复默认"按钮，一键回退到内置 `system_core`
- 恢复默认不删除自定义版本，只是切换回内置版本，用户可以再次启用自定义

---

## 发现路径与优先级

Agent 从两个位置加载，优先级从低到高：

```
1. 用户级  — ~/.march/agents/*.md
2. 项目级  — .march/agents/*.md
```

同名时项目级覆盖用户级，与 Skills 优先级逻辑一致。

### 文件格式

```markdown
---
name: reviewer
display_name: 代码审查员
description: 重点审查实现风险、边界条件、测试缺口和潜在回归。
avatar_color: "#3B82F6"
model: anthropic/claude-haiku-4-5   # 可选，格式 provider_name/model_id
---

你是一位严格的代码审查员，专注于代码质量、性能和安全。

审查时优先关注：
- 潜在的安全漏洞
- 性能瓶颈
- 错误处理是否完备
- 代码可读性和命名规范
```

`model` 可省略，省略时跟随 task 当前模型。格式为 `provider_name/model_id`，March 加载时按 provider name 匹配已配置的 provider。如果指定的 provider 或 model 在当前环境中不存在，March 回退到 task 模型并在加载日志中给出警告。

`name` 可省略，省略时用文件名（不含 `.md`）。`display_name` 可省略，省略时用 `name`。`description` 强烈建议填写，供 UI 和 prompt roster 使用；缺失时 March 可退回到自动摘要。`avatar_color` 可省略，March 自动分配。

### Skills 配置

大多数角色不需要配置 skills——角色的 `system_prompt` 自然引导 AI 选择合适的 skill。只在需要**限制**时使用：

```markdown
---
name: security-auditor
display_name: 安全审计员
skills:
  mode: only
  list: [rust, security]
---
```

Frontmatter 中的 `skills` 字段可选，省略时继承 task 默认 skills。

---

## @mention 工作机制

### 用户 @Agent

聊天输入中 `@角色名` 触发角色切换：

```
用户: @reviewer 帮我看看 auth 模块的代码质量

March 处理流程：
  1. 识别 @reviewer
  2. 构建本轮 AgentContext：
     system_core = 基础指令片段 + reviewer.system_prompt
     available_agents = task 当前可用 agent roster（含 name / display_name / description）
     open_files  = 共享文件 ∪ reviewer 私有文件
     notes       = 共享笔记 ∪ reviewer 私有笔记
     recent_chat = task 最近 N 轮（包含所有角色发言）
     skills      = reviewer.skills_config ?? task 默认
  3. agent loop 正常运行
  4. 轮结束 → 回复出现在聊天区，带 reviewer 标识
     recent_chat 追加这条记录
```

这里的“触发角色切换”是实现描述，不是推荐暴露给用户的文案。面向用户的语义应统一为：

- `@Agent` = 请该角色接手当前问题
- 当前回复 = 该角色的发言
- 不额外插入“已切到某角色”的系统播报

### Agent roster 注入

为了让 AI 不靠“猜”去决定该 `@` 谁，March 在构建每轮上下文时，应显式注入当前 task 可用的 agent roster。

推荐结构：

```text
# Available Agents
- march | March | 默认通用搭档，负责通用 coding、查证和推进。
- reviewer | 代码审查员 | 重点审查实现风险、边界条件、测试缺口和潜在回归。
- architect | 架构师 | 关注模块边界、抽象层次、长期演进成本和设计一致性。

active_agent: reviewer
```

这层 roster 的语义是：

- 告诉当前 agent“现在系统里有哪些可协作角色”
- 告诉它“这些角色分别擅长什么”，避免只凭名字猜职责
- 让 handoff 成为“找对同事接手”，而不是“随口编一个常见角色名”

它不是权限系统，也不替代 `system_prompt`。真正激活后的行为仍由目标角色自己的 `system_prompt` 决定。

### AI @Agent（自动接力）

AI 在输出中 @另一个角色时，March 自动触发下一轮：

```
reviewer: 这段并发逻辑我拿不准，@architect 你看看设计是否合理？
  │
  └─ March 检测到 @architect
     → 本轮结束后，自动以 architect 身份启动下一轮
     → architect 在 recent_chat 里看到 reviewer 的请求
     → architect 工作完成后，如果也 @了其他角色，继续接力
```

不设深度上限。用户可以随时按暂停按钮停止所有 AI 工作。

### @mention 面板

输入 `@` 时，mention 面板同时列出文件和角色：

```
┌──────────────────────────────────────────────┐
│  > rev                                       │
│  👤 reviewer  代码审查员                      │  ← 角色
│     重点审查实现风险、边界条件、测试缺口…      │
│  src/review.rs                               │  ← 文件
└──────────────────────────────────────────────┘
```

角色排在文件前面，视觉上用图标区分。

---

## 角色创建

### AI 创建

用户在聊天中描述需求，当前活跃 AI 通过工具创建：

```
用户: 帮我创建一个代码审查角色，要求严格，关注性能和安全

March:
  ┌─ create_agent
  │  name: "reviewer"
  │  display_name: "代码审查员"
  │  system_prompt: "你是一位严格的代码审查员..."
  └─ done

  已创建 "代码审查员" 角色，你可以用 @reviewer 召唤它。
```

AI 也可以通过 `update_agent` 优化已有角色的 system_prompt：

```
用户: 帮我优化一下 reviewer 的提示词，让它更关注安全问题

March:
  ┌─ update_agent
  │  name: "reviewer"
  │  system_prompt: "你是一位严格的代码审查员，安全优先..."
  └─ done
```

### 用户管理

设置页提供"角色管理"板块：

```
[角色管理]

  March (默认)        [编辑 system prompt]  [恢复默认]
  reviewer  代码审查员  [编辑]  [删除]
  architect 架构师      [编辑]  [删除]

  [+ 新建角色]
```

编辑页面：

```
名称          [reviewer        ]
显示名        [代码审查员       ]
短描述        [重点审查实现风险、边界条件……]
头像颜色      [🔵 ∨]

模型          [跟随任务默认 ∨]
                ┌─────────────────────────┐
                │ ✓ 跟随任务默认           │
                │ ─────────────────────── │
                │ Anthropic               │
                │   claude-sonnet-4-6     │
                │   claude-haiku-4-5      │
                │ OpenAI                  │
                │   gpt-5.4              │
                └─────────────────────────┘

System Prompt
┌──────────────────────────────────────┐
│ 你是一位严格的代码审查员……            │
│                                      │
│                                      │
└──────────────────────────────────────┘

Skills 限制（可选）
  ○ 不限制（继承默认）
  ○ 仅允许: [rust] [security] [+]
  ○ 禁用:   [deploy] [+]

                        [取消]  [保存]
```

---

## 工具定义

```rust
// 角色管理工具
create_agent {
    name: String,
    display_name: String,
    description: String,
    system_prompt: String,
    model: Option<String>,  // "provider_name/model_id", 省略则跟随 task
}

update_agent {
    name: String,
    // 以下均为可选，只更新提供的字段
    display_name: Option<String>,
    description: Option<String>,
    system_prompt: Option<String>,
    model: Option<String>,  // 设为 "" 可清除绑定，回退到跟随 task
}

delete_agent {
    name: String,
}
```

这些工具在 `[tools]` 层注册，所有角色都可以使用。删除当前正在活跃的角色时，March 拒绝操作并返回错误提示。

---

## 存储

### 用户级（~/.march/settings.db）

```sql
CREATE TABLE agent_profiles (
    id             INTEGER PRIMARY KEY,
    name           TEXT    NOT NULL UNIQUE,
    display_name   TEXT    NOT NULL,
    description    TEXT    NOT NULL,
    system_prompt  TEXT    NOT NULL,
    avatar_color   TEXT,
    provider_id    INTEGER REFERENCES providers(id) ON DELETE SET NULL,  -- nullable
    model_id       TEXT,     -- nullable, 与 provider_id 配对
    skills_config  TEXT,     -- JSON, nullable
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL
);

-- March 默认角色的自定义 system_core
-- key: 'custom_system_core'
-- 存在 settings 表中，与 default_provider_id 等并列
```

### 项目级（.march/agents/*.md）

文件系统即真相源，watcher 监控变化。同名时项目级覆盖用户级。

### Task 级上下文存储

```sql
-- open_files 加 scope 列
CREATE TABLE open_files (
    task_id   INTEGER NOT NULL REFERENCES tasks(id),
    scope     TEXT    NOT NULL DEFAULT 'shared',  -- 'shared' | agent name
    path      TEXT    NOT NULL,
    position  INTEGER NOT NULL,
    locked    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (task_id, scope, path)
);

-- notes 加 scope 列
CREATE TABLE notes (
    task_id   INTEGER NOT NULL REFERENCES tasks(id),
    scope     TEXT    NOT NULL DEFAULT 'shared',  -- 'shared' | agent name
    note_id   TEXT    NOT NULL,
    content   TEXT    NOT NULL,
    position  INTEGER NOT NULL,
    PRIMARY KEY (task_id, scope, note_id)
);
```

查询当前角色视角：

```sql
SELECT * FROM open_files
WHERE task_id = ? AND scope IN ('shared', ?)
ORDER BY scope = 'shared' DESC, position;

SELECT * FROM notes
WHERE task_id = ? AND scope IN ('shared', ?)
ORDER BY scope = 'shared' DESC, position;
```

---

## UI 表现

### 聊天区

不同角色的消息带颜色标识：

```
┌──────────────────────────────────────┐
│                         用户  14:32  │
│  @reviewer 帮我看看 auth 模块        │
│                                      │
│ 🔵 代码审查员  14:32                  │
│ 看了一下 auth.rs，有几个问题……        │
│ ┌─ open_file src/auth.rs            │
│ └─ ...                              │
│ @architect 这个锁的粒度我觉得有问题   │
│                                      │
│ 🟢 架构师  14:33                     │
│ 同意，建议改成 RwLock……              │
└──────────────────────────────────────┘
```

March 的消息继续显示 "March"，不加额外角色标识。

聊天区中的角色标签本身已经足够表达“现在是谁在说话”。因此正文不需要再重复“我现在是 reviewer / 已切换到 reviewer”之类的自我说明，否则会把体验从“多人协作”拉回“单 AI 切模式”。

### 右栏上下文面板

在 Skills 下方显示 Agents 区域：

```
[Agents]
  🔵 reviewer    重点审查实现风险、边界条件…   (当前)
  🟢 architect   关注模块边界与长期演进成本
  ── March       默认通用搭档
```

**Scope 标记策略**：只有当 task 中出现过多角色时，open_files 和 notes 区域才显示 scope 标记。单角色场景（只有 March）下 UI 表现与当前完全一致，不显示 scope 信息。

多角色时：

```
[Open Files]
  src/auth.rs        2.8k tok
🔒 AGENTS.md         0.3k tok
  src/security.rs    1.2k tok   👤
  tests/auth_test.rs 0.8k tok   👤

[Notes]
  background   项目背景……
  findings     发现三个安全问题……   👤
```

无标记 = 共享，👤 = 当前角色私有。切换角色时，右栏自动切换到新角色的视角。

---

## 与现有设计的关系

### system_core 的构成

角色激活时，AI 实际收到的 system 层内容：

```
[system_core]
  ── 不可移除的基础指令（工具使用规则、安全约束等）──
  ── 当前 task 的可用 agents roster（name / display_name / description）──
  ── 角色 system_prompt（March 用 custom 或 built-in，其他角色用各自的）──
```

基础指令是所有角色共享的"地基"，确保无论什么角色都能正确使用工具、遵守安全规则。agent roster 负责告诉当前 agent“团队里有哪些人以及各自职责”。角色的 `system_prompt` 则定义它自己特有的行为风格和专业方向。

### 与 Skills 的关系

Skills injection 仍在 `[injections]` 层，但索引内容可能因角色的 `skills_config` 过滤而不同。如果角色设置了 `skills.mode: only`，索引中只展示白名单内的 skills。

### 与 recent_chat 的关系

`recent_chat` 记录包含所有角色的发言，每条记录额外携带 `agent_name` 字段：

```rust
struct ChatTurn {
    role: Role,         // user | assistant
    agent: String,      // "march" | "reviewer" | "architect"
    content: String,
    tool_summaries: Vec<String>,
    timestamp: SystemTime,
}
```

渲染进上下文时，assistant 消息带角色名前缀，让后续角色能区分是谁说的。

### 与 conversation_turns 的关系

```sql
-- conversation_turns 加 agent 列
CREATE TABLE conversation_turns (
    id             INTEGER PRIMARY KEY,
    task_id        INTEGER NOT NULL REFERENCES tasks(id),
    role           TEXT    NOT NULL,
    agent          TEXT    NOT NULL DEFAULT 'march',  -- agent name
    content        TEXT    NOT NULL,
    tool_summaries TEXT,
    created_at     INTEGER NOT NULL
);
```
