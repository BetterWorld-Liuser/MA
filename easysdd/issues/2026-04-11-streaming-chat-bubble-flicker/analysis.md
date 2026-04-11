# 流式输出时聊天气泡文字闪烁 根因分析

> 阶段:阶段二（根因分析）
> 状态:已确认（采用方案 A）
> 分析日期:2026-04-11
> 关联问题报告:report.md

## 1. 问题定位

| 关键位置 | 说明 |
|---|---|
| `src/components/chat/TimelineRenderer.vue:3` | `v-for` 的 `:key` 对 text 类型 entry 使用 `` `text:${entry.text}` `` —— 文字内容即 key |
| `src/composables/chatRuntime/chatEventReducer.ts:271-288` | `appendMessageTextDelta`：每次 delta 到达都创建一个新的 text 对象，`text` 字段不断增长 |
| `src/data/mock.ts:32-35` | `AssistantTimelineTextEntry` 类型只有 `kind` 和 `text` 两个字段，没有稳定 ID |

## 2. 失败路径还原

**正常路径（期望）**：
后端推送 delta → `appendMessageTextDelta` 在 timeline 最后一个 text entry 上追加字符 → Vue 检测到 `entry.text` 变化 → `MarkdownRender` 接收到新 `content` prop → 组件内部增量更新渲染 → 文字稳定追加显示

**失败路径（实际）**：
后端推送 delta → `appendMessageTextDelta` 创建新 text 对象 `{ kind: 'text', text: '原文+delta' }` → 新对象的 `text` 已变长 → `TimelineRenderer` 中 key 从 `text:原文` 变为 `text:原文+delta` → **Vue 判定这是一个新节点，销毁旧的 `MarkdownRender` 实例，挂载全新实例** → 挂载过程导致视觉上的闪烁 → 下一个 delta 到达，同样的过程重复

**分叉点**：`src/components/chat/TimelineRenderer.vue:3` — key 策略把"不断变化的内容"当作"节点身份标识"，两者生命周期完全不同，导致每次内容更新都被 Vue 误判为节点替换

## 3. 根因

**根因类型**：逻辑错误（key 策略错误）

**根因描述**：
Vue 的 `v-for` `:key` 的作用是让 Vue 识别"这是同一个节点，只是内容变了"，从而做 in-place patch 而不是销毁重建。`TimelineRenderer.vue` 对 text 类型 entry 使用了 `` `text:${entry.text}` `` 作为 key，把"正在增长的文本内容"当作节点身份。每次 streaming delta 到来，文本变长，key 随之改变，Vue 判定这是一个完全不同的节点，于是销毁旧的 `MarkdownRender` 组件实例、挂载新实例——这正是闪烁的来源。

`AssistantTimelineTextEntry` 类型本身没有稳定的唯一 ID（只有 `kind` 和 `text`），这是一个类型设计上的缺失，直接导致了 key 策略无法找到更好的锚点。

**是否有多个根因**：否。是单一根因（key 策略错误），类型缺少稳定 ID 是其直接诱因。

## 4. 影响面

- **影响范围**：所有触发流式输出的场景——即每次 AI 回复，全程必现
- **潜在受害模块**：仅 `TimelineRenderer` 中对 text 类型 entry 的渲染；tool 类型 entry 使用 `toolCallId` 作为 key，不受影响
- **数据完整性风险**：无。这是纯渲染层问题，不影响数据本身
- **严重程度复核**：维持 P2。功能完整可用，但视觉体验在每次对话都会受损

## 5. 修复方案

### 方案 A：给 `AssistantTimelineTextEntry` 添加稳定 `textId` 字段（推荐）

- **做什么**：
  1. `src/data/mock.ts`：在 `AssistantTimelineTextEntry` 类型加 `textId: string`
  2. `src/composables/chatRuntime/chatEventReducer.ts`：`appendMessageTextDelta` 中，创建新 text entry 时生成一次性 ID（如 `crypto.randomUUID()` 或 `\`text:${turn_id}:${sequence}\``），后续 delta 追加时保留同一 `textId`
  3. `src/components/chat/TimelineRenderer.vue`：key 改为 `entry.textId`
- **优点**：语义正确，彻底解决问题；`textId` 一旦生成即固定，无论 `text` 如何增长 key 都不变；Vue 做 in-place patch，`MarkdownRender` 实例全程复用
- **缺点/风险**：需要同时改类型定义、reducer、模板三处；如果有其他地方构造 `AssistantTimelineTextEntry` 字面量，也需要补 `textId`
- **影响面**：`mock.ts`（类型）、`chatEventReducer.ts`（reducer）、`TimelineRenderer.vue`（模板），需检查其他构造 text entry 的位置

### 方案 B：在 `v-for` 中使用数组下标作为 text 类型的 key

- **做什么**：
  1. `TimelineRenderer.vue`：将 `v-for` 改为 `v-for="(entry, index) in entries"`，key 改为 `` entry.kind === 'text' ? `text-idx:${index}` : entry.toolCallId ``
- **优点**：改动极小，只动一行模板代码；timeline 是 append-only 结构（streaming 期间只追加，不删除、不重排），index 天然稳定
- **缺点/风险**：使用 index 作为 key 是 Vue 官方不推荐的做法，若未来 timeline 出现删除或重排操作，可能引入新的渲染 bug；现在安全，但依赖"append-only"这一隐性约束
- **影响面**：仅 `TimelineRenderer.vue` 一处

### 推荐方案

**推荐方案 A**，理由：

- 根因是"text entry 缺少稳定身份"，方案 A 直接修复根因，方案 B 是绕过根因的权宜之计
- 方案 A 让 key 的语义清晰（"这个文本块的 ID"），不依赖任何关于数组结构的隐性假设
- 改动量虽然多一点，但都是可控的小改动（加一个字段、改一处 key）
- 方案 B 留下的隐性约束（append-only）没有类型或注释保障，是一颗定时炸弹

## 6. 修复记录（阶段三回填）

> 本节由 easysdd-issue-fix 阶段完成后回填。

- 实际采用方案：待填
- 改动文件清单：待填
- 验证结果：待填
- 遗留事项：待填
