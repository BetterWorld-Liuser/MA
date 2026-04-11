# Tauri Dev 启动黑屏约 1 分钟 Issue Report

> 阶段：阶段一（问题报告）
> 状态：已确认
> 创建日期：2026-04-11
> 严重程度：P1

## 1. 问题现象

执行 `npm run tauri:dev` 后，cargo 编译完成（或增量跳过）、`Running march-ui.exe` 启动后，UI 窗口持续黑屏约 **1 分钟**，期间 Node.js 进程 CPU 占用率高，直到 Vite dev server 完全准备好后界面才渲染出来。

终端日志示例（第二次启动，增量编译跳过）：

```
Running DevCommand (`cargo run --no-default-features --color always --`)
Info Watching D:\playground\MA\src-tauri for changes...
Info Watching D:\playground\MA\crates\march-core for changes...
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.79s
Running `D:\playground\MA\target\debug\march-ui.exe`
```

此后窗口黑屏，约 1 分钟后 UI 才出现。

## 2. 复现步骤

1. 在项目根目录执行 `npm run tauri:dev`
2. 等待 cargo 编译完成（首次约 22s，后续增量约 0.79s）
3. 看到 `Running march-ui.exe` 输出
4. 观察到：UI 窗口为黑屏，持续约 1 分钟；期间 Node.js 进程 CPU 占用高
5. 约 1 分钟后 UI 渲染完成，Node.js CPU 恢复正常

复现频率：**稳定复现**（每次启动均如此，无论是否改动代码）

## 3. 期望 vs 实际

**期望行为**：`Running march-ui.exe` 启动后，Vite dev server 应在数秒内（如 5-10 秒）准备完成，WebView 随即渲染出 UI 界面。

**实际行为**：Vite dev server 冷启动需要约 1 分钟，WebView 在此期间显示黑屏，开发者每次重启都需等待。

## 4. 环境信息

- 涉及模块/功能：Tauri dev 启动流程、Vite dev server 冷启动
- 相关文件/函数：
  - `tauri.conf.json`：`beforeDevCommand: "npm run dev"`，`devUrl: "http://localhost:5173"`
  - `vite.config.ts`：Vue 3 + `@tailwindcss/vite`（Tailwind CSS v4）插件
  - `package.json`：依赖包括 reka-ui、markstream-vue、@iconify、lucide-vue-next 等
- 运行环境：dev
- 其他上下文：
  - OS：Windows 11 Pro for Workstations
  - Node.js 作为 Vite 宿主，`beforeDevCommand` 由 Tauri 在后台启动
  - Vite 6，Tailwind CSS v4（`@tailwindcss/vite`），依赖量较大
  - `manualChunks` 配置仅对 build 生效，对 dev 冷启动无帮助

## 5. 严重程度

**P1 - 严重** — 核心开发工作流受损，每次重启均需等待约 1 分钟黑屏，严重影响开发效率；功能本身不受影响，但开发体验不可接受。

## 备注

- 首次启动额外包含 cargo 编译约 22 秒（`march-core` + `march-ui`），属正常增量编译行为，非本 issue 核心问题
- 已有快捷命令 `tauri:dev:reuse-web`（`tauri dev --no-dev-server`）可跳过 Vite 重启，但前提是 Vite 已在运行，不能根本解决冷启动慢的问题
