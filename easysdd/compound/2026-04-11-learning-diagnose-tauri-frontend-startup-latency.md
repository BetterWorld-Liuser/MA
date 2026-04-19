---
track: knowledge
date: 2026-04-11
slug: diagnose-tauri-frontend-startup-latency
component: tauri/frontend-dev-workflow
tags: [tauri, vite, debugging, performance, startup]
---

## 背景
Tauri dev 启动黑屏，初步怀疑 Vite 编译、WebView2、CSS 处理等多个方向，
需要系统方法把问题定位到具体阶段。

## 指导原则
在 `main.ts` 最顶部插入 `console.time` 打点，配合浏览器 DevTools 网络面板，
可以在 5 分钟内将 Tauri 启动延迟定位到「哪个阶段」。

## 为什么重要
Tauri 启动链路长（Rust 编译 → Vite → WebView2 → JS → IPC），每个阶段都可能是瓶颈。
没有打点就只能猜测，容易走错方向（本次误判了 3 次才找到根因）。

## 何时适用
任何 Tauri app 出现「启动后黑屏 / 白屏持续超过 5 秒」的情况。

## 示例

**Step 1**：在 `main.ts` 顶部加打点：

```ts
console.time('[startup] total');
console.timeLog('[startup] total', 'JS execution started');
// ... imports ...
app.mount('#app');
console.timeEnd('[startup] total');
```

**Step 2**：打开 Tauri DevTools（黑屏时右键 → Inspect），看 Console。

**读结果**：
- 「已导航到 localhost:5173」出现 → JS 执行打点之间有很长间隔 → 瓶颈在 Vite 模块编译
- JS 很快执行，但 app mounted 之后卡顿 → 瓶颈在 JS 初始化逻辑或 IPC
- JS 执行正常，视觉上黑屏 → 瓶颈在 CSS 变量/主题加载

**Step 3**：用 Chrome 打开同一 `devUrl`。
- Chrome 也慢 → 问题在 Vite/构建配置
- Chrome 快，WebView2 慢 → 问题在 WebView2 本身（HTTP 栈差异）

**本次结论**：JS 执行前 gap = 1 分钟，Chrome 同样慢 → 确认是 Vite 懒编译，与 WebView2 无关。
