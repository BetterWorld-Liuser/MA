# 记忆系统

> 从 [DESIGN.md](DESIGN.md) 延伸：跨 session、跨 task 的持久记忆，AI 在长期使用中积累对项目和用户的认知。与 Notes（轮内/task 内工作记忆）互补，记忆是 March 的长期知识层。

---

## 核心问题

Notes 是 task 级的工作记忆，task 结束就沉默了。AI 每次开新 task 都从零开始，不记得"上次改 auth 模块时发现的坑""用户不喜欢过度注释""这个项目测试前要先跑 migration"。

记忆系统解决的是：**让 AI 跨 session 保持对项目和用户的认知连续性。**

存储不是难点。难点是**召回**——记忆会随时间积累到几百上千条，每轮上下文窗口有限，必须只加载当前真正相关的子集。

---

## 设计原则

### 不用 RAG

RAG（向量数据库 + embedding 检索）需要 embedding 模型和外部存储，违背 March 本地优先、轻量的设计原则。March 的替代方案：**SQLite FTS5 全文搜索 + 路径前缀匹配 + 时间/频率加权**，全部在 SQLite 内完成，零外部依赖。

### 二层召回：索引常驻 + 详情按需

借鉴 `open_files` 的思路（列表在上下文里，内容由 watcher 按需提供）：

- **Memory Index**：每轮注入上下文，每条记忆只占一行摘要（id + 类型 + 话题 + 标题），控制在 ~500 tokens
- **Memory Detail**：AI 调用 `recall_memory(id)` 按需加载，内容作为工具返回值出现在当前轮，不持久留在上下文

March 的匹配负责"粗筛 + 排序"（偏向召回率），AI 自己负责"精选"（看了索引后决定要不要 recall）。这和 March 上下文管理的一贯哲学一致：**系统提供结构，AI 做决策**。

### 类型 + 话题两级组织

线性列表在记忆超过几十条后不可管理。用两级结构：

- **类型**（一级）：记忆的性质，帮助 AI 和匹配逻辑做过滤
- **话题**（二级）：自由文本但鼓励复用，同话题记忆可合并压缩

---

## 数据模型

```rust
struct Memory {
    id: String,                 // 项目级：文件名（不含 .md）；全局级：数字 id
    level: MemoryLevel,
    scope: MemoryScope,
    memory_type: String,           // 自由文本，约定常用值见下方
    topic: String,              // 二级话题，自由文本，如 "auth"、"testing"、"style"
    title: String,              // 索引层展示的一行摘要
    content: String,            // 详情层的完整内容
    tags: Vec<String>,          // 用于匹配的关键词、文件路径等
    access_count: u32,          // 被 recall 的次数，用于排序
    skip_count: u32,            // 出现在索引中但未被 recall 的次数
    created_at: i64,
    updated_at: i64,
}

enum MemoryLevel {
    Project,    // 存为 .march/memories/*.md，随项目走，可 git 管理
    Global,     // 存在 ~/.march/settings.db，用户级，跨项目
}

// memory_type 是自由文本，不是枚举。
// 以下为约定的常用值，AI 可自创新类型。
// 常用值: "fact", "decision", "pattern", "preference"

enum MemoryScope {
    Shared,          // 所有 agent 可见
    Agent(String),   // 仅特定 agent 可见
}
```

### 类型语义

`memory_type` 是自由文本字符串，不做枚举限制。March 预定义以下常用值作为约定，system_prompt 中列出供 AI 参考，但 AI 可以根据需要自创新类型：

| 约定值 | 含义 | 典型来源 |
|--------|------|---------|
| `fact` | 项目客观事实 | AI 在工作中发现并记录 |
| `decision` | 设计/架构决策及原因 | 用户说明或 AI 参与讨论后记录 |
| `pattern` | 重复出现的工作流程/步骤 | AI 多次执行同类任务后总结 |
| `preference` | 用户的风格偏好、工作习惯 | 用户明确要求或 AI 从反馈中归纳 |

**为什么不用枚举**：记忆的分类需求会随使用场景自然演化。比如 AI 可能需要 `"caveat"`（踩过的坑）、`"contact"`（项目相关人员）、`"glossary"`（项目术语表）等类型，硬编码枚举会阻碍这种自然生长。自由文本让 AI 成为类型体系的共建者，而不只是消费者。

**匹配中的处理**：`memory_type` 参与 FTS5 索引（作为 tags 的一部分），可被搜索和过滤，但不作为硬性过滤条件。索引渲染时显示类型的缩写形式（如 `[fact]`、`[caveat]`），帮助 AI 快速判断记忆性质。

### Level 语义：项目级 vs 全局级

记忆分为两个层级，解决"项目知识随项目走，个人偏好随用户走"的需求：

| | 项目级 (Project) | 全局级 (Global) |
|---|---|---|
| 存储位置 | `.march/memories/*.md` | `~/.march/settings.db` |
| 可否 git 管理 | 可以，纯文本文件 | 不可，DB 二进制 |
| 随什么走 | 随项目仓库 | 随用户 |
| 典型内容 | 项目事实、架构决策、项目特有模式 | 用户偏好、跨项目通用模式 |
| 多人协作 | 团队共享，提交到仓库后其他人也能受益 | 个人专属 |

**默认 level 推断**：AI 调用 `memorize` 时可不指定 level，March 按 memory_type 推断默认值：

- `preference` → **Global**（用户偏好跨项目通用）
- 其他所有类型 → **Project**（默认假设记忆与当前项目绑定）

AI 可显式指定 level 覆盖默认推断。对于自创类型，默认归入 Project 是安全的——项目级记忆最多只是不跨项目，不会丢失；而全局记忆如果不该全局化，反而会在其他项目中造成噪音。

### Scope 规则

复用 [agents-teams.md](agents-teams.md) 的 shared + private 模型：

| 场景 | Scope | 理由 |
|------|-------|------|
| 项目事实、设计决策 | **Shared** | 所有角色都该知道 |
| 用户偏好 | **Shared** | 用户意图是全局的 |
| 角色的专业经验积累 | **Agent** | reviewer 的审查经验不应干扰 architect 的视角 |
| AI 通过 `memorize` 写入 | 默认 **Shared**，可指定 agent scope | 大多数记忆天然是共享的 |

Level 和 Scope 是正交的两个维度。项目级记忆也可以有 Agent scope（只有某个角色在此项目中看得到），全局记忆也可以有 Agent scope（某个角色的跨项目经验）。

### Agent 删除时的记忆归属

Agent 被删除时，其名下的记忆不随之删除——记忆的价值不应因角色生命周期结束而丢失。处理策略：

**scope 自动回落到 Shared**。March 在执行 `delete_agent` 时，将该 agent 名下的所有记忆（项目级和全局级）的 scope 从 `Agent("reviewer")` 改为 `Shared`。

这样做的理由：
- 记忆是知识积累，角色只是观察视角。reviewer 记下的"auth 模块有个竞态条件"不会因为 reviewer 被删就失去价值
- 回落到 Shared 后，所有角色都能看到这些记忆，不会有知识盲区
- 如果某些记忆确实不再需要，AI 或用户可以事后手动清理

**用户确认**：`delete_agent` 的工具返回中会注明"N 条记忆已从 reviewer 私有转为共享"，让用户和 AI 知道发生了什么。

对于项目级记忆，scope 变更意味着修改 `.march/memories/*.md` 文件的 frontmatter；对于全局级记忆，直接 UPDATE DB。

---

## 存储

记忆分两个存储后端：项目级用 Markdown 文件（git 友好），全局级用 SQLite（跨项目持久）。两者在 session 启动时合并加载到统一的运行时 FTS5 索引中。

### 项目级存储：`.march/memories/*.md`

纯文本文件，可提交到 git，团队共享：

```markdown
---
type: fact
scope: shared
topic: auth
tags: auth jwt token src/auth
access_count: 5
skip_count: 0
created_at: 1743811200
updated_at: 1743897600
---
# JWT refresh token 有效期 7 天，access token 15 分钟

项目的 auth 模块使用双 token 方案：
- access token 有效期 15 分钟，用于 API 鉴权
- refresh token 有效期 7 天，用于静默续期
- refresh token 存储在 httpOnly cookie 中，不暴露给前端 JS
```

**文件名即 id**：文件名（不含 `.md`）作为记忆 id，如 `jwt-token-policy.md` → id 为 `jwt-token-policy`。AI 在 `memorize` 时提供 id，March 检查是否重名。

**frontmatter 字段**：`type`、`scope`、`topic`、`tags`、`access_count`、`skip_count`、`created_at`、`updated_at`。正文第一个 `#` 标题行即 `title`，标题之后的内容即 `content`。

**与 watcher 的关系**：`.march/memories/` 目录纳入 watcher 监控范围。用户手动编辑记忆文件（或 git pull 带来新记忆），watcher 检测到变化后重建 FTS5 索引中对应条目。这与 `open_files` 的 Source of Truth 哲学一致——文件系统即真相源。

**access_count / skip_count 的写回**：这两个字段随使用递增，March 在 session 结束时或定期（如每 10 分钟）批量写回文件的 frontmatter。不在每次 recall 时立即写盘，避免高频 IO 和频繁产生 git diff 噪音。

### 全局级存储：`~/.march/settings.db`

与现有 `agent_profiles` 等表并列：

```sql
CREATE TABLE memories (
    id           INTEGER PRIMARY KEY,
    scope        TEXT    NOT NULL DEFAULT 'shared',  -- 'shared' | agent name
    memory_type  TEXT    NOT NULL,                    -- 自由文本，如 'fact', 'decision', 'pattern', 'preference' 等
    topic        TEXT    NOT NULL,
    title        TEXT    NOT NULL,
    content      TEXT    NOT NULL,
    tags         TEXT    NOT NULL DEFAULT '',          -- 空格分隔
    access_count INTEGER NOT NULL DEFAULT 0,
    skip_count   INTEGER NOT NULL DEFAULT 0,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE INDEX idx_memories_scope ON memories(scope);
CREATE INDEX idx_memories_topic ON memories(topic);
```

全局记忆不需要 git 友好，DB 存储更高效，查询也更方便。

### 运行时 FTS5 索引

FTS5 索引是**运行时构建**的，不持久化。Session 启动时从两个来源加载所有记忆，写入内存中的 FTS5 虚拟表：

```sql
-- 运行时 FTS5 表，在 march.db（工作目录）的内存 attach 中
CREATE VIRTUAL TABLE memory_fts USING fts5(
    memory_id,      -- 项目级: "p:jwt-token-policy"  全局级: "g:42"
    title,
    content,
    tags,
    topic,
    tokenize='jieba'
);
```

**id 前缀约定**：索引中用 `p:` 前缀标记项目级记忆，`g:` 前缀标记全局级记忆，保证 id 不冲突，也方便 recall 时路由到正确的存储后端。

**重建时机**：

- Session 启动时全量构建
- Watcher 检测到 `.march/memories/` 文件变化时增量更新对应条目
- 全局记忆写入 DB 时通过触发器或显式调用同步更新

**同名冲突**：项目级和全局级的 id 空间天然隔离（前缀不同），不存在冲突。但如果项目级和全局级存在**语义重复**的记忆（如同一个 topic 下标题近似），March 在索引渲染时标注来源，AI 可自行合并或删除。

### Jieba 分词

SQLite FTS5 的默认 tokenizer 对中文逐字切分，无法识别词组（"刷新令牌"会被切成"刷""新""令""牌"，匹配噪音大）。March 通过 SQLite FTS5 的自定义 tokenizer 接口接入 jieba：

**实现方式**：在 Rust 侧用 `jieba-rs` crate 实现 FTS5 tokenizer API（`xCreate`、`xTokenize` 等），在数据库初始化时注册为 `tokenize='jieba'`。jieba-rs 是纯 Rust 实现，内置词典，无外部依赖。

**分词策略**：

- 中文文本：jieba 精确模式分词
- 英文/路径/标识符：按空格和 `/` `.` `_` `-` 分割后保留原始 token
- tags 字段：已经是空格分隔的，直接按空格切分即可，不走 jieba

**自定义词典**：jieba-rs 支持加载用户词典。March 可选地从 `.march/dict.txt` 加载项目专用词汇（如项目特有的术语、模块名），格式与 jieba 标准词典一致。不存在则跳过，只用内置词典。

---

## 匹配机制

### 信号提取

每轮上下文构建时，March 从当前状态提取匹配信号：

| 信号源 | 提取方式 | 权重 | 示例 |
|--------|---------|------|------|
| 用户最新消息 | 原文（去掉 @mention 和标点） | 高 | "auth 模块登录超时问题" |
| open_files 路径 | 路径段拆分 | 高 | `src/auth/middleware.rs` → "auth middleware" |
| 当前 agent | scope 过滤 | 过滤 | "reviewer" |
| task 名称 | 原文 | 中 | "修复登录超时" |
| 最近 2 轮 AI 回复 | 提取文件路径和关键名词 | 低 | "JWT token expire" |

### 双通道查询

**通道 A：FTS5 文本匹配**

从信号源构造 FTS5 查询，用 BM25 排序：

```sql
SELECT memory_id, title, topic, rank AS bm25_score
FROM memory_fts
WHERE memory_fts MATCH ?query
ORDER BY rank
LIMIT 50;
-- 结果再在 Rust 侧按 scope 过滤（scope 信息在内存的 Memory 结构体中）
```

FTS5 查询支持列级权重（`{title tags}: auth middleware OR {content}: 登录超时`），title 和 tags 的命中权重高于 content。

**通道 B：路径前缀匹配**

FTS5 对文件路径的分词效果有限，路径匹配单独做：

```sql
SELECT m.*
FROM memories m
WHERE m.scope IN ('shared', ?current_agent)
  AND EXISTS (
      -- tags 中存的路径片段与当前 open_files 做前缀匹配
      -- 实际实现在 Rust 侧：遍历 open_files 路径段，
      -- 与每条记忆的 tags 做前缀比较
  );
```

具体实现：Rust 侧将 open_files 的路径拆成段集合（如 `{"src", "auth", "middleware"}`），与每条记忆的 tags 做交集，交集非空即命中。这个操作在内存中做，不依赖 SQL。

### 合并排序

两通道的结果取并集，按综合分数排序：

```
score = w1 × bm25_score          -- FTS5 文本相关性（归一化到 0-1）
      + w2 × path_match_score    -- 路径命中数 / open_files 总数
      + w3 × recency_score       -- 1.0 / (1 + days_since_update)
      + w4 × frequency_score     -- log(1 + access_count) / log(1 + max_access_count)
```

初始权重：`w1=0.5, w2=0.25, w3=0.15, w4=0.10`。硬编码即可，后续根据使用反馈微调。

### 冷启动

记忆总量 ≤ 50 条时，跳过匹配，全部放进索引——50 条标题约 200-300 tokens，在预算内。匹配逻辑只在记忆量超过阈值后启用。

---

## 上下文集成

### 层级位置

```
[system_core]
[injections]
[tools]
[open_files]
[notes]
[memory_index]       ← 新增
[session_status]
[runtime_status]
[hints]
[recent_chat]
```

位于 notes 之下、session_status 之上。理由：比 notes 不稳定（每轮匹配结果随上下文变化），但比 session/runtime 状态更具持续性。

### 索引渲染

```
[memory_index]
匹配到 12 条相关记忆，可用 recall_memory(id) 查看详情：

  p:jwt-token-policy  [fact]     auth     JWT refresh token 有效期 7 天，access token 15 分钟
  p:auth-middleware    [decision] auth     选了 middleware 方案而非 guard 方案，因为需要路由级粒度
  p:test-migration     [pattern]  testing  auth 相关测试必须先跑 seed_users migration
  g:12                 [pref]     style    用户不喜欢过度注释，改动处只在不直观时加注释
  ...
```

`p:` 前缀 = 项目级记忆（存在 `.march/memories/` 中），`g:` 前缀 = 全局级记忆（存在用户 DB 中）。

Token 预算约 500 tokens，按 score 降序截取。如果当轮完全无匹配，`[memory_index]` 层不出现。

### recall 后的流转

AI 调用 `recall_memory(id)` → March 返回完整 content 作为工具结果 → 出现在当前轮内历史 → 轮结束后随轮内历史一起丢弃。

如果 AI 认为某条记忆的信息在后续轮次也需要，应主动写入 Notes——这和"工具执行结果需要通过 Notes 跨轮保留"的现有逻辑一致。

---

## AI 工具

```rust
// 创建记忆
memorize {
    id: String,                    // 如 "jwt-token-policy"，用作文件名或 DB key
    memory_type: String,           // 自由文本，常用值: "fact" | "decision" | "pattern" | "preference"
    topic: String,                 // 话题，鼓励复用已有 topic
    title: String,                 // 一行摘要，出现在索引中
    content: String,               // 完整内容
    tags: Vec<String>,             // 关键词、文件路径等，用于匹配
    scope: Option<String>,         // 省略 = "shared"，或指定 agent name
    level: Option<String>,         // "project" | "global"，省略则按 memory_type 推断
}

// 按需加载记忆详情
recall_memory {
    id: String,                    // 索引中展示的 id，如 "p:jwt-token-policy" 或 "g:42"
}

// 更新已有记忆
update_memory {
    id: String,
    // 以下均可选，只更新提供的字段
    title: Option<String>,
    content: Option<String>,
    tags: Option<Vec<String>>,
    topic: Option<String>,
    memory_type: Option<String>,
}

// 删除记忆
forget_memory {
    id: String,
}
```

`recall_memory` 调用时自动递增 `access_count`，用于频率加权。

`memorize` 对同一 `id` + 同一 `level` 是**覆盖更新**，和 Notes 的 `write_note` 语义一致。AI 应优先复用已有 id 刷新内容，避免语义重复的记忆条目。

---

## 生命周期管理

### AI 主动管理

与 Notes 一致——March 提供工具，AI 自己判断何时创建、更新、删除。system_prompt 中引导 AI：

- 发现值得长期保留的事实/决策/模式时主动 `memorize`
- 发现已有记忆过时时主动 `update_memory` 或 `forget_memory`
- 用户明确说"记住这个"时立即 `memorize`

### 话题合并提示

同一 topic 下记忆超过 5 条时，March 在 `[memory_index]` 中追加提示：

```
⚠ topic "auth" 下有 8 条记忆，建议合并为更精炼的摘要。
```

AI 可以在合适时机（比如当前轮工作完成后）主动合并：recall 多条 → 综合为一条新记忆 → forget 旧的。

### 过期降权

如果一条记忆连续 N 次出现在索引中但从未被 AI recall（出现但被忽略），降低其 `frequency_score` 的基础分。这避免低质量记忆长期占据索引位置。

具体策略：维护 `skip_count`（出现在索引中但未被 recall 的次数），当 `skip_count > 10` 时，该记忆的综合分数乘以衰减因子 `0.5`。AI 主动 recall 一次则重置 `skip_count`。

项目级记忆的 `skip_count` 存在 frontmatter 中，全局级的存在 DB 的 `skip_count` 列中。

---

## 与现有设计的关系

### 与 Notes 的区别

| | Notes | Memory |
|---|---|---|
| 生命周期 | task 内，task 结束后沉默 | 跨 task、跨 session 持久 |
| 写入方 | AI 在工作中随时写入 | AI 判断值得长期保留时写入 |
| 上下文中的形态 | 完整内容常驻 | 索引常驻，详情按需加载 |
| 用途 | 当前任务的工作记忆 | 项目和用户的长期认知 |

两者互补：Notes 是"这个任务我在干什么"，Memory 是"我对这个项目/用户已经知道什么"。

### 与 Agents 的关系

- 记忆的 scope 复用 agents-teams 的 shared/private 模型
- Agent 切换时，`memory_index` 的 scope 过滤条件自动切换
- Agent 的 `skills_config` 不影响记忆匹配（记忆是知识层，不是能力层）
- Agent 删除时，其名下记忆自动回落到 Shared scope（详见上方"Agent 删除时的记忆归属"）

### 与上下文压力的关系

`memory_index` 的 token 预算（~500 tokens）是固定的，不随上下文压力动态调整。理由：索引本身已经很小，压缩它带来的收益微乎其微；真正的上下文释放应通过 close_file 和 remove_note 完成。

如果上下文压力极高（>95%），March 可以完全跳过 `memory_index` 层注入，优先保障 open_files 和 recent_chat 的空间。

### 项目级记忆与 git

`.march/memories/` 目录下的 `.md` 文件是普通文本文件，自然融入 git 工作流：

- **提交到仓库**：团队成员 clone 项目后，March 自动加载项目级记忆，新人可以立即受益于已积累的项目知识
- **Code Review**：记忆变更出现在 PR diff 中，团队可以审查 AI 记了什么
- **合并冲突**：Markdown frontmatter + 正文的格式对 git merge 友好，即使冲突也容易手动解决
- **`.gitignore` 可选**：如果团队不想共享 AI 记忆，在 `.gitignore` 中加入 `.march/memories/` 即可

项目级记忆本质上是一种**可版本管理的项目知识库**，和 `AGENTS.md` 一样，属于"项目的一部分"而非"工具的私有状态"。
