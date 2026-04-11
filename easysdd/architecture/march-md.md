# MarchMd — 自研 Markdown 渲染引擎设计

## 背景与动机

当前使用 `markstream-vue`（v0.0.10）做 Markdown 渲染。它的模型是把文档解析成节点树后通过 batch timer 分批 flush 到 DOM——每次新 token 到来，所有"live nodes"都要重新计算，导致内容一批批跳出、持续闪烁，体验较差。

MarchMd 的目标：在 streaming 过程中消除闪烁，做到**只有文档末尾的当前块在变化**，其余内容完全静止。

---

## 核心设计：Sealed Prefix + Live Tail

### 洞察

Markdown 文档有天然的稳定边界。一旦一个块（段落、标题、代码块等）之后出现了空行或下一个块级元素，该块的内容就不可能再变化——无论后面 streaming 来多少 token。

利用这一点，把文档分成两个区域：

```
已 sealed 的块（稳定前缀）
┌──────────────────────────────┐
│ ## 这是标题（sealed）         │  渲染一次后永不再碰
│                              │
│ 第一段文字，已完成。（sealed） │  渲染一次后永不再碰
│                              │
│ ```python                    │
│ def foo(): pass              │  渲染一次后永不再碰
│ ```（sealed）                 │
└──────────────────────────────┘

Live 块（动态尾部）
┌──────────────────────────────┐
│ 正在流式输出的这一段文字▌      │  唯一会随 token 更新的部分
└──────────────────────────────┘
```

**整个文档中只有一个 live 块**（最后一个未完成的块）会在每次 token 到来时重渲。其余全部用 `v-once` 冻结，Vue 不对它们做任何 diff。

---

## 块的密封时机

| 块类型 | 密封条件 |
|--------|---------|
| 段落 `paragraph` | 遇到空行，或下一行是块级标记（`#`、` ``` `、`---`、`$$` 等） |
| 标题 `heading` | 行末换行（标题单行即完整） |
| 代码块 `code_fence` | 遇到匹配的结束围栏 ` ``` ` |
| Mermaid `mermaid` | 遇到匹配的结束围栏 ` ``` `（lang 为 `mermaid`，与代码块流程相同） |
| 公式块 `math_block` | 遇到独立的 `$$` 结束行 |
| 无序列表 `ul` | 空行后跟非列表内容 |
| 有序列表 `ol` | 空行后跟非列表内容 |
| 引用块 `blockquote` | 空行后跟非 `>` 内容 |
| 表格 `table` | 空行 |
| 分割线 `hr` | 行末换行（单行即完整） |

---

## Inline 解析：Lenient 模式

这是消除闪烁的第二个关键点。

**问题**：streaming 过程中 inline 标记符频繁处于半开状态（`**bold` 还没打完 `**`），如果此时强行解析，会导致布局突变——先显示 `**bold`，用户打完后变成粗体，整个行高、宽度都可能变化。

**解决方案**：live 块使用"宽容解析（lenient）"——遇到未闭合的标记符一律当字面量处理，不产生任何 DOM 结构变化。只有标记符闭合后，才在下一次解析中转为对应的格式化元素。

```
streaming 中:  "这是 **重要"    → 渲染为: 这是 **重要     （纯文字，无跳变）
token 到来:    "这是 **重要**"  → 渲染为: 这是 重要       （块 seal 时一次性切换）
```

**Sealed 块始终使用严格解析**，完全正确，无歧义。

`final=true`（streaming 结束）时，对 live 块做最终一次严格解析再密封，清理所有宽容模式留下的字面量标记符。

---

## 代码块的特殊处理

代码块是 streaming 中最容易出问题的结构。未闭合的 ` ``` ` 围栏会把后续所有内容都当作代码，造成灾难性的错误解析。

处理策略：

1. 解析器识别到 ` ```lang ` 开始行后，进入 `CODE_FENCE` 状态
2. 此后所有内容**直接追加为 raw text**，不做任何 inline 解析
3. Live 代码块在最后一行末尾渲染一个光标 `▌`
4. 遇到匹配的结束围栏时，密封代码块，移除光标
5. `final=true` 且代码块未闭合时，自动补一个结束围栏完成密封

```
streaming 中的代码块（live，未 sealed）：
┌─────────────────────────┐
│ ```python               │
│ def foo():              │
│     return 42▌          │  ← 光标在这里，raw text 追加
└─────────────────────────┘
```

---

## 数学公式渲染（KaTeX）

**库选型**：KaTeX。相比 MathJax，KaTeX 是**同步渲染**，无需等待异步加载，且体积更小、速度更快，完全满足需求。

**行内公式 `$...$`**（`MdMathInline.vue`）：

```vue
<template>
  <!-- sealed：用 KaTeX 渲染为 HTML -->
  <span v-if="block.sealed" class="march-math-inline" v-html="rendered" />
  <!-- live（streaming 中）：显示原始 LaTeX，避免 KaTeX 报错和布局抖动 -->
  <span v-else class="march-math-inline-raw">{{ latex }}</span>
</template>

<script setup lang="ts">
import katex from 'katex'
const rendered = computed(() =>
  katex.renderToString(props.latex, { throwOnError: false, displayMode: false })
)
</script>
```

**公式块 `$$...$$`**（`MdMathBlock.vue`）：

```vue
<template>
  <div v-if="block.sealed" class="march-math-block" v-html="rendered" />
  <pre v-else class="march-math-block-raw">{{ block.latex }}<MdCursor v-if="cursor" /></pre>
</template>

<script setup lang="ts">
const rendered = computed(() =>
  katex.renderToString(props.block.latex, { throwOnError: false, displayMode: true })
)
</script>
```

`throwOnError: false`：LaTeX 语法错误时渲染为红色错误提示，不抛异常、不白屏。

KaTeX CSS 在 `main.ts` 中一次性引入：`import 'katex/dist/katex.min.css'`。

---

## Mermaid 渲染

**库**：`mermaid`（官方 JS 库）。渲染是**异步**的，需要特殊处理。

**渲染策略**：

- **Streaming 中（live 块）**：和普通代码块一样，显示 raw Mermaid DSL 文本，不触发渲染
- **块密封（sealed=true）时**：触发一次 `mermaid.render()`，结果替换为 SVG
- **渲染失败**：回退展示原始代码块，附带错误信息

```vue
<!-- MdMermaid.vue -->
<template>
  <div class="march-mermaid">
    <!-- 渲染成功：展示 SVG -->
    <div v-if="svg" v-html="svg" class="march-mermaid-diagram" />
    <!-- 渲染失败：回退为代码块 -->
    <pre v-else-if="error" class="march-mermaid-fallback"><code>{{ block.code }}</code>
<span class="march-mermaid-error">{{ error }}</span></pre>
    <!-- 渲染中 / live 阶段：显示原始代码 -->
    <pre v-else class="march-mermaid-source"><code>{{ block.code }}</code><MdCursor v-if="cursor" /></pre>
  </div>
</template>

<script setup lang="ts">
import mermaid from 'mermaid'
import { ref, watch } from 'vue'

const props = defineProps<{ block: MermaidBlock; cursor?: boolean }>()
const svg   = ref<string | null>(null)
const error = ref<string | null>(null)

// 只在 sealed 后渲染一次
watch(() => props.block.sealed, async (sealed) => {
  if (!sealed) return
  try {
    const { svg: result } = await mermaid.render(`mermaid-${props.block.id}`, props.block.code)
    svg.value = result
  } catch (e) {
    error.value = String(e)
  }
}, { immediate: true })
</script>
```

**Mermaid 初始化**（`main.ts` 或 `MarchMd.vue` 内）：

```typescript
import mermaid from 'mermaid'
mermaid.initialize({
  startOnLoad: false,          // 关闭自动扫描，我们手动调用 render
  theme: 'neutral',            // 或 'dark'，配合 March 主题
  securityLevel: 'strict',     // 防止 XSS
})
```

---

## 架构分层

```
src/lib/march-md/          ← 纯逻辑层，无 Vue 依赖
  types.ts                 ← AST 节点类型
  block-parser.ts          ← 行级状态机：识别块边界，维护 sealed/live 列表
  inline-parser.ts         ← Inline 元素解析（strict + lenient 两种模式）
  index.ts                 ← 对外导出

src/components/march-md/   ← Vue 渲染层
  MarchMd.vue              ← 主组件
  blocks/
    MdParagraph.vue
    MdHeading.vue
    MdCodeBlock.vue
    MdMermaid.vue          ← Mermaid 图表渲染
    MdMathBlock.vue        ← $$ 公式块渲染（KaTeX）
    MdList.vue
    MdTable.vue
    MdBlockquote.vue
    MdHr.vue
  inline/
    MdMathInline.vue       ← $ 行内公式渲染（KaTeX）
  MdCursor.vue             ← 闪烁光标
```

---

## 类型定义（types.ts）

```typescript
// 块类型
export type BlockType =
  | 'paragraph'
  | 'heading'
  | 'code_fence'
  | 'mermaid'       // ```mermaid 围栏，特殊渲染
  | 'math_block'    // $$ ... $$ 独立公式块
  | 'ul'
  | 'ol'
  | 'blockquote'
  | 'table'
  | 'hr'

// Inline 节点
export type InlineNode =
  | { type: 'text';        value: string }
  | { type: 'bold';        children: InlineNode[] }
  | { type: 'italic';      children: InlineNode[] }
  | { type: 'bold_italic'; children: InlineNode[] }
  | { type: 'code';        value: string }
  | { type: 'math_inline'; latex: string }   // $...$ 行内公式
  | { type: 'link';        href: string; children: InlineNode[] }
  | { type: 'image';       src: string; alt: string }
  | { type: 'strikethrough'; children: InlineNode[] }
  | { type: 'literal';     value: string }  // lenient 模式下未闭合的标记符

// 块节点
export interface Block {
  id: string           // 稳定 ID，用作 Vue key
  type: BlockType
  sealed: boolean
  // 各块类型的具体数据
  raw: string          // 原始文本（调试用 + 最终严格解析用）
}

export interface ParagraphBlock extends Block {
  type: 'paragraph'
  inlines: InlineNode[]
}

export interface HeadingBlock extends Block {
  type: 'heading'
  level: 1 | 2 | 3 | 4 | 5 | 6
  inlines: InlineNode[]
}

export interface CodeFenceBlock extends Block {
  type: 'code_fence'
  lang: string
  code: string         // raw text，不做 inline 解析
}

export interface ListBlock extends Block {
  type: 'ul' | 'ol'
  items: ListItem[]
}

export interface ListItem {
  inlines: InlineNode[]
  children?: ListBlock  // 嵌套列表
}

export interface BlockquoteBlock extends Block {
  type: 'blockquote'
  innerBlocks: Block[]  // 引用块内部递归解析
}

export interface TableBlock extends Block {
  type: 'table'
  header: InlineNode[][]
  align: ('left' | 'center' | 'right' | null)[]
  rows: InlineNode[][]
}

export interface HrBlock extends Block {
  type: 'hr'
}

export interface MathBlockBlock extends Block {
  type: 'math_block'
  latex: string     // raw LaTeX，不做任何解析
}

export interface MermaidBlock extends Block {
  type: 'mermaid'
  code: string      // raw Mermaid DSL
}
```

---

## Block Parser 状态机（block-parser.ts）

```typescript
export class BlockParser {
  private blocks: Block[] = []
  private state: ParserState = { mode: 'top_level' }
  private blockIdCounter = 0

  // 追加新文本（streaming 场景，content 是累积全文）
  feed(content: string): void { ... }

  // 获取当前 sealed 块列表（稳定，不变）
  getSealedBlocks(): Block[] { ... }

  // 获取当前 live 块（可能为 null）
  getLiveBlock(): Block | null { ... }

  // 最终化：对 live 块做严格解析后密封
  finalize(): void { ... }
}

type ParserMode =
  | 'top_level'      // 顶层，等待下一个块的开始
  | 'paragraph'      // 在段落内
  | 'code_fence'     // 在代码围栏内（含 mermaid）
  | 'math_block'     // 在 $$ ... $$ 公式块内
  | 'list'           // 在列表内
  | 'blockquote'     // 在引用块内
  | 'table'          // 在表格内

type ParserState = { mode: ParserMode; [key: string]: any }
```

**逐行处理逻辑**（伪代码）：

```
for each line in newLines:
  switch state.mode:
    case top_level:
      if line matches /^#{1,6} /:     → start HeadingBlock, immediate seal
      if line matches /^```mermaid/:  → start MermaidBlock, enter code_fence (isMermaid=true)
      if line matches /^```/:         → start CodeFenceBlock, enter code_fence (isMermaid=false)
      if line matches /^\$\$$/:       → start MathBlockBlock, enter math_block
      if line matches /^[-*+] /:      → start ListBlock ul, enter list
      if line matches /^\d+\. /:      → start ListBlock ol, enter list
      if line matches /^>/:           → start BlockquoteBlock, enter blockquote
      if line matches /^(\|.+\|)/:    → start TableBlock, enter table
      if line matches /^---+$/:       → emit HrBlock (sealed immediately)
      if line is empty:               → stay top_level
      else:                           → start ParagraphBlock, enter paragraph

    case paragraph:
      if line is empty:               → seal current paragraph, → top_level
      if line is block-level marker:  → seal current paragraph, reprocess line
      else:                           → append line to paragraph

    case code_fence:
      if line matches closing fence:  → seal CodeFenceBlock/MermaidBlock, → top_level
      else:                           → append raw line

    case math_block:
      if line matches /^\$\$$/:       → seal MathBlockBlock, → top_level
      else:                           → append raw line to latex content

    case list / blockquote / table:
      （类似逻辑，识别结束条件后 seal）
```

---

## Inline Parser（inline-parser.ts）

```typescript
// 严格模式：标准 Markdown inline 规则
export function parseInlineStrict(text: string): InlineNode[] { ... }

// 宽容模式：未闭合标记符输出为 literal 节点
export function parseInlineLenient(text: string): InlineNode[] { ... }
```

**扫描策略**：左到右单遍扫描，维护一个标记符栈。

```
对于每个字符：
  if 遇到标记符开始（**、*、`、~~、[）：
    push 到栈
  if 遇到标记符结束且栈顶匹配：
    pop 栈，产生对应 InlineNode
  
lenient 模式下，扫描结束时：
  栈中未闭合的标记符 → 展平为 literal 节点
  
strict 模式下，扫描结束时：
  栈中未闭合的标记符 → 也展平为 literal 节点（标准行为：视为普通字符）
```

支持的 inline 元素（按优先级）：

1. `` `code` `` — 内联代码，最高优先级，内部不做进一步解析
2. `$...$` — 行内公式，第二优先级，内部不做 inline 解析（防止 `$a*b$` 里的 `*` 被误解析为斜体）
3. `**bold**` / `__bold__`
4. `*italic*` / `_italic_`
5. `***bold italic***`
6. `~~strikethrough~~`
7. `[text](url)` — 链接
8. `![alt](url)` — 图片
9. `\escape` — 反斜杠转义

**`$...$` 的歧义处理**：普通文本中的 `$` 很常见（价格、Shell 变量），按以下规则区分：
- 开括号 `$` 后面第一个字符**不是空格**，且与内容之间没有换行
- 闭括号 `$` 前面**不是空格**
- 内容非空
- lenient 模式下：`$` 后没有找到闭合 `$` → 按字面量输出 `$`

---

## Vue 主组件（MarchMd.vue）

```vue
<template>
  <div class="march-md">
    <!-- Sealed 块：v-once，渲染后 Vue 永不再 diff -->
    <component
      v-for="block in sealedBlocks"
      :is="blockComponent(block.type)"
      v-once
      :key="block.id"
      :block="block"
    />

    <!-- Live 块：每次 content 变化都会重渲，但只有这一个 -->
    <component
      v-if="liveBlock"
      :is="blockComponent(liveBlock.type)"
      :key="liveBlock.id"
      :block="liveBlock"
      :lenient="true"
      :cursor="cursor"
    />
  </div>
</template>

<script setup lang="ts">
import { computed, watchEffect } from 'vue'
import { BlockParser } from '@/lib/march-md/block-parser'

const props = defineProps<{
  content: string
  final?: boolean
  cursor?: boolean
}>()

const parser = new BlockParser()

watchEffect(() => {
  parser.feed(props.content)
  if (props.final) parser.finalize()
})

const sealedBlocks = computed(() => parser.getSealedBlocks())
const liveBlock    = computed(() => parser.getLiveBlock())
</script>
```

---

## 光标组件（MdCursor.vue）

```vue
<template>
  <span class="march-md-cursor" aria-hidden="true">▌</span>
</template>

<style scoped>
.march-md-cursor {
  display: inline-block;
  animation: cursor-blink 1s step-end infinite;
  opacity: 1;
  color: var(--color-accent, currentColor);
}

@keyframes cursor-blink {
  0%, 100% { opacity: 1; }
  50%       { opacity: 0; }
}
</style>
```

---

## 与现有代码的集成

`<TimelineRenderer />` 组件遍历 `AssistantMessage.timeline`，对每个 `{ kind: 'text' }` 条目渲染 `<MarchMd>`，传入拼合文本与 streaming 状态：

```vue
<!-- TimelineRenderer 内部，渲染单个 TextChunk -->

<!-- 历史消息内的文本条目（Turn 已 done） -->
<MarchMd
  :content="entry.text"
  :final="true"
/>

<!-- Streaming 中最后一个 TextChunk（Turn 仍 streaming 且该 entry 是 timeline 末尾） -->
<MarchMd
  :content="entry.text"
  :final="false"
  :cursor="true"
/>
```

完整的 TextChunk 文本由 `chatEventReducer` 增量追加维护，`<TimelineRenderer />` 直接读取 `entry.text`，无需手动拼合。`<MarchMd>` 自身收到 `:final="true"` 或 `:cursor="false"` 后按最终态渲染，不再激活 Live Tail。

移除 `markstream-vue` 相关的 import（`main.ts` 第 6 行）及 CSS 覆盖样式。

---

## 依赖

```
katex        — 同步 LaTeX 渲染，体积小、速度快
mermaid      — 官方 Mermaid JS 库，异步 SVG 渲染
```

两者均按需引入，不影响主包初始加载。

---

## 实现顺序

1. `types.ts` — 定义 AST 类型（无依赖，先写稳）
2. `inline-parser.ts` — 先实现 strict 模式（含 `$...$`），再加 lenient
3. `block-parser.ts` — 行级状态机，单元测试驱动（含 `$$` 和 `mermaid` 分支）
4. Block Vue 组件 — 从 `MdParagraph`、`MdHeading`、`MdCodeBlock` 开始，再加 `MdMathBlock`、`MdMermaid`
5. `MarchMd.vue` — 组装，接入 `ChatMessageList.vue`
6. 样式迁移 — 把 `main.css` 里 markstream 相关覆盖样式迁移到组件 scoped 样式

---

## 不实现的部分（有意排除）

- **HTML 块**（`<div>` 等原始 HTML 嵌入）：安全风险，不支持
- **脚注**：复杂度高，使用场景极少
- **定义列表**：非标准 GFM，不支持

---

## 参考

- [CommonMark Spec](https://spec.commonmark.org/)
- [GFM Spec](https://github.github.com/gfm/)（表格、删除线扩展）
