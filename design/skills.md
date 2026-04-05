# Skills 加载设计

> Skills 是普通的 Markdown 文件。`[injections]` 只注入一份目录索引，AI 需要时用 `open_file` 读取，watch 系统自然处理后续更新。

---

## 什么是 Skill

Skill 是放在约定目录下的文件夹，文件夹内包含一个 `SKILL.md`，内容是专项指令。例如：

```
/Users/alice/.agent/skills/rust/SKILL.md          — 跨工具共享：Rust 工作流
/Users/alice/.agents/skills/find-skills/SKILL.md  — 兼容现有代理生态：共享技能目录
/Users/alice/.march/skills/api-style/SKILL.md     — March 专用：本项目 API 风格约定
/workspace/demo/.march/skills/deploy/SKILL.md     — 项目级：部署流程
```

`SKILL.md` 不需要特殊格式，纯 Markdown 即可。YAML frontmatter 只用于提供 `name` 和 `description`，供索引展示：

```markdown
---
name: rust
description: Rust 项目工作流：编译错误处理、cargo 命令、代码风格
---

你正在一个 Rust 项目中工作。
...
```

`name` 可省略，省略时用文件夹名作为显示名。`description` 可省略，省略时索引里只展示 name。

文件夹结构也支持附属文件，AI 可以通过 `open_file` 按需读取：

```
/Users/alice/.agent/skills/rust/
├── SKILL.md          — 主指令，AI 先读这个
└── snippets.md       — 附属文件，AI 按需打开
```

---

## 发现路径与优先级

索引和运行时对外暴露的 `path` 一律使用**规范化后的绝对路径**，不使用 `~` 或相对路径，避免因运行账户或当前目录不同而产生歧义。

按以下顺序扫描，优先级从低到高：

```
1. 跨工具共享级  — `<home>/.agent/skills/*/SKILL.md`
2. 跨工具共享级(兼容) — `<home>/.agents/skills/*/SKILL.md`
3. 用户级        — `<home>/.march/skills/*/SKILL.md`
4. 项目级        — `<workdir>/.march/skills/*/SKILL.md`
```

以**文件夹名为 name**，同名时高优先级覆盖低优先级。没有内置 skill。
`~/.agents/skills` 是为兼容现有代理技能目录而增加的别名扫描路径。

---

## 激活方式

扫描到就激活，不需要注册。`config.toml` 只支持禁用：

```toml
# .march/config.toml
[skills]
disable = ["git"]   # 屏蔽该项目下的 git skill
```

另外，Skills 可以根据工作目录根下的标志文件做**自动触发提示**。触发规则由两部分组成：

```
内置规则表
config.toml 里的 [[skills.triggers]] 自定义规则
```

这里的“自动触发”指的是：在 Skills 索引中标记该 skill 与当前工作目录匹配，帮助 AI 在 session 起点更快决定是否应 `open_file` 读取它；**不会自动把 `SKILL.md` 放进 `open_files`**。

规则支持**多对多**：

```toml
[[skills.triggers]]
paths = ["package.json", "tsconfig.json"]
skills = ["node", "typescript", "frontend"]

[[skills.triggers]]
paths = ["pyproject.toml", "requirements.txt"]
skills = ["python"]
```

- 只要 `paths` 中任一文件存在，就会为该规则里的所有 `skills` 打上 auto trigger 标记
- 同一个 skill 可以同时被多条规则命中，最终在索引里合并显示触发原因

MVP 内置规则表示例：

```text
Cargo.toml                               -> rust
package.json                             -> node, javascript, typescript
tsconfig.json                            -> typescript
pyproject.toml / requirements.txt        -> python
go.mod                                   -> go
Gemfile                                  -> ruby
Dockerfile / docker-compose.yml / compose.yaml -> docker
```

---

## 与上下文的集成

Skills **不把内容塞进 `[injections]`**，而是在 session 启动时生成一条索引注入：

```
可用 Skills：
- rust  (/Users/alice/.agent/skills/rust/SKILL.md)：Rust 项目工作流
- rust  (/Users/alice/.agent/skills/rust/SKILL.md)：Rust 项目工作流 [auto: detected Cargo.toml]
- git   (/Users/alice/.agent/skills/git/SKILL.md)：Git 提交规范
- api-style  (/workspace/demo/.march/skills/api-style/SKILL.md)：本项目 API 风格约定

需要某个 skill 的详细内容时，用 open_file 打开对应路径。
```

AI 决定何时需要某个 skill，主动 `open_file` 读取，watcher 从此追踪该文件，内容永远反映磁盘真实状态。这与普通源码文件的使用方式完全一致。

---

## 加载流程

```
session 启动
    │
    ├─ 按优先级顺序扫描四个目录：
    │   <home>/.agent/skills/*/SKILL.md
    │   <home>/.agents/skills/*/SKILL.md
    │   <home>/.march/skills/*/SKILL.md
    │   <workdir>/.march/skills/*/SKILL.md
    │   └─ 同名时高优先级覆盖低优先级
    │
    ├─ 解析各 SKILL.md 的 YAML frontmatter，提取 name / description
    │
    ├─ 过滤 config.disable
    │
    ├─ 根据内置规则 + config 里的 skills.triggers
    │   为命中的 skill 打上 auto trigger 标记
    │
    └─ 生成包含绝对路径的索引文本，作为单条 Injection 注入 AgentContext.injections
```

---

## 数据结构

```rust
struct SkillEntry {
    name: String,         // frontmatter 里的 name，省略时用文件夹名
    path: PathBuf,        // SKILL.md 的路径，供 AI open_file 使用
    description: String,  // frontmatter 里的 description，可为空
    auto_triggered: bool,
    trigger_reason: Option<String>,
}

struct SkillLoader {
    work_dir: PathBuf,
    home_dir: PathBuf,
}

impl SkillLoader {
    fn load(&self, config: &MarchConfig) -> Result<Vec<SkillEntry>>;

    fn to_injection(entries: &[SkillEntry]) -> Injection {
        // 生成索引文本，构造单条 Injection { id: "skills", content: ... }
    }
}
```

`Injection` 结构：

```rust
struct Injection {
    id: String,       // "skills"
    content: String,  // 索引文本，session 内固定
}
```

---

## 右侧面板展示

右侧面板展示当前可用的 skill 列表，已被 AI open 的标注打开状态，默认只读：

```
  [Skills]
  rust         (已打开)
  git
  api-style
```

操作权在 AI，用户若想影响 skill 使用，通过聊天消息告知即可。详见 → [ui-chat.md](ui-chat.md)
`available` 一侧提供刷新按钮，手动重新扫描技能目录并更新当前 task 的可用 skill 视图；刷新后，该 task 后续轮次构建上下文时使用的 skills injection 会同步采用新的扫描结果。
