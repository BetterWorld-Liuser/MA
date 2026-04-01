# Skills 加载设计

> Skills 是普通的 Markdown 文件。`[injections]` 只注入一份目录索引，AI 需要时用 `open_file` 读取，watch 系统自然处理后续更新。

---

## 什么是 Skill

Skill 是放在约定目录下的 `.md` 文件，内容是专项指令。例如：

```
~/.ma/skills/git.md          — Git 提交规范
~/.ma/skills/rust.md         — Rust 工作流
.ma/skills/api-style.md      — 本项目 API 风格约定
```

Skill 不需要特殊格式，纯 Markdown 即可。frontmatter 只用于提供 `description`，供索引展示：

```markdown
---
description = "Rust 项目工作流：编译错误处理、cargo 命令、代码风格"
---

你正在一个 Rust 项目中工作。
...
```

`description` 可省略，省略时索引里只展示文件名。

---

## 发现路径与优先级

```
1. 用户级  — ~/.ma/skills/*.md
2. 项目级  — .ma/skills/*.md
```

以**文件名（不含扩展名）为 name**，同名时项目级覆盖用户级。没有内置 skill。

---

## 激活方式

扫描到就激活，不需要注册。`config.toml` 只支持禁用：

```toml
# .ma/config.toml
[skills]
disable = ["git"]   # 屏蔽该项目下的 git skill
```

---

## 与上下文的集成

Skills **不把内容塞进 `[injections]`**，而是在 session 启动时生成一条索引注入：

```
可用 Skills：
- rust  (~/.ma/skills/rust.md)：Rust 项目工作流
- git   (~/.ma/skills/git.md)：Git 提交规范
- api-style  (.ma/skills/api-style.md)：本项目 API 风格约定

需要某个 skill 的详细内容时，用 open_file 打开对应路径。
```

AI 决定何时需要某个 skill，主动 `open_file` 读取，watcher 从此追踪该文件，内容永远反映磁盘真实状态。这与普通源码文件的使用方式完全一致。

---

## 加载流程

```
session 启动
    │
    ├─ 扫描 ~/.ma/skills/ 和 .ma/skills/
    │   └─ 同名时项目级覆盖用户级
    │
    ├─ 过滤 config.disable
    │
    └─ 生成索引文本，作为单条 Injection 注入 AgentContext.injections
```

---

## 数据结构

```rust
struct SkillEntry {
    name: String,         // 文件名去扩展名，作为唯一标识
    path: PathBuf,        // 供 AI open_file 使用
    description: String,  // frontmatter 里的 description，可为空
}

struct SkillLoader {
    work_dir: PathBuf,
    home_dir: PathBuf,
}

impl SkillLoader {
    fn load(&self, config: &MaConfig) -> Result<Vec<SkillEntry>>;

    fn to_injection(entries: &[SkillEntry]) -> Injection {
        // 生成索引文本，构造单条 Injection
    }
}
```

`Injection` 结构回归原始定义，无需 `path` 字段：

```rust
struct Injection {
    id: String,       // "skills"
    content: String,  // 索引文本，session 内固定
}
```

---

## TUI 展示

右侧面板展示当前可用的 skill 列表，已被 AI open 的标注打开状态：

```
  [Skills]
  rust         (已打开)
  git
  api-style
```

用户可手动触发 open/close，效果等同于在聊天框里告诉 AI。
