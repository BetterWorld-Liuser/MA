# UI 设计：应用壳层与布局

> 本文定义 Ma 的桌面应用壳层、整体视觉风格与三栏布局。聊天区内部交互详见 [ui-chat.md](ui-chat.md)。

---

## 技术选型

| 层 | 技术 |
|----|------|
| 壳 | Tauri（Rust 后端，原生窗口） |
| 前端框架 | Vue 3 |
| 样式 | Tailwind CSS + CSS Variables |

Rust 后端通过 Tauri command / event 与前端通信：
- **command**：前端主动调用后端（发送消息、open_file、lock 文件等）
- **event**：后端主动推送到前端（AI 流式输出、tool 生命周期、watcher 变更、上下文用量更新等）

事件流是 UI 实时反馈的基础，因此等待态和工具状态都应建立在 event 上，而不是前端自行猜测。

---

## 视觉风格

参考 Claude Code 官网风格：低饱和度边框、橙色 accent、桌面应用密度优先。默认主题为深色，但壳层必须支持浅色主题切换；两套主题共用同一组语义化 CSS 变量，而不是在组件里分叉写颜色常量。桌面 UI 文字默认使用无衬线字体，代码、路径、命令等技术内容再使用等宽字体。

颜色通过 CSS 变量定义在 `:root`，并允许通过 `data-theme` 覆盖同名变量。Tailwind theme 直接引用这些变量。这样调色时只改 CSS 变量，Tailwind 工具类自动生效，无需改 config。

### CSS 变量（`src/styles/vars.css`）

```css
:root {
  /* 背景 */
  --color-bg:           #0a0a0a;
  --color-bg-secondary: #111111;
  --color-bg-tertiary:  #1a1a1a;
  --color-bg-hover:     #222222;

  /* 边框 */
  --color-border:       #2a2a2a;
  --color-border-focus: #444444;

  /* 文字 */
  --color-text:         #e8e8e8;
  --color-text-muted:   #888888;
  --color-text-dim:     #555555;

  /* Accent */
  --color-accent:       #d4692a;
  --color-accent-hover: #e07a3a;
  --color-accent-dim:   rgba(212, 105, 42, 0.15);

  /* 语义色 */
  --color-warning: #e6a817;
  --color-error:   #e05252;
  --color-success: #4caf7d;

  /* 字体 */
  --font-sans: system-ui, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
  --font-mono: "JetBrains Mono", "Berkeley Mono", "Fira Code", ui-monospace, monospace;

  /* 圆角 */
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 10px;

  /* 滚动条 */
  --scrollbar-width: 4px;
}
```

### Tailwind 配置（`tailwind.config.js`）

```js
export default {
  content: ['./src/**/*.{vue,ts}'],
  theme: {
    extend: {
      colors: {
        bg:        'var(--color-bg)',
        'bg-secondary': 'var(--color-bg-secondary)',
        'bg-tertiary':  'var(--color-bg-tertiary)',
        'bg-hover':     'var(--color-bg-hover)',
        border:         'var(--color-border)',
        'border-focus': 'var(--color-border-focus)',
        text:           'var(--color-text)',
        'text-muted':   'var(--color-text-muted)',
        'text-dim':     'var(--color-text-dim)',
        accent:         'var(--color-accent)',
        'accent-hover': 'var(--color-accent-hover)',
        'accent-dim':   'var(--color-accent-dim)',
        warning:        'var(--color-warning)',
        error:          'var(--color-error)',
        success:        'var(--color-success)',
      },
      fontFamily: {
        sans: 'var(--font-sans)',
        mono: 'var(--font-mono)',
      },
      borderRadius: {
        sm: 'var(--radius-sm)',
        md: 'var(--radius-md)',
        lg: 'var(--radius-lg)',
      },
    },
  },
}
```

---

## 窗口结构

桌面端默认隐藏原生标题栏，改用应用内部自绘标题栏：

- 左侧显示品牌与当前任务
- 中间为空白拖拽区
- 右侧提供最小化、最大化、关闭按钮
- 窗口控制按钮使用明确的图标，不使用字符充当按钮
- 标题栏高度保持紧凑，尽量接近原生桌面窗口密度

这样能让窗口控制和应用视觉语言统一，避免原生标题栏把界面切成两层。

---

## 三栏布局

```text
┌──────────────────┬──────────────────────┬────────────────────┐
│    任务列表        │       聊天区           │    上下文面板         │
│    260px         │       flex-1         │    320px           │
└──────────────────┴──────────────────────┴────────────────────┘
```

三栏宽度：左栏固定 260px，右栏固定 320px，中栏自适应填满剩余空间。左栏和右栏可折叠。

布局密度应更接近 IDE / 桌面工具而不是 landing page：
- 顶部状态区保持低高度，不做横幅式 hero 卡片，也不堆技术实现标签
- 列表项、面板 header、输入区 padding 优先紧凑
- 同一屏内优先展示更多任务、更多消息和更多上下文状态
- 避免同义信息上下重复出现，例如 section 名称和大标题重复表达同一概念

---

## 左栏：任务列表

```text
┌──────────────┐
│  Ma          │
│──────────────│
│▶ 重构认证模块 │
│  添加支付集成 │
│  修复登录 bug │
│──────────────│
│  + 新建任务  │
│──────────────│
│       ⚙      │
└──────────────┘
```

- 头部采用图标 + 单行标题，不重复显示 `WORKSPACE` / `TASKS` 这类语义已明显的标签
- 任务名单行截断，hover 显示完整名
- 右键菜单：重命名、归档
- 正在运行的任务显示 spinner
- 多任务并行时各自独立，切换即切换上下文
- 左下角固定设置入口，作为全局应用配置入口，不随任务滚动

---

## 右栏：上下文面板

这是 Ma 的差异化区域，把 AI 内部状态投影成可操作的界面元素。

### 笔记区

```text
[笔记]
target   当前目标：拆分 auth 模块     [编辑] [×]
plan     1. 读现有结构 2. 拆接口层     [编辑] [×]
                                       [+ 新建]
```

- 笔记以紧凑单行列表展示：`id · content · actions`
- 显示 id 和内容摘要，id 弱化为辅助标签，内容本身优先，不把单条 note 做成大卡片
- 用户删除 = AI 下一轮收不到这条笔记
- 用户新建 = 给 AI 留一条背景信息

### 监控文件区

```text
[监控文件]
  auth.rs            14:32
  lib.rs             14:28
  models.rs          11:05
  prod.toml      🔒  09:11
                     [+ 打开文件]
```

- 顺序与 AI 实际收到的上下文顺序一致
- 面板头采用图标 + 单行标题，不使用 `CONTEXT` / `Live state` 这类重复双层标题
- 默认展示文件名，完整路径通过 hover tooltip 或后续详情查看，避免路径噪音占满右栏
- 条目尽量压成单行紧凑高度，不额外显示时间，避免右栏被稀释
- 文件名使用等宽字体，保持技术对象的扫描感
- 时间戳与文件名同列紧凑展示，不另起一行
- 文件新旧感仅通过文字饱和度表达，不额外占一行展示辅助信息
- 左侧 close 图标只在 hover 时出现，hover 时高亮并轻微放大；locked 文件不显示 close
- 右侧始终保留锁位：未锁定显示开锁图标，已锁定显示闭锁图标，保证用户随时可以点击切换锁定状态
- 不额外显示 `HIGH / MID / LOW` 一类状态标签，避免把“新旧感”误做成按钮语义
- 右键菜单：关闭文件、锁定/解锁
- `+ 打开文件` 弹出文件选择器
- 文件刚被读取、写入或 watcher 检测到变化时，可短暂高亮对应条目，帮助用户把聊天区事件和右栏上下文联系起来

### 提示区（Hints）

```text
[提示]
[Telegram] foo: 部署好了吗？    4m32s · 3轮  [×]
[CI] main 构建失败 exit 1       12m08s       [×]
```

- 显示内容、剩余时间、剩余轮次
- 空状态默认折叠，不占整块面板高度
- 用户可手动关闭，不提供手动新建

### 上下文用量区

```text
[上下文用量]
████████░░░░░░░░  42%  10.2k / 24k

文件  ██████  6.1k
笔记  █       0.8k
提示  ░       0.1k
对话  ██      2.1k
系统  █       1.2k
```

- 整体压缩为 2-3 行：一行标题与总量、一行进度条、一行紧凑 breakdown
- 不使用 `CURRENT / BUDGET` 大卡片，避免重复占高
- 用量 > 80% 时进度条变为 `--color-warning` 色
- 用量 > 95% 时变为 `--color-error` 色
- 在新一轮上下文构建完成后实时刷新，让用户看到“这轮 AI 实际拿到了多少上下文”

---

## 设置页

通过左栏底部固定的设置图标进入，覆盖整个窗口。设置页本身是一个稳定的应用级容器，不只是某一项配置的弹层。

当前应包含两个一级分区：

- **外观**：至少提供深色 / 浅色主题切换；切换后立即作用于当前窗口并持久化到本地
- **Provider**：新增/编辑/删除 provider，填写 name、api_key、base_url，保存后可测试连通性，并配置默认模型

这样做的原因是：

- 主题切换属于应用壳层能力，不能塞进任务区或聊天区临时按钮
- provider 与外观都属于“全局设置”，应该共用一个入口，但在设置页内部按职责分区，避免混成单页长表单
