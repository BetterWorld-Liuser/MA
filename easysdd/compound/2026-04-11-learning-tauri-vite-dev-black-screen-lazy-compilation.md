---
track: pitfall
date: 2026-04-11
slug: tauri-vite-dev-black-screen-lazy-compilation
component: tauri/frontend-dev-workflow
severity: high
tags: [tauri, vite, windows, dev-startup, webview2]
---

## 问题
Tauri + Vite dev 模式每次冷启动，WebView 黑屏约 1 分钟才渲染出 UI。

## 症状
- `cargo run` 完成、`Running march-ui.exe` 输出后，窗口黑屏约 1 分钟
- DevTools Console 无报错
- DevTools Network 显示所有 JS/Vue 请求处于 pending 状态，约 1 分钟后全部一次性完成
- 第二次访问（不关闭 Vite dev server）立即渲染，没有黑屏

## 没用的做法
- 添加 `server.warmup` glob 覆盖所有源文件 → warmup 本身也需要 1 分钟，WebView 在 Vite "ready"
  后立即连接，warmup 来不及完成，无效
- 怀疑 WebView2 的 HTTP 开销 → Chrome 打开同一 URL 表现一致，排除 WebView2
- 怀疑 Tailwind CSS v4 扫描阻塞 → Tailwind 扫描完成后 CSS 在 0.15ms 内 imported，不是瓶颈
- 怀疑 Vite 启动慢 → `vite` 自身 470ms 即 ready，不是瓶颈

## 解法
将 `beforeDevCommand` 从 `vite`（dev server）改为 `vite build --watch`，
同时移除 `tauri.conf.json` 中的 `devUrl`，让 Tauri 直接加载 `frontendDist`（`dist/`）。

```json
// tauri.conf.json
"build": {
  "beforeDevCommand": "npm run dev:build-watch",
  "frontendDist": "../dist"
}
```

```json
// package.json
"dev:build-watch": "vite build --watch"
```

## 为什么有效
Vite dev server 对源码采用**懒编译**：80+ 个 `.vue`/`.ts` 文件在浏览器首次请求时才逐个编译，
全量编译约需 1 分钟（编译结果缓存于内存，第二次即时响应）。
`vite build` 改用 Rollup 预先打包成 bundle，WebView 加载静态文件，彻底消除请求瀑布。
实测 `vite build` 耗时 8.75s（797 modules），启动等待从 1 分钟降至约 9 秒。

## 预防
Tauri 桌面 app 开发时，若前端源文件超过 ~50 个，优先考虑 `vite build --watch`
而非 `vite` dev server，避免懒编译瀑布。`vite` dev server 适合纯浏览器场景，
在 Tauri WebView 中因每次冷启动都触发全量首次编译，体验较差。
