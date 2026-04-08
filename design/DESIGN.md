# March — 设计文档

> **March** — 三月诞生，向前进军，码农的伙伴。命令行直接 `march "帮我重构这个函数"`。

## 项目目标

构建一个以**上下文管理**为核心竞争力的 agentic coding 工具，用 Rust 实现。

---

## 核心理念：Conversations that never decay

其他 coding agent 的对话是一条不断变长的纸带——每次读文件、改文件，原始内容和操作记录全部堆进消息历史。改 10 个文件就有 10 份过期快照永远留在上下文里，即使文件早已面目全非。对话越长，AI 越迟钝，直到上下文塞满，只能截断或重开 session。

**March 的对话永远不退化。** 上下文每轮重新构建，AI 永远只看到磁盘的真实状态，不是历史快照的堆叠。对话长度不影响质量，不需要 compaction，不需要重开 session。一个主题就是一个对话框，用户改需求、转方向、反复来回，都在同一个窗口里进行。

这通过两个设计实现：

1. **文件系统作为 Source of Truth**：用 file watcher 监控工作目录，无论是 AI 改的、用户手动改的、还是其他程序改的，上下文里的文件内容永远反映磁盘真实状态
2. **上下文每轮重建，不线性累积**：大小由当前状态决定，而不是随对话轮次增长

具体而言，March 的上下文由以下部分组成：

- `open_files`：只有当前文件的真实内容，不是历史快照的堆叠
- `AGENTS.md`：若工作目录存在，则在 session 初始化时自动加入 `open_files`，并默认锁定，作为项目级工作规则随文件真实内容一起进入上下文
- `session_status` / `runtime_status`：环境信息与运行时状态分层注入，不混入聊天和文件内容
- `recent_chat`：只保留最近 N 轮人类↔AI 对话（当前默认 10 轮），并附带每条消息时间，旧的自动丢弃
- 工具执行结果：在同一轮 agent loop 内持续可见，直到本轮不再产生新的 tool calls、自然收敛后才整体丢弃；若要跨轮保留，需主动写入 Notes
- `notes`：AI 主动管理，随时 remove 不再需要的条目；`write_note(id, content)` 对同一 `id` 是覆盖更新，不是追加新条目，因此 AI 应优先复用已有 note id 来刷新事实、目标和阶段状态，避免留下多条语义重叠的笔记

**一个主题就是一个对话框。** 用户改需求、转方向、反复来回，都在同一个窗口里进行，March 自己管理 AI 侧的信息密度，用户不需要感知上下文的存在。

### 推论：工作目录应是任务级上下文，而不是应用级全局状态

“工作目录”直接决定了 watcher 监控范围、`AGENTS.md` 自动接入、Skills 自动发现、工具默认 `cwd`、以及 AI 对项目结构的初始认知，因此它属于 **task/session 的运行上下文**，不应被固定为“软件启动目录”。

- 每个 task 持久化自己的 `working_directory`
- 新建 task 默认继承当前 workspace 根目录，用户可在聊天输入区显式改成其他目录
- task 切换时，AI 运行目录、右侧上下文状态、`@` 文件/目录搜索范围都应随该 task 的 `working_directory` 一起切换
- “恢复默认”语义不是清空为未知值，而是回到当前 workspace 根目录
- 同一 task 的 agent 轮次保持串行执行，但用户侧输入框不因当前轮运行而锁死；用户可以在 AI 回复过程中继续编辑下一条消息，真正发送时再基于当下最新的 chat 快照与 task 运行上下文创建新一轮

这样做的原因是：

- 工作目录和模型选择一样，都是“这个任务要在什么环境里工作”的任务级决策
- 同一应用窗口里，用户可能并行处理多个项目或同一仓库下的不同子目录
- 只有把工作目录下沉到 task，Source of Truth 才能稳定落在“当前 task 实际面对的磁盘范围”上

---

## 架构设计

### 双轨设计：用户视图 vs AI 上下文分离

```
用户看到的                    AI 收到的
─────────────────            ─────────────────
完整聊天记录                  精简后的上下文
所有历史对话                  只有近几轮对话
工具调用摘要                  当前文件真实快照
```

用户永远看到完整对话，AI 收到的是精简过的上下文。

详见 → [上下文管理](context.md)

---

## 项目规则文件

兼容 `AGENTS.md` 约定，但接入方式遵循 March 自己的 Source of Truth 设计：如果工作目录存在 `AGENTS.md`，就在 session 初始化时自动把它加入 `open_files`，并默认设为 locked。

这样做的原因是：

- `AGENTS.md` 本质上也是文件，应和其他上下文文件一样由 watcher 提供真实快照
- 它属于项目级工作规则，应该默认稳定存在于上下文中，不依赖 AI 主动 `open_file`
- 通过 locked 避免 AI 在清理上下文时误 `close_file` 掉规则文件

详见 → [AGENTS.md 接入](agents.md)

---

## 工具哲学

AI 以命令执行为主，但不能只有命令行；基础文件读写能力应作为一级工具存在，文件修改仍通过行号精确定位。

- `run_command`：负责调用编译、测试、git、grep、脚本等环境能力
  并且允许显式指定命令执行环境，例如 `bash` / `powershell` / `cmd`
  March 会先扫描当前环境中真实可用的 shell，并把可选项注入给 AI
- `read_file` / `write_file`：负责最基础、最稳定的文件读取与落盘
- 行号级编辑能力：负责精确修改文件片段，避免基于文本匹配的脆弱替换
- LSP 工具集：提供语义层查询能力（hover 类型、go-to-definition、find-references、code actions），按需调用，结果在轮内可见

这样设计的原因不是回到”高层魔法工具”，而是明确区分两类能力：

- 命令执行是和外部环境交互
- 文件工具是和 March 自己维护的 Source of Truth 直接交互

前者保留通用性，后者保证基础文件操作简单、稳定、可控。LSP 工具是第三类：语义层查询，补充文件系统 Source of Truth 无法直接提供的类型和引用信息。

详见 → [工具设计](tools.md)、[LSP 集成](lsp.md)

---

## Skills

补充 AI 在特定领域的"怎么做"，注入到 `[injections]` 层，session 内固定不变。
支持按工作目录文件自动触发（`Cargo.toml` → rust skill）或显式配置启用/禁用。

详见 → [Skills 设计](skills.md)

---

## Agents / Teams

Agent 是人格配置，不是独立进程。用户和 AI 都可以创建角色，通过 `@角色名` 在聊天中召唤。角色切换时复用 March 的上下文分层架构：共享层（用户放上桌面的文件、笔记）即时继承，私有层（角色自己 open 的文件、write 的笔记）互不干扰。March 本身也是一个普通角色。

详见 → [Agents / Teams 设计](agents-teams.md)

---

## 记忆系统

跨 session、跨 task 的持久记忆。AI 在长期使用中积累对项目和用户的认知（事实、决策、模式、偏好）。项目级记忆存为 `.march/memories/*.md` 文件，随项目走、可 git 管理；全局级记忆存在用户 DB 中，跨项目持久。对 AI 和 UI 暴露的记忆 id 一律使用稳定字符串 id；SQLite 的数字主键只作为全局记忆的存储层内部实现细节，不进入 prompt、工具调用或界面展示。不依赖 RAG，用 SQLite FTS5 + jieba 分词做全文检索，结合路径前缀匹配和时间/频率加权实现召回。二层召回设计：匹配后的索引摘要常驻上下文（~500 tokens），详情由 AI 按需 recall。

详见 → [记忆系统](memory.md)

---

## Subconscious

后台运行的辅助进程，在主 agent 工作间隙自动执行记忆整理、模式识别等任务。与 Agent（前台人格）互补，Subconscious 是 March 的后台认知层。复用主 agent 的完整上下文前缀（最大化 prefix cache 命中），追加主 agent 的轮内历史和自己的任务说明。权限写死：只能操作 Memory 和 Hints，不能修改文件、执行命令或侵入主 agent 的工作状态。支持 AI + 程序混合执行——程序阶段做条件判断（零 LLM 成本），条件满足时才进入 AI 阶段。

详见 → [Subconscious](subconscious.md)

---

## Turn 快照与回退

每轮 AI 回复开始前自动保存快照（有 git 时用 `git stash create`，无 git 时快照 open_files 内容），回复结束后在聊天区展示文件变更列表与 inline diff，支持一键撤销本轮所有文件修改。

详见 → [Turn 快照与回退](turn-snapshot.md)

---

## 图片输入

图片作为消息级内容流转，不进入 `open_files` 文件追踪通道。用户粘贴、`@` 引用图片文件、AI 调用 `view_image` 工具，最终都归结为 API 的 image content block，随 `recent_chat` 或轮内历史自然管理。

详见 → [图片输入](images.md)

---

## 浏览器与电脑操作

文本优先，截图按需，通过 CDP 操作用户真实浏览器环境。

详见 → [浏览器与电脑操作](browser.md)

---

## Reasoning 模型支持

各家 reasoning 格式（Anthropic thinking block、OpenAI reasoning_effort、DeepSeek-R1 inline tag、Gemini thought block）在 WireAdapter 层归一化，上层统一消费。Anthropic thinking block 的历史透传约束与 March 整条截断策略天然兼容。reasoning 作为 task 级运行参数，在输入框运行参数入口暴露。

详见 → [Reasoning 模型支持](reasoning.md)

---

## 技术栈

| 用途 | 库 |
|------|----|
| 文件 watcher | `notify` |
| async 运行时 | `tokio` |
| UI 壳 | `tauri` |
| 前端框架 | Vue 3 |
| 前端样式 | Tailwind CSS + CSS Variables |
| LLM provider 抽象 | `genai` |
| 中文分词 | `jieba-rs` |
| JSON 处理 | `serde_json` |

详见 → [Provider 设计](provider.md)

---

## UI 形态

三栏布局：左侧任务列表、中间聊天区、右侧上下文面板。右侧面板是 March 独有的差异化——把 AI 的 Notes、监控文件、上下文用量变成用户可见、可直接操作的界面元素。

设置页也按职责拆分信息结构：外观、模型、供应商、角色各自成区。应用级默认运行不单列为独立入口，而是作为模型实体的一个全局标记，在“模型”页直接设置，避免再引入第二套运行心智模型。

详见 → [UI 设计总览](ui.md)、[应用壳层与布局](ui-shell.md)、[聊天区与运行反馈](ui-chat.md)、[聊天运行事件模型](ui-events.md)、[任务自动命名](ui-task-naming.md)

---

## 国际化

轻量级自管理方案，不依赖 vue-i18n。通过全局 composable 提供 `t()` 函数和 locale 切换，语言文件用 TypeScript 嵌套对象，持久化到 localStorage，即时生效。初期覆盖中文和英文。

详见 → [国际化设计](i18n.md)

---

## 开发顺序

1. **上下文管理数据结构**（当前阶段）
2. File watcher 模块
3. 最小 agent 循环（读文件 + 调 API + 写文件）
4. Tauri UI
