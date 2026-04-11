# 流式过程消息与最终消息均不显示 Issue Report

> 阶段: 阶段一(问题报告)
> 状态: 草稿
> 创建日期: 2026-04-11
> 严重程度: P1

## 1. 问题现象

聊天区里 assistant turn 的卡片能够创建出来，也能看到顶部 agent 信息，以及折叠摘要中的“1 个动作 ▸ / 最终消息”等结构性文案，但真正的消息内容没有显示出来：

- 流式进行过程中，看不到 assistant 正在输出的正文
- turn 结束后，最终消息区域仍然是空的
- 用户最终只能看到过程摘要或工具动作外壳，看不到 AI 的实际回复文本

从截图看，该问题发生时聊天区中央已经渲染出一个 assistant turn 卡片，但卡片内部只有摘要结构，没有正文内容。

## 2. 复现步骤

1. 打开聊天界面并进入任意 task
2. 发送一条普通用户消息，触发 agent 执行
3. 等待 agent 在过程中产生活动（如工具动作或最终回复）
4. 观察聊天区中该 assistant turn 的消息内容区域

观察到: 流式过程中没有正文显示，turn 结束后最终消息也没有显示，只剩下“X 个动作 ▸ / 最终消息”等外壳结构

复现频率: 稳定复现（按当前用户描述与截图，当前问题已明确出现）

## 3. 期望 vs 实际

**期望行为**: agent 在流式输出时，聊天气泡内应持续显示正在生成的消息内容；turn 结束后，最终消息应完整显示在“最终消息”区域。

**实际行为**: assistant turn 的结构能出现，但流式正文和最终消息正文都没有显示出来，用户只能看到摘要/外壳，看不到实际回复文本。

## 4. 环境信息

- 涉及模块/功能: 聊天主界面 - assistant turn 的时间线渲染与最终消息展示
- 相关文件/函数: `src/components/chat/TurnGroup.vue`、`src/components/chat/TimelineRenderer.vue`、`src/composables/chatRuntime/chatEventReducer.ts`、`src/composables/workspaceApp/useTaskTimelineState.ts`、`crates/march-core/src/ui/backend/messaging.rs`、`crates/march-core/src/agent/runner.rs`
- 运行环境: dev（基于本地仓库 `D:/playground/MA` 的桌面应用界面）
- 其他上下文: 用户提供了问题截图；截图中可见 assistant turn 卡片已经出现，但正文区域为空，只显示折叠摘要与“最终消息”标签

## 5. 严重程度

**P1** — 聊天是核心功能，assistant 回复正文在流式阶段和结束后都不可见，用户无法正常阅读结果；虽然界面结构与动作摘要仍可见，但核心输出已严重受损。

## 备注

- 当前 report 仅记录现象与可复现行为，不包含根因判断。
- 仓库中已有相近但不同的问题记录 `2026-04-11-streaming-chat-bubble-flicker`，那次是“文字闪烁”；本次是“正文完全不显示”，不属于同一现象。
