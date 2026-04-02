# UI 设计

> 基于 Tauri 实现，Rust 后端 + Web 前端。风格参考 Claude Code 官网：深色、等宽字体、极简。

---

## 技术选型

| 层 | 技术 |
|----|------|
| 壳 | Tauri（Rust 后端，原生窗口） |
| 前端框架 | Vue 3 |
| 样式 | Tailwind CSS + CSS Variables |

Rust 后端通过 Tauri command / event 与前端通信：
- **command**：前端主动调用后端（发送消息、open_file、lock 文件等）
- **event**：后端主动推送到前端（AI 流式输出、watcher 变更、上下文用量更新等）

---

## 视觉风格

参考 Claude Code 官网风格：深色背景、等宽字体、低饱和度边框、橙色 accent。

颜色通过 CSS 变量定义在 `:root`，Tailwind theme 直接引用这些变量——两层分离的好处是：调色时只改 CSS 变量，Tailwind 工具类自动生效，无需改 config。

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

  /* Accent（橙色，参考 Anthropic 品牌色） */
  --color-accent:       #d4692a;
  --color-accent-hover: #e07a3a;
  --color-accent-dim:   rgba(212, 105, 42, 0.15);

  /* 语义色 */
  --color-warning: #e6a817;
  --color-error:   #e05252;
  --color-success: #4caf7d;

  /* 字体 */
  --font-mono: "Berkeley Mono", "JetBrains Mono", "Fira Code", ui-monospace, monospace;

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

这样在 Vue 组件里直接用 Tailwind 工具类：`bg-bg-secondary`、`text-text-muted`、`border-border` 等，后续改色只动 `vars.css`。

---

## 三栏布局

```
┌──────────────┬──────────────────────────┬──────────────────┐
│   任务列表    │         聊天区            │   上下文面板      │
│   200px      │         flex-1           │   280px          │
└──────────────┴──────────────────────────┴──────────────────┘
```

三栏宽度：左栏固定 200px，右栏固定 280px，中栏自适应填满剩余空间。左栏和右栏可折叠。

---

## 左栏：任务列表

```
┌──────────────┐
│  March         │  ← 标题/logo，点击打开设置
│──────────────│
│▶ 重构认证模块 │  ← 当前活跃任务，高亮
│  添加支付集成 │
│  修复登录 bug │
│──────────────│
│  + 新建任务  │
└──────────────┘
```

- 任务名单行截断，hover 显示完整名
- 右键菜单：重命名、归档
- 正在运行的任务显示 spinner
- 多任务并行时各自独立，切换即切换上下文

---

## 中栏：聊天区

```
┌──────────────────────────────────┐
│                                  │
│  用户                    14:32   │
│  帮我把 auth 模块拆成更小的单元   │
│                                  │
│  March                     14:32   │
│  好的，先看一下现有结构……         │
│  ┌─ open_file src/auth.rs       │
│  ├─ replace_lines 12-30         │
│  └─ reply ✓                     │
│                                  │
│  March                     14:33   │
│  已完成，auth 模块拆成了三个文件  │
│                                  │
├──────────────────────────────────┤
│  输入框                    [发送] │
└──────────────────────────────────┘
```

- 完整对话历史，用户可以翻回去
- 工具调用以折叠摘要内联展示，默认收起，点击展开原始 input/output
- AI 输出流式渲染
- 输入框支持 `@文件路径` 触发 open_file
- 输入框 `Shift+Enter` 换行，`Enter` 发送

---

## 右栏：上下文面板

这是 March 的差异化区域，把 AI 内部状态投影成可操作的界面元素。

### 笔记区

```
[笔记]
target   当前目标：拆分 auth 模块     [编辑] [×]
plan     1. 读现有结构 2. 拆接口层     [编辑] [×]
                                       [+ 新建]
```

- 显示 id 和内容摘要，点击编辑直接修改
- 用户删除 = AI 下一轮收不到这条笔记
- 用户新建 = 给 AI 留一条背景信息

### 监控文件区

```
[监控文件]
  src/auth.rs        14:32  ← 高饱和
  src/lib.rs         14:28  ← 高饱和
  src/models.rs      11:05  ← 中饱和
🔒 config/prod.toml  09:11  ← 低饱和
                     [+ 打开文件]
```

- 顺序与 AI 实际收到的上下文顺序一致
- 时间戳用颜色饱和度传达新旧感，越新越鲜艳
- 锁定图标表示 locked，close 操作被禁用
- 右键菜单：关闭文件、锁定/解锁
- `+ 打开文件` 弹出文件选择器

### 提示区（Hints）

```
[提示]
[Telegram] foo: 部署好了吗？    4m32s · 3轮  [×]
[CI] main 构建失败 exit 1       12m08s       [×]
```

- 显示内容、剩余时间、剩余轮次
- 用户可手动关闭，不提供手动新建

### 上下文用量区

```
[上下文用量]
████████░░░░░░░░  42%  10.2k / 24k

文件  ██████  6.1k
笔记  █       0.8k
提示  ░       0.1k
对话  ██      2.1k
系统  █       1.2k
```

- 用量 > 80% 时进度条变为 `--color-warning` 色
- 用量 > 95% 时变为 `--color-error` 色

---

## 设置页

通过左栏标题点击进入，覆盖整个窗口。包含：

- **Provider 配置**：新增/编辑/删除 provider，填写 name、api_key、base_url，保存后可测试连通性
- **默认模型**：从已配置的 provider 里选
- **外观**：字体大小、面板折叠状态等（MVP 后）

---

## 设计原则

**全等宽字体**：代码和 UI 文字统一用 `--font-mono`，不混用衬线字体。

**透明但不强迫**：右侧面板默认展开，可折叠，不感兴趣的用户只看聊天区。

**操作即指令**：右侧面板的操作直接修改 AI 下一轮收到的上下文，等价于用自然语言告诉 AI，但更精确。
