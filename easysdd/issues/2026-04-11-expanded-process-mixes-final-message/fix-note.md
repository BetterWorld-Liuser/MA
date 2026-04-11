# 展开过程后最终消息与过程消息视觉混在一起 修复记录

> 阶段: 阶段三(修复验证)
> 修复日期: 2026-04-11
> 关联根因分析: [analysis.md](/D:/playground/MA/easysdd/issues/2026-04-11-expanded-process-mixes-final-message/analysis.md:1)

## 1. 实际采用方案

方案 A。

在 [src/components/chat/TurnGroup.vue](/D:/playground/MA/src/components/chat/TurnGroup.vue:42) 中把 turn 显式拆成“过程消息”和“最终消息”两层语义：展开时只展示 `processMessages`，最后一条 `finalMessage` 继续保留在独立“最终消息”区，不再把它并入展开过程列表。

## 2. 改动文件清单

- [src/components/chat/TurnGroup.vue](/D:/playground/MA/src/components/chat/TurnGroup.vue:42) — 调整 assistant turn 的渲染分支：streaming / expanded / collapsed 三种状态分别处理
- [src/components/chat/TurnGroup.vue](/D:/playground/MA/src/components/chat/TurnGroup.vue:112) — 新增 `processMessages` 计算属性，并收紧 `visibleMessages` 的返回逻辑，避免展开态全量返回 `messages`

## 3. 验证结果

- 复现步骤验证: 部分通过。代码路径与渲染分支已按 report 的失败路径修正，但尚未完成桌面界面的人工点击复现
- 期望行为验证: 部分通过。组件逻辑上已满足“展开只显示过程、最终消息保持独立”这一期望；待实际界面验证确认视觉结果
- 影响面回归: `npm run build` 通过，说明模板、类型和打包链路未被本次修改破坏；尚未做界面级冒烟
- 相关测试:
  - `npm run build` ✓

## 4. 遗留事项

- 前端界面的人工交互验证待补：需要在实际桌面界面中按 report.md 的复现步骤点击“X 个动作 ▸”，确认展开态视觉边界符合预期
- 无其他顺手发现
