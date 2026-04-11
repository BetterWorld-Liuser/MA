---
name: easysdd-issue
description: easysdd-issue 是 easysdd 家族里专门处理问题修复的子工作流入口。当用户说"发现了一个 bug"、"有个问题想提"、"提个 issue"、"记录一个问题"、"分析这个 bug"、"帮我修这个 bug"、"这里有个问题"、"某功能不对"等意图时,务必触发本技能。本技能负责:介绍 issue 工作流、检查已有产物、把用户路由到正确的子技能(report / analyze / fix)。如果用户已经明确说"开始写 report / 做分析 / 开始修"等具体阶段意图,优先触发对应的子技能而不是本技能。
---

# easysdd-issue

**easysdd 家族的问题修复子工作流**——处理项目里发现的 BUG、异常行为、文档错误等各类问题,从清晰记录到根因分析再到验证修复,全程有 spec 存档。

本技能是路由中心,不替代子技能干活。

---

## 一、为什么需要 issue 工作流

直接跳进代码"找到哪里错了就改",典型失败模式:

1. **问题描述在脑子里,改完就消失**:三个月后同样的 bug 再现,没有任何复现步骤留存
2. **根因没分析就动手**:改了表面现象,深层问题还在,等待下次爆发
3. **修复范围扩散**:发现一个 bug 顺手改了五处,引入新问题,无法追溯
4. **没有验收闭环**:怎么判断改好了?改好了什么?没有记录

easysdd-issue 在"发现问题"和"动手改代码"之间加缓冲层:

```
发现问题 → 清晰记录(report) → 根因分析(analyze) → 定点修复 + 验证(fix)
```

---

## 二、目录安排

本节是 easysdd-issue 子工作流目录约定的唯一定义处。

### issue 目录位置

`easysdd/issues/` 下,每个 issue 一个子目录 `{issue}/`,里面住着该 issue 的所有 spec 产物:

```
easysdd/
└── issues/                     ← issue 聚合根
    └── {issue}/                ← issue 目录
        ├── report.md           ← 问题报告(Stage 1)
        ├── analysis.md         ← 根因分析(Stage 2)
        └── fix-note.md         ← 修复记录(Stage 3,必出产物)
```

**issue 目录命名格式**:`YYYY-MM-DD-{英文 slug}`

- **日期前缀**:取发现/提报问题当天的日期,一经确定不变
- **英文 slug**:小写字母 + 数字 + 连字符,简短且能一眼看出是什么问题(如 `auth-token-leak`、`null-pointer-on-empty-list`)
- 两部分用连字符 `-` 连接

> `{issue}` 是占位符,代表具体 issue 目录名。正文用自然语言术语(问题报告、根因分析、修复备注、issue 目录),字面路径看上面目录树。

### 组织规则

1. **一个 issue = 一个 issue 目录**。同一问题的 report / analysis / fix-note 永远聚合在一起
2. **fix-note.md 是阶段三的必出产物**。无论修复简单还是复杂,都要写 fix-note.md 记录实际采用方案、改动清单、验证结果和遗留事项。修复记录不再回填到 analysis.md
3. **issue 目录不要和 feature 目录混**:`easysdd/issues/` 和 `easysdd/features/` 是并列的,不允许交叉

---

## 三、三个阶段

| 阶段 | 子技能 | 主导者 | 产出 |
|---|---|---|---|
| ① 问题报告 | `easysdd-issue-report` | 用户描述,AI 引导结构化 | 问题报告(`report.md`) |
| ② 根因分析 | `easysdd-issue-analyze` | AI 读代码分析,用户确认 | 根因分析(`analysis.md`) |
| ③ 修复验证 | `easysdd-issue-fix` | AI 按分析定点修复,用户验证 | 代码修复 + 修复记录(`fix-note.md`) |

**阶段之间有硬 checkpoint**:每个阶段退出条件未满足,下一阶段不得开始。用户没明确放行,AI 不自作主张往下走。

---

## 四、路由:用户该用哪个子技能

启动本技能后,先 Glob 检查 `easysdd/issues/` 下有没有相关的 issue 目录,**不要只听用户口头描述**。

| 当前状态 | 触发哪个子技能 |
|---|---|
| 刚发现问题,还没有任何文件 | `easysdd-issue-report` |
| `report.md` 已存在,没有 `analysis.md` | `easysdd-issue-analyze` |
| `analysis.md` 已存在,代码还没改 | `easysdd-issue-fix` |
| 代码已改,还没有修复验证记录 | `easysdd-issue-fix`(走验证环节) |
| 不确定 | 自己读已有文件,按上表对号入座 |

**如果用户描述的是新功能需求而不是 BUG**:告诉用户走 `easysdd-feature` 工作流,本工作流不适用。

---

## 五、与 easysdd-feature 工作流的边界

- **issue 工作流处理**:已有代码里的 BUG、异常行为、文档错误、性能问题——即"本来应该好的东西坏了"
- **feature 工作流处理**:新功能、新能力——即"从来没有的东西要加进来"
- **灰色地带判断**:如果修复 issue 的过程中发现需要新增能力才能真正解决问题,**先用 issue 工作流把问题记录和分析做完,再视情况开 feature 工作流**——不要在 issue 里偷偷做新功能

---

## 六、相关文档

- `easysdd/SKILL.md` — easysdd 家族根技能,跨阶段共同约束在那里
- `AGENTS.md` — 全项目代码规范,issue 修复时同样遵守
- 架构总入口 — 做根因分析时可能需要查
