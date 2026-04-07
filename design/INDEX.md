# March — 设计文档索引

| 文档 | 内容 |
|------|------|
| [DESIGN.md](DESIGN.md) | 项目目标、核心理念、架构总览、技术栈、开发顺序 |
| [ui-task-naming.md](ui-task-naming.md) | 新建 task 的延迟命名与首轮自动命名规则 |
| [context.md](context.md) | 上下文分层、数据结构、prefix cache、watcher 边界情况、任务切换、上下文压力管理 |
| [agents.md](agents.md) | `AGENTS.md` 规则文件接入方式、session 初始化自动加载、默认锁定语义 |
| [agents-teams.md](agents-teams.md) | 多角色系统、共享/私有上下文分层、@mention 机制、角色创建与管理 |
| [memory.md](memory.md) | 记忆系统、FTS5 + jieba 召回机制、二层索引、scope 规则、生命周期管理 |
| [subconscious.md](subconscious.md) | 后台辅助进程、prefix cache 复用、AI+程序混合执行、权限模型、触发与编排 |
| [tools.md](tools.md) | 工具分层、用户可见输出、run_command、文件工具、行号编辑、错误处理 |
| [config.md](config.md) | UI 配置（provider）与文本配置（config.toml）的职责划分 |
| [skills.md](skills.md) | skill 文件格式、发现路径、加载流程、与上下文的集成 |
| [ui.md](ui.md) | UI 总览、设计原则、子文档边界 |
| [ui-shell.md](ui-shell.md) | 应用壳层、视觉风格、三栏布局、任务列表、上下文面板、设置页 |
| [ui-chat.md](ui-chat.md) | 聊天区、等待态、工具调用展示、输入框交互、运行反馈 |
| [ui-events.md](ui-events.md) | 聊天运行事件模型、事件字段、状态机、前端聚合方式 |
| [provider.md](provider.md) | genai 选型理由、与上下文管理的分工、cache_control 映射 |
| [reasoning.md](reasoning.md) | Reasoning 模型支持：各家 wire format 归一化、thinking block 透传规则、task 级运行参数、UI 展示 |
| [browser.md](browser.md) | 截图策略、CDP 浏览器操作、桌面 GUI 操作 |
| [march-md.md](march-md.md) | 自研 Markdown 渲染引擎：Sealed Prefix + Live Tail、Block Parser 状态机、Lenient Inline 解析 |
| [turn-snapshot.md](turn-snapshot.md) | Turn 快照与回退：git stash create 快照、文件 diff 展示、一键撤销此轮 |
