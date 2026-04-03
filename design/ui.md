# UI 设计

> 基于 Tauri 实现，Rust 后端 + Web 前端。风格参考 Claude Code 官网：深色、极简，但桌面 UI 文字使用无衬线字体，代码/路径再局部使用等宽字体。整体密度偏桌面应用而非营销页，避免过高标题栏、超大按钮和过度装饰。

当前实现进度见 → [Working/ui-status.md](Working/ui-status.md)

---

## 文档拆分

UI 设计拆为“总览 + 子系统文档”，避免把布局、聊天交互、上下文面板和状态反馈混在一份长文里：

- [ui-shell.md](ui-shell.md)：应用壳层、视觉风格、窗口结构、三栏布局、左右侧面板、设置页
- [ui-chat.md](ui-chat.md)：聊天区、消息流、等待态、工具调用展示、输入框交互
- [ui-events.md](ui-events.md)：聊天运行事件模型、前后端状态边界、右栏联动草案
- [ui-task-naming.md](ui-task-naming.md)：task 的延迟命名、首轮自动命名和标题来源规则

`ui.md` 只保留总览和文档边界；具体实现细节写入子文档，避免多个地方重复定义同一交互。

---

## UI 目标

UI 必须服务于 Ma 的核心设计，而不是单独演化出一套“看起来像 AI 工具”的界面风格：

- 聊天区负责承载完整用户视图，让用户持续看到完整对话与当前进展
- 右侧面板负责投影 AI 下一轮真正会收到的上下文，让“上下文管理”变成可见、可操作对象
- 等待态、工具反馈、文件变化提示都应基于真实 agent 事件，而不是纯装饰性动画

这延续了 [`DESIGN.md`](DESIGN.md) 中“用户视图 vs AI 上下文分离”的原则：

```text
用户看到的                    AI 收到的
─────────────────            ─────────────────
完整聊天记录                  精简后的上下文
完整执行过程提示              当前轮真实工作状态
工具调用摘要                  当轮工具结果
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
│  March         │
│──────────────│
│▶ 重构认证模块 │  ← 当前活跃任务，高亮
│  添加支付集成 │
│  修复登录 bug │
│──────────────│
│  + 新建任务  │
│──────────────│
│       ⚙      │  ← 左下角全局设置入口
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
│  └─ run_command cargo test      │
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
  src/auth.rs        2.8k tok  ← 高饱和
  src/lib.rs         1.9k tok  ← 高饱和
  src/models.rs      0.9k tok  ← 中饱和
🔒 config/prod.toml  0.3k tok  ← 低饱和
                     [+ 打开文件]
```

- 顺序与 AI 实际收到的上下文顺序一致
- 次信息优先展示该文件的估算 token 消耗，而不是时间戳
- 文件名的文字饱和度仍可作为轻量辅助视觉，用来区分 locked / 非 locked 或内容状态，但不单独渲染时间
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
████████░░░░░░░░  42%  10.2k / 128k

文件  ██████  6.1k
笔记  █       0.8k
提示  ░       0.1k
对话  ██      2.1k
系统  █       1.2k
```

- UI 展示的是**估算 token 用量**，不是字节数
- 总预算优先取当前模型的真实 context window；拿不到时再回退到本地能力表或默认值
- 用量 > 80% 时进度条变为 `--color-warning` 色
- 用量 > 95% 时变为 `--color-error` 色

---

## 设置页

通过左栏底部固定的设置图标进入，覆盖整个窗口。设置属于应用级能力，不挂在任务标题、聊天区或右侧上下文面板上，避免把全局配置与任务上下文混淆。

MVP 包含：

- **Provider 配置**：新增/编辑/删除 provider，填写 name、api_key、base_url
- **默认模型**：选择默认 provider 对应的默认 model
- **外观**：字体大小、面板折叠状态等（MVP 后）

---

## 设计原则

**界面与代码字体分层**：UI 文字默认使用无衬线字体，代码、路径、命令等技术内容使用等宽字体。

**状态与操作分离**：如果一个元素只是状态提示，就不要把它做成看起来像按钮的样子。

**透明但不强迫**：用户应该能看到 AI 正在做什么，但默认看到的是摘要而不是调试日志全文。

**操作即指令**：右侧面板的编辑、删除、锁定等操作直接影响 AI 下一轮收到的上下文。

**等待必须可感知**：用户发送消息后，界面应持续反馈“当前轮正在推进”，但只展示真实事件，不伪造思考痕迹。
