# 展开过程后最终消息与过程消息视觉混在一起 Issue Report

> 阶段: 阶段一(问题报告)
> 状态: 草稿
> 创建日期: 2026-04-11
> 严重程度: P2

## 1. 问题现象

assistant turn 在收起状态下会把最后一条 assistant message 单独显示为“最终消息”，但当用户展开“过程”后，界面会把整个 turn 的所有 message 一起渲染，并把“收起过程”按钮放在整组消息最底部。

这会造成明显的视觉歧义：

- 用户看到“收起过程”按钮出现在气泡最下面
- 最终回复正文也出现在这段展开区域里
- 整体观感像是“最终回复也属于过程内容”
- 用户难以判断“过程消息”和“最终消息”的边界

从现象上看，更像是前端把“展开过程”实现成了“展开整个 turn”，而不是只展开过程消息。

## 2. 复现步骤

1. 打开聊天界面并进入任意 task
2. 发送一条会触发工具调用的用户消息
3. 等待 assistant 完成该 turn，使卡片进入可折叠状态
4. 点击“X 个动作 ▸”展开过程
5. 观察展开后的 assistant 气泡结构

观察到: 展开后不仅过程消息会显示，最终回复也会和前面的过程消息一起出现在同一个展开区域里，且“收起过程”按钮位于整组内容最底部，视觉上像最终回复也在过程里

复现频率: 稳定复现

## 3. 期望 vs 实际

**期望行为**: 展开“过程”后，过程消息应与最终消息保持明确边界；最终消息要么继续保留在独立的“最终消息”区域，要么在展开态用明显分隔方式单独标识，避免用户误认为最终回复属于过程内容。

**实际行为**: 展开后前端直接渲染整个 turn 的所有 messages，最终消息和过程消息混在同一展示区里，“收起过程”按钮也落在整个气泡最底部，造成“最终回复在过程里”的错觉。

## 4. 环境信息

- 涉及模块/功能: 聊天主界面 - assistant turn 折叠/展开展示
- 相关文件/函数: [src/components/chat/TurnGroup.vue](/D:/playground/MA/src/components/chat/TurnGroup.vue:42), [src/components/chat/TimelineRenderer.vue](/D:/playground/MA/src/components/chat/TimelineRenderer.vue:1), [src/composables/chatRuntime/chatEventReducer.ts](/D:/playground/MA/src/composables/chatRuntime/chatEventReducer.ts:1), [crates/march-core/src/agent/runner.rs](/D:/playground/MA/crates/march-core/src/agent/runner.rs:178), [crates/march-core/src/storage/timeline.rs](/D:/playground/MA/crates/march-core/src/storage/timeline.rs:80)
- 运行环境: dev（本地桌面应用，工作目录 `D:/playground/MA`）
- 其他上下文: 代码检查显示后端 turn 语义上区分了多条 assistant message；当前问题更像前端展开态把整个 turn 全量渲染导致的展示边界错误，而不是后端把最终消息并进过程字段

## 5. 严重程度

**P2** — 核心回复内容仍可见，但过程区与最终消息区的边界被破坏，容易误导用户对系统行为的理解，属于稳定可复现的交互层问题，应在计划内修复。

## 备注

- 该问题与 `2026-04-11-streaming-and-final-message-not-visible` 不同；后者是正文不可见，本问题是正文可见但分区边界错误。
- 当前 report 仅记录现象与可复现行为，不包含修复方案结论。
