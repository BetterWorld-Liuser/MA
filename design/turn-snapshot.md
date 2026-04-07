# Turn 快照与回退

> 从 [DESIGN.md](DESIGN.md) 延伸：每轮 AI 回复落盘前保存快照，回复结束后展示文件变更列表，支持一键回退。

---

## 核心问题

March 的 Source of Truth 设计让 AI 永远面对磁盘真实状态，但这同时意味着 AI 改完文件之后没有原生的撤销路径。用户想看"这一轮 AI 到底改了什么"，或者对某个修改不满意想回滚，目前没有手段。

本文档描述如何在不破坏现有 watcher / open_files 机制的前提下，为每轮操作引入快照与回退能力。

---

## 快照策略

### 主路径：Git Plumbing

当工作目录是 git 仓库时，在 turn 开始前执行：

```bash
git stash create
```

`git stash create` 把当前 working tree 状态打包成一个 stash commit 对象，返回该对象的 hash。它**不修改工作区**，不影响分支历史，也不污染 `git stash list`（stash 对象只存在于 git 对象数据库，未被 `refs/stash` 引用）。

将该 hash 存入数据库，作为本轮快照的引用。

turn 结束后，对发生过 `ModifiedBy::Agent` 的文件执行：

```bash
git diff <stash_hash>^3 -- <path>
```

（`^3` 指向 stash 的 working tree parent，即快照时的磁盘状态。）

回退时：

```bash
git checkout <stash_hash>^3 -- <path1> <path2> ...
```

**优点**：
- git 对象存储去重压缩，零额外磁盘占用
- diff 算法成熟，自动尊重 `.gitignore`
- 回退精确，可靠
- 用户若需要高级恢复，可以直接操作 stash hash

### 降级路径：文件内容快照（无 git）

当工作目录不是 git 仓库时：

- turn 开始时，对 `open_files` 中所有已追踪文件的当前内容做内存快照（`HashMap<PathBuf, String>`）
- turn 期间，watcher 实时标记 `ModifiedBy::Agent` 的文件
- turn 结束后，对每个被修改文件，用内存快照内容与当前 `FileSnapshot` 做 diff
- diff 文本持久化入 DB

回退时，将 DB 中存的快照内容直接写回磁盘，watcher 感知变化，下一轮上下文自动刷新。

**覆盖范围说明**：此路径仅能追踪 turn 开始时已在 `open_files` 中的文件，以及 turn 期间新创建并被 watcher 报告的文件。通过 `run_command` 间接修改、且不在 `open_files` 中的文件可能漏追踪。有 git 时不存在此问题，建议优先引导用户在 git 仓库中使用。

---

## 数据结构

### 新增 DB 表

```sql
-- 每轮的快照引用
CREATE TABLE turn_snapshots (
    turn_id      INTEGER PRIMARY KEY REFERENCES conversation_turns(id),
    task_id      INTEGER NOT NULL,
    git_stash    TEXT,    -- git stash create 返回的 hash；null 表示使用降级路径
    created_at   INTEGER NOT NULL
);

-- 每轮实际变更的文件及其 diff
CREATE TABLE turn_changed_files (
    turn_id      INTEGER NOT NULL REFERENCES turn_snapshots(turn_id),
    path         TEXT    NOT NULL,
    diff_unified TEXT    NOT NULL,  -- unified diff 格式，turn 结束后计算并持久化
    PRIMARY KEY (turn_id, path)
);
```

### `conversation_turns` 扩展

不需要修改原表，`turn_changed_files` 通过 `turn_id` 外键关联，查询时 join 即可。

---

## 触发时机

| 时机 | 动作 |
|------|------|
| turn 开始前 | 执行 `git stash create`，存 hash；或做内存快照（降级路径） |
| turn 执行中 | watcher 正常标记 `ModifiedBy::Agent`，无额外操作 |
| turn 自然结束（agent loop 收敛）后 | 对所有 `ModifiedBy::Agent` 文件计算 diff，写入 `turn_changed_files` |
| 无文件修改的 turn | 不创建 `turn_snapshots` 记录，不显示 diff 区域 |

---

## UI 展示

turn 结束、有文件变更时，在聊天区 assistant 气泡下方追加一个折叠区域：

```
▶ 3 个文件已修改  [↩ 撤销此轮]
  ├ src/auth.rs        +12  −3
  ├ Cargo.toml          +1  −0
  └ src/lib.rs          +5  −8
```

- 点击文件名展开 inline diff（unified diff 风格，新增行绿底，删除行红底）
- "撤销此轮"按钮触发回退流程（见下节）
- 此区域作为 `DisplayTurn` 的附属数据，与 `tool_summaries` 并列渲染，重新打开应用后仍可查看历史变更

历史 turn 的 diff 区域只读，"撤销"按钮只在**最近一轮有变更的 turn** 上显示，避免多轮交叉回退引发混乱。

---

## 回退流程

用户点击"撤销此轮"：

1. 从 `turn_snapshots` 读取本轮的 `git_stash` hash（或降级路径下的文件快照）
2. 对 `turn_changed_files` 中列出的每个文件执行还原
   - git 路径：`git checkout <stash_hash>^3 -- <files>`
   - 降级路径：直接将 DB 中存储的原始内容写回磁盘
3. watcher 感知文件变化，更新 `FileSnapshot`
4. 若相关文件在当前 task 的 `open_files` 中，下一轮上下文自动反映回退后状态
5. 清理本轮 `turn_snapshots` 和 `turn_changed_files` 记录（快照已失效）
6. 若使用 git 路径，stash 对象可在下一次 `git gc` 时自然回收，无需手动清理

---

## Stash 对象生命周期

`git stash create` 生成的对象未被任何 ref 引用，会在 `git gc` 时作为 unreachable object 被回收（默认宽限期 2 周）。

March 不需要手动管理这些对象：
- 回退后 stash 已消费，让 gc 回收即可
- 如果用户希望保留某个快照，可以自己 `git tag march/snap/<turn_id> <hash>` 固定引用

---

## 与现有设计的关系

- **不侵入 watcher 逻辑**：快照通过 `ModifiedBy::Agent` 归因接入，不增加新的 watcher 事件类型
- **不影响 open_files Source of Truth**：回退写回磁盘后，watcher 自然感知并刷新 `FileSnapshot`，上下文管理机制完全不需要感知"回退"这件事
- **不影响 recent_chat**：回退只操作文件，不修改对话历史。用户和 AI 之间的对话记录不受影响，下一轮 AI 可以感知到文件已回退（open_files 内容变了）
- **与上下文压力无关**：`turn_changed_files` 存储在 DB 中，不进入 AI 上下文，不占用 token
