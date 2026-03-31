# Ma — 项目规则

## 设计中心

**`design/DESIGN.md` 是整个项目的设计权威文档。**

- 所有架构决策、模块划分、数据结构设计，必须能从 `design/DESIGN.md` 的核心理念中追溯出来
- 子系统设计（file watcher、context builder、TUI 等）是 `design/DESIGN.md` 思路的延伸，不是独立存在的
- 新增功能或改动方向前，先对照 `design/DESIGN.md`，确认与核心设计一致；若有冲突，先更新 `design/DESIGN.md`，再动代码
- 子系统若有独立设计文档，放在 `design/` 目录下，并在 `design/DESIGN.md` 中引用
- 不要在代码注释或其他文档里另起炉灶地描述架构——设计讨论的落点是 `design/`
- 代码实现应保持较丰富的注释，优先解释设计意图、关键流程和不直观的实现细节。
