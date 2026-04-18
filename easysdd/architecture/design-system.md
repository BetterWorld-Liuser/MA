# Design System

> March 的视觉规范不再散落在各 `ui-*.md` 和组件样式里，而是收敛成一套三层 token 体系：**primitive → semantic → component**。所有界面文档只描述"这个界面长什么样、怎么交互"，本文负责定义"所有界面共享的原子与规则"。

当前实现进度见 → [Working/design-system-status.md](Working/design-system-status.md)

---

## 为什么要单独抽出来

现状（见 `src/styles/vars.css` 和 `src/styles/main.css`）：

- 原子值（`#0a0a0a`、`rgba(255,255,255,0.08)`、`12px`）和语义角色（`--ma-bg`、`--ma-text-muted`）混在同一层。改一个颜色色相，需要在深色/浅色两份重复改十几个位置。
- 没有显式间距 scale，组件里散落 `gap-1.5`、`px-3`、`mb-3`，相邻组件视觉节奏靠手工对齐。
- 阴影有 4 个命名（panel / floating / composer / dialog），但没有 elevation 概念，新组件选哪个靠猜。
- 组件样式里还残留硬编码（如 `hover:bg-[#c42b1c]`），以及局部的 `color-mix()` 推导，应该是 token 的都降级成了字面量。
- 最近的 "unify UI font scale" 和 "update styles for improved UI consistency" 已经朝这个方向走了一步，但只动了字号；色板、间距、阴影没跟上。

目标：消费端**永远不写字面量**，只用 semantic token；semantic token 在主题里重映射；primitive 层独立演化色板和 scale。

---

## 三层架构

```
┌─────────────────────────────────────────────────────────────┐
│  primitive 层（参考值，不带语义）                              │
│  color.gray.50..900、color.orange.500、space.1..10、radius.1..4│
│  size.font.10..24、duration.fast/base/slow、ease.*            │
└─────────────────────────────────────────────────────────────┘
                          ▲ 引用
┌─────────────────────────────────────────────────────────────┐
│  semantic 层（角色映射，主题切换在这一层发生）                  │
│  bg.canvas / bg.surface / bg.elevated / text.primary /        │
│  text.muted / border.subtle / accent.default / status.error / │
│  elevation.1..4 / space.gutter / space.inline                 │
└─────────────────────────────────────────────────────────────┘
                          ▲ 引用
┌─────────────────────────────────────────────────────────────┐
│  component 层（只在需要定制时才存在）                          │
│  chat.bubble.user.bg、titlebar.close.hover.bg、              │
│  composer.shadow、task-list.row.active.bg                    │
└─────────────────────────────────────────────────────────────┘
                          ▲ 消费
┌─────────────────────────────────────────────────────────────┐
│  消费端（Vue 组件、Tailwind utility、@utility 组合类）          │
│  只允许引用 semantic 或 component 层，**不允许**直接用 primitive │
└─────────────────────────────────────────────────────────────┘
```

**硬性规则**：

1. 组件 / utility class 不能出现 hex、rgb、rgba、`#xxx` 字面量颜色。
2. 组件 / utility class 不能出现除 `0` 以外的裸 px 间距（必须用 space token 或 Tailwind 映射）。
3. 主题切换只重写 semantic 层；primitive 层是主题无关的"颜料盘"。
4. component 层 token 只在当组件有"独特视觉特性、无法用 semantic 组合"时才新增（如聊天气泡的玻璃感底色）。

---

## Primitive 层

文件：`src/styles/tokens.primitive.css`

### 色板（无主题、无语义）

```css
:root {
  /* 中性阶 —— 深色主题取高位，浅色主题取低位 */
  --color-gray-0:   #000000;
  --color-gray-50:  #0a0a0a;
  --color-gray-100: #111111;
  --color-gray-150: #151515;
  --color-gray-200: #1a1a1a;
  --color-gray-250: #202020;
  --color-gray-300: #2a2a2a;
  --color-gray-400: #444444;
  --color-gray-500: #5a5a5a;
  --color-gray-600: #75685b;  /* 偏暖，浅色主题用 */
  --color-gray-700: #8a8a8a;
  --color-gray-800: #e8e8e8;
  --color-gray-900: #ffffff;

  /* 暖中性阶（浅色主题） */
  --color-warm-50:  #fbf8f3;
  --color-warm-100: #f4efe8;
  --color-warm-200: #efe8df;
  --color-warm-300: #e8ddd1;
  --color-warm-400: #d9cdbf;
  --color-warm-500: #c4b19c;
  --color-warm-900: #241d16;

  /* 品牌色 —— 橙 */
  --color-orange-400: #e07a3a;
  --color-orange-500: #d4692a;
  --color-orange-600: #c05820;

  /* 状态色 */
  --color-yellow-500: #e6a817;
  --color-yellow-600: #b98512;
  --color-red-500:    #e05252;
  --color-red-600:    #c74f4f;
  --color-green-500:  #4caf7d;
  --color-green-600:  #3f8f69;

  /* Alpha 原语（用于半透明层） */
  --alpha-white-02: rgba(255, 255, 255, 0.02);
  --alpha-white-04: rgba(255, 255, 255, 0.04);
  --alpha-white-08: rgba(255, 255, 255, 0.08);
  --alpha-white-14: rgba(255, 255, 255, 0.14);
  --alpha-black-45: rgba(0, 0, 0, 0.45);
  --alpha-warm-06:  rgba(36, 29, 22, 0.06);
  --alpha-warm-10:  rgba(36, 29, 22, 0.1);
  --alpha-warm-16:  rgba(36, 29, 22, 0.16);
}
```

### 间距 scale（4px 基准）

```css
--space-0:  0;
--space-1:  2px;
--space-2:  4px;
--space-3:  6px;
--space-4:  8px;
--space-5:  12px;
--space-6:  16px;
--space-7:  20px;
--space-8:  24px;
--space-9:  32px;
--space-10: 48px;
```

用完这 11 档覆盖当前所有 `gap-*` / `p-*` / `m-*` 的使用。Tailwind v4 的 `@theme` 里把 `--spacing-*` 映射到这些值，组件里继续写 `gap-4` / `px-5`，但含义从"Tailwind 默认 1rem"变成"March space-4"。

### 字号 scale（已有，保留）

```css
--font-size-dense:   10px;  /* 仅 mono/debug/status */
--font-size-caption: 12px;  /* meta、标签 */
--font-size-ui:      13px;  /* 按钮、控件 */
--font-size-body:    14px;  /* 正文阅读 */
--font-size-title:   16px;  /* 区块标题 */
```

超过 16px 的尺寸（设置页大标题、空状态大字）继续用 Tailwind 预设，不进 primitive 层，避免过早固化。

### 圆角 scale

```css
--radius-1: 4px;   /* 输入框、小按钮、tag */
--radius-2: 6px;   /* 卡片、消息气泡 */
--radius-3: 10px;  /* 面板、对话框 */
--radius-4: 16px;  /* 窗口外壳（Tauri 圆角） */
--radius-full: 9999px;
```

### Elevation（阴影）

把现有的 panel/floating/composer/dialog 四个阴影改成按层级命名：

```css
--elevation-0: none;
--elevation-1:
  0 0 0 1px var(--alpha-white-04),
  0 18px 50px var(--alpha-black-45);    /* 面板、侧栏 */
--elevation-2:
  0 14px 40px rgba(0, 0, 0, 0.28),
  inset 0 1px 0 var(--alpha-white-04);  /* 输入区、浮动条 */
--elevation-3: 0 24px 60px rgba(0, 0, 0, 0.42);  /* 浮层、菜单 */
--elevation-4:
  0 28px 90px rgba(0, 0, 0, 0.48),
  inset 0 1px 0 rgba(255, 255, 255, 0.05);  /* 对话框、设置覆盖层 */
```

组件用 `box-shadow: var(--elevation-2)`，不关心它由哪几层阴影叠出来。

### 动效 token

```css
--duration-fast:   120ms;   /* hover、focus 切换 */
--duration-base:   200ms;   /* 面板展开、tooltip */
--duration-slow:   320ms;   /* 路由切换、对话框进出 */
--ease-standard:   cubic-bezier(0.4, 0, 0.2, 1);
--ease-accelerate: cubic-bezier(0.4, 0, 1, 1);
--ease-decelerate: cubic-bezier(0, 0, 0.2, 1);
```

---

## Semantic 层

文件：`src/styles/tokens.semantic.css`

这是消费端看到的主要接口。命名按"做什么用"，不按"长什么样"。

### 表面与背景

```
bg.canvas          应用最底层背景（窗口底色）
bg.surface         第一层面板（聊天区主背景、任务列表）
bg.surface-strong  第一层面板的强调态（hover、selected 的弱化版）
bg.elevated        浮起的面板（设置页、上下文面板卡片）
bg.elevated-strong 更浮起的浮层（popover、menu）
bg.overlay         对话框 / 设置页背景遮罩
```

### 文字

```
text.primary       正文、标题
text.secondary     次要信息（时间戳、meta）
text.muted         弱化信息（placeholder、dim）
text.on-accent     品牌色背景上的文字
text.on-status     状态色背景上的文字
```

### 边框与分隔线

```
border.subtle      低对比分隔线（面板内部）
border.default     常规边框（卡片、输入框）
border.strong      强调边框（focus、active）
border.accent      品牌色边框
```

### 品牌与状态

```
accent.default / accent.hover / accent.subtle
status.warning / status.error / status.success
status.warning-subtle / status.error-subtle / status.success-subtle
```

`*-subtle` 是 `color-mix()` 到 `bg.surface` 上的浅底色，用于状态提示条、badge 背景。

### 间距角色（可选映射）

```
space.inline   行内元素间距（图标与文字、标签间）→ space-3
space.stack    垂直堆叠间距 → space-4
space.gutter   面板内边距 → space-6
space.section  区块之间分隔 → space-8
```

大部分场景直接用 `space-*` scale 就够，这组别名只在反复出现的布局概念上启用。

### 示例实现

```css
:root {
  --bg-canvas:          var(--color-gray-50);
  --bg-surface:         var(--alpha-white-02);
  --bg-surface-strong:  var(--alpha-white-04);
  --bg-elevated:        rgba(10, 10, 10, 0.92);
  --bg-elevated-strong: rgba(10, 10, 10, 0.98);
  --bg-overlay:         rgba(3, 3, 3, 0.7);

  --text-primary:   var(--color-gray-800);
  --text-secondary: var(--color-gray-700);
  --text-muted:     var(--color-gray-500);
  --text-on-accent: #16110d;

  --border-subtle:  var(--alpha-white-08);
  --border-default: var(--color-gray-300);
  --border-strong:  var(--color-gray-400);
  --border-accent:  var(--color-orange-500);

  --accent-default: var(--color-orange-500);
  --accent-hover:   var(--color-orange-400);
  --accent-subtle:  rgba(212, 105, 42, 0.15);

  --status-warning: var(--color-yellow-500);
  --status-error:   var(--color-red-500);
  --status-success: var(--color-green-500);
}

:root[data-theme='light'] {
  --bg-canvas:          var(--color-warm-100);
  --bg-surface:         rgba(255, 255, 255, 0.72);
  --bg-surface-strong:  rgba(255, 255, 255, 0.9);
  --bg-elevated:        rgba(251, 248, 243, 0.94);
  --bg-elevated-strong: rgba(251, 248, 243, 0.98);
  --bg-overlay:         rgba(192, 175, 154, 0.34);

  --text-primary:   var(--color-warm-900);
  --text-secondary: var(--color-gray-600);
  --text-muted:     #9a8c7d;

  --border-subtle:  var(--alpha-warm-10);
  --border-default: var(--color-warm-400);
  --border-strong:  var(--color-warm-500);

  --status-warning: var(--color-yellow-600);
  --status-error:   var(--color-red-600);
  --status-success: var(--color-green-600);
}
```

**主题的关键**：primitive 层一行不改，只重映射 semantic。

---

## Component 层

文件：`src/styles/tokens.component.css`

只承载"semantic 不足以表达，但确实跨多个组件复用"的值。入选标准：

- 视觉上有独特构造（如聊天气泡的玻璃感底色、窗口关闭按钮的红色 hover）
- 至少被两个地方使用，或有强烈主题差异（深色有、浅色无）
- 不是单个组件内部的一次性值

示例：

```css
/* 聊天气泡 */
--chat-bubble-user-bg:      color-mix(in srgb, var(--bg-surface-strong) 94%, transparent);
--chat-bubble-assistant-bg: color-mix(in srgb, var(--bg-surface) 88%, transparent);
--chat-bubble-border:       var(--border-subtle);
--chat-bubble-shadow:       none;  /* 浅色主题下覆盖 */

/* 窗口标题栏 */
--titlebar-close-hover-bg: #c42b1c;
--titlebar-close-hover-fg: #ffffff;

/* 输入框（Composer） */
--composer-shadow: var(--elevation-2);
--composer-bg:     var(--bg-elevated);
```

一次性值不进这层，留在组件的 `<style>` 里。

---

## 消费规则

### 在 Vue 组件里

```vue
<template>
  <!-- 用 Tailwind utility，背后映射到 semantic token -->
  <button class="bg-surface text-primary border border-subtle rounded-2 px-5 py-3">
    发送
  </button>
</template>
```

### 在 `@utility` 合成类里

`src/styles/main.css`：

```css
@utility message-bubble {
  background: var(--chat-bubble-user-bg);
  border: 1px solid var(--chat-bubble-border);
  border-radius: var(--radius-2);
  box-shadow: var(--chat-bubble-shadow);
  padding: var(--space-5) var(--space-6);
}
```

### Tailwind v4 `@theme` 映射

`src/styles/main.css` 里的 `@theme` 块不再直接引用原始 hex，只引用 semantic token：

```css
@theme {
  --color-bg-canvas: var(--bg-canvas);
  --color-bg-surface: var(--bg-surface);
  --color-text-primary: var(--text-primary);
  --color-text-muted: var(--text-muted);
  --color-border-subtle: var(--border-subtle);
  --color-accent: var(--accent-default);
  --radius-1: var(--radius-1);
  --radius-2: var(--radius-2);
  --spacing-1: var(--space-1);
  /* … */
}
```

这样 Tailwind 的 `bg-bg-surface`、`text-text-primary`、`rounded-2`、`px-5` 全部走 semantic。

### 旧命名的废弃路径

`--ma-*` 和 `--color-*` 作为"兼容别名"保留一个过渡期，指向新的 semantic token。组件逐步迁移完成后删除。

---

## 文件组织

```
src/styles/
  tokens.primitive.css      # 原子值，理论上几乎不改
  tokens.semantic.css       # 角色映射 + 主题覆盖
  tokens.component.css      # 跨组件复用的定制 token
  main.css                  # Tailwind @theme + @utility 合成类
  vars.css                  # 【废弃中】保留为 semantic 的别名层
```

入口 `src/main.ts`（或 `main.css` 的 `@import`）按 primitive → semantic → component → main 的顺序引入，后面的可以覆盖前面的。

---

## 迁移路径

建议分四步走，每一步都能独立验证，不会一次性把 UI 打穿：

**Step 1 — 建立 primitive 层**
- 新建 `tokens.primitive.css`，把所有 hex、rgba 数值搬进去
- `vars.css` 的 `--ma-*` 改为引用 primitive token，外观完全不变
- 验证：`pnpm dev`，深色/浅色主题切换观察，应该零视觉差

**Step 2 — 引入 semantic 层**
- 新建 `tokens.semantic.css`，定义上文列出的角色 token
- `@theme` 块改为引用 semantic token
- `vars.css` 里的 `--ma-*` 继续存在，但也改为指向 semantic，作为兼容别名

**Step 3 — 组件迁移**
- 按目录逐个走：`ui/` → `chat/` → `context/` → `settings/` → `shell`
- 每个组件把 `bg-bg-secondary` → `bg-bg-surface`、`text-text-muted` → `text-text-muted`（语义不变但来源换）
- 清理硬编码 hex（如 `hover:bg-[#c42b1c]` → 引用 `--titlebar-close-hover-bg`）
- 清理裸 px（如 `px-3` 保留，但 `w-[13px]` 这种看情况提炼）

**Step 4 — 抽 component 层 & 删除别名**
- 把聊天气泡、窗口控件、composer 等跨组件定制值归档到 `tokens.component.css`
- 观察一周没有回退，删除 `vars.css` 里的 `--ma-*` 别名

每一步结束后更新 `Working/design-system-status.md` 记录进度。

---

## 设计原则

**primitive 不带语义**：`--color-orange-500` 永远不出现在消费端代码里，只出现在 semantic 层的赋值右侧。

**semantic 是稳定接口**：组件消费 semantic token 后，即使 primitive 色板整体换一套，组件代码也不用改。

**component 层要克制**：能用 semantic 组合出来的就不进 component；component 层越小越健康。

**主题切换只在 semantic 层**：任何需要深色/浅色差异的值都在 semantic 层定义两份；primitive 和 component 默认主题无关（除非该组件本身有强主题特性）。

**约束优于灵活**：宁可让 scale 粗一点（11 档间距而不是无限自由），也不要让消费端回到"自己写 padding: 13px"的自由。

---

## Claude 视觉语言的选择性吸收

March 的骨架是**桌面密度 UI**（见 ui.md 的 information density 约束），不能照搬 Claude.ai 的编辑器式留白。从 Claude 的视觉语言里吸收的部分：

- **暖色板整体替换**：primitive 层的中性色全部换成 warm-* 系（parchment `#f5f4ed`、ivory `#faf9f5`、Anthropic near-black `#141413`），强调色换成 terracotta `#c96442`。不再用冷蓝灰。
- **ring-based elevation**：浅色主题 `--shadow-panel/composer/floating/dialog` 全部由 `0 0 0 1px <warm-border>` + 轻柔 whisper 阴影构成；深色主题保留原有的偏重阴影。新增 `--elevation-ring / ring-strong / ring-subtle / whisper` 四个 primitive。
- **Serif 角色字体（opt-in）**：新增 `--font-family-serif`（Anthropic Serif / Source Serif 4 / Songti SC fallback）和 `--font-heading` 语义 token。**仅在 heading-class 表面使用**（dialog title、settings hero、空态大标题、品牌字样），正文/UI 控件/聊天气泡一律保持 sans。
- **Serif 工具类**：`heading-serif` / `heading-serif-display` / `heading-serif-lg` / `heading-serif-md` / `brand-wordmark`，定义在 `main.css` 的 @utility 里，消费端直接加 class。

**不吸收的部分**：
- 不放大留白、不降密度。间距 scale、字号 scale、组件 padding 都保持原值。
- 不用 Claude 官网的章节 hero 式排版；March 仍是紧凑的多面板工作台。
- 聊天内容、按钮标签、表单 label 不用 serif，避免桌面工具变成"文章阅读器"。
