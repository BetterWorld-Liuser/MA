# Tauri Dev 启动黑屏约 1 分钟 根因分析

> 阶段：阶段二（根因分析）
> 状态：草稿
> 分析日期：2026-04-11
> 关联问题报告：easysdd/issues/2026-04-11-tauri-dev-slow-startup/report.md

## 1. 问题定位

| 关键位置 | 说明 |
|---|---|
| `tauri.conf.json:7` | `beforeDevCommand: "npm run dev"` — Tauri 后台启动 Vite，但 Tauri 只等 HTTP 端口可达（不等模块编译完成）就打开 WebView |
| `vite.config.ts:11` | `plugins: [vue(), tailwindcss()]` — 无 `server.warmup` 配置，Vite 以全懒加载模式工作 |
| `src/styles/main.css:1` | `@import 'tailwindcss'` — 44KB CSS 文件，`@tailwindcss/vite` 首次请求时需扫描所有源文件生成 CSS |
| `src/main.ts:7` | 入口引入 `./styles/main.css`，触发整个模块图的级联加载 |
| `node_modules/.vite/deps/reka-ui.js` | 1.47MB 预打包文件，首次请求需磁盘读取 + 解析 |
| `node_modules/.vite/deps/lucide-vue-next.js` | 1MB 预打包文件，仅 2 个组件使用，与 `@iconify-icons/lucide`（18 个组件）并存，形成双图标库 |

## 2. 失败路径还原

**正常路径（期望）**：
`npm run tauri:dev` → 前端构建产出静态文件 → Tauri 加载静态 bundle → 页面秒级渲染

**实际失败路径**：
`npm run tauri:dev` → Vite dev server 启动（470ms，HTTP 可用）→ Tauri 检测到 5173 可达后立即打开 WebView → WebView 发起 ES module 请求 → Vite **按需懒编译** 80+ 个 `.vue`/`.ts` 文件 → 所有请求 pending，约 1 分钟后全部完成 → 页面渲染

诊断确认（`console.time` 打点）：
- Vite dev server 本身 ready：470ms（正常）
- 从导航到 JS 开始执行：约 1 分钟（全在浏览器等 Vite 编译）
- JS 开始执行后到 app mounted：45ms（极快，不是瓶颈）
- `initialize()` + IPC：约 1.5s（不是瓶颈）
- Chrome 和 WebView2 表现一致 → 问题在 Vite，不在 WebView2

**分叉点**：使用 Vite dev server（懒编译）—— Vite dev 模式不预先打包源码，每个文件是独立的 HTTP 请求 + 按需编译，80+ 文件首次全量编译约需 1 分钟。

## 3. 根因

**根因类型**：配置/环境

**根因描述**：
Vite dev server 的**懒编译（on-demand compilation）**模式：所有 `.vue`/`.ts` 源文件在 WebView 发出 HTTP 请求时才逐个编译。80+ 个源文件首次编译约需 1 分钟（实测），编译结果缓存于内存，第二次访问瞬间响应。`server.warmup` 配置虽然方向正确，但 warmup 本身也需要 1 分钟，且 WebView 在 Vite "ready"（470ms）后立即连接，warmup 来不及完成。

根本解法：改用 `vite build --watch` 替代 `vite`。构建模式预先产出打包好的 bundle，WebView 加载静态文件，彻底消除请求瀑布。`vite build` 实测耗时 8.75s（797 modules），首次启动等待从 1 分钟降至约 9 秒。

**是否有多个根因**：否，单一根因（dev server 懒编译）。原分析中 `server.warmup` 和 `lucide-vue-next` 属于改动机会，非根因。

## 4. 影响面

- **影响范围**：仅影响开发冷启动体验，功能不受影响
- **潜在受害模块**：无
- **数据完整性风险**：无
- **严重程度复核**：维持 P1

## 5. 修复方案

### 方案 A（最终采用）：改用 `vite build --watch` 替代 `vite` dev server

- **做什么**：`tauri.conf.json` 的 `beforeDevCommand` 改为 `npm run dev:build-watch`（= `vite build --watch`），移除 `devUrl`，Tauri 直接加载 `frontendDist`（`dist/`）
- **优点**：彻底消除请求瀑布；首次启动等待 8.75s（vs 1 分钟）；后续代码变更触发增量重建（1-5s）+ Tauri 热重载
- **缺点/风险**：失去 HMR（热模块替换），代码变更后是全页刷新；对桌面 app 可接受
- **影响面**：`tauri.conf.json`、`package.json`

### 方案 B（已实施，机会性清理）：移除 `lucide-vue-next` 重复依赖

- **做什么**：2 个 Dialog 组件改用 `@iconify/vue`，移除 1MB 重复包
- **影响面**：2 个 Dialog 组件 + `package.json`

### 推荐方案

**方案 A**，同时保留方案 B 的清理。

## 6. 修复记录

- **实际采用方案**：方案 A（`vite build --watch`）+ 方案 B（移除 `lucide-vue-next`）
- **改动文件清单**：
  - `tauri.conf.json` — `beforeDevCommand` 改为 `npm run dev:build-watch`，移除 `devUrl`
  - `package.json` — 新增 `"dev:build-watch": "vite build --watch"`，移除 `lucide-vue-next` 依赖
  - `vite.config.ts` — `server.warmup` 改为 glob 覆盖全部源文件（辅助优化，非主修复）
  - `src/components/ui/dialog/DialogContent.vue` — `lucide-vue-next` → `@iconify/vue`
  - `src/components/ui/dialog/DialogScrollContent.vue` — 同上
  - `src-tauri/tauri.reuse-web.conf.json` — 新增（覆盖 `beforeDevCommand` 为空，供复用已有 dist 场景使用）
- **验证结果**：
  - TypeScript typecheck：通过 ✓
  - 复现步骤验证：待用户跑 `npm run tauri:dev` 确认启动时间 ≤ 10s
  - 影响面回归：Dialog 关闭按钮视觉不变（同款 lucide x 图标）
- **遗留事项**：
  - 改用 build 模式后失去 HMR，代码变更为全页刷新；对桌面 app 可接受
  - 若 Tauri 打开 WebView 时 first build 尚未完成，可能短暂白屏（约 9s 后自动刷新），可接受
