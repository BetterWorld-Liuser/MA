# UI 当前实现进度

> 本文档记录当前仓库里的 UI 实现状态，用来区分“设计目标”和“已经落地的部分”。UI 的目标形态仍以 [`../ui.md`](../ui.md) 为准。

相关专题设计：

- [`ui-debug-panel.md`](ui-debug-panel.md)：聊天过程中的原始上下文、provider 输入输出、tool call/result 观测面板设计，采用 Debug 内部分 tab 的结构

---

## 已实现项

### 技术栈与工程骨架

- 已搭建 `Tauri + Vue 3 + Tailwind CSS + CSS Variables` 的桌面 UI 工程
- 前端入口、Tauri 壳、Tailwind 配置、主题变量均已接通
- 前端与 Rust 后端已通过 Tauri `invoke` 建立基本调用链

### 视觉与布局

- 已实现深色主题、低饱和边框、橙色 accent 的整体视觉方向
- 已实现 UI 文本与代码/路径内容的字体分层
- 已实现自绘标题栏，包含最小化、最大化、关闭、拖拽
- 标题栏已接入 Iconify 图标，不再使用字符或手搓线条作为窗口控制按钮
- 标题栏高度和图标尺寸已做过一轮收紧，偏向更接近桌面窗口的紧凑感
- 已实现三栏主布局：左侧任务列表、中间聊天区、右侧上下文面板

### 左栏：任务列表

- 已有任务列表组件
- 已支持当前任务高亮
- 已支持任务切换
- 已支持新建任务
- 新建任务已改为直接创建空主题窗口，不再先弹任务名表单
- 首条用户消息发送后，已支持根据该消息自动生成 task 标题
- 已支持手动刷新工作区快照
- 已有基础任务状态标记（active / running / idle）

### 中栏：聊天区

- 已有完整聊天区组件
- 已支持显示历史消息
- 已支持发送消息到后端
- 已支持工具调用摘要的折叠展示
- 已有发送中状态与空状态展示
- 已支持 `Enter` 发送、`Shift+Enter` 换行
- 已改为更明确的双侧对话流：AI 在左，用户在右
- 已移除聊天区顶部冗余的 `Conversation` / `Chat` 双层标题
- agent 层已经产出 `debug_rounds` 调试数据，但当前尚未接入 Tauri UI 展示

### 右栏：上下文面板

- 已展示 `target` note 与普通 notes
- 已支持 note 的新增、编辑、删除
- 已展示 open files 列表
- open files 已移除会误导用户的 `HIGH / MID / LOW` 标签
- open files 已支持 `Lock / Unlock`
- open files 已支持 `Close`
- locked 文件会显示锁标记，且 `Close` 操作会被禁用
- open files 次信息已切换为估算 token 消耗，不再显示时间
- 已展示 hints 列表
- 已展示 context usage 总量与分项
- 已根据 context usage 百分比切换 warning / error 颜色

### 后端支撑

- 已有面向 UI 的 `workspace snapshot` 数据结构
- 已接通 `load_workspace_snapshot`
- 已接通 `create_task`
- 已接通 `select_task`
- 已接通 `send_message`
- 已接通 `upsert_note`
- 已接通 `delete_note`
- 已接通 `toggle_open_file_lock`
- 已接通 `close_open_file`

### 当前验证状态

- `npm run build` 通过
- `cargo check -p ma-ui` 通过
- `cargo check -p ma` 通过

---

## 待实现项

### 布局与页面

- 左栏折叠
- 右栏折叠
- 设置页整体实现
- 从左栏标题进入设置页的完整交互

### 任务列表

- 右键菜单
- 任务重命名
- 任务归档
- 更明确的运行中 spinner 表现
- 多任务并行状态的更细致反馈

### 聊天区

- AI 输出流式渲染
- `@文件路径` 触发 `open_file`
- 工具调用详情展开查看原始 input / output
- 最近一轮的 Debug 调试面板，内部按 `Overview / Context / Request / Response / Tools` 分 tab 展示
- 更完整的错误态、加载态与长对话体验

### 上下文面板

- open files 的右键菜单
- `+ 打开文件` 文件选择器
- hints 手动关闭
- 更接近设计稿的上下文用量可视化
- system status / context pressure 的前端展示

### 设置页内容

- Provider 配置（新增 / 编辑 / 删除 / 测试）
- 默认模型选择
- 外观配置

### 交互体验

- 把当前 `window.prompt` / `window.confirm` 式交互替换成正式 UI
- 更一致的桌面端交互细节和状态反馈
- 聊天区的工具摘要区仍偏“日志块”感，和正文的主次关系还可以继续优化
- open files 目前已有按钮式操作，但还没做成设计稿里的右键菜单 + 更轻量控制

---

## MVP 优先级

### P0：必须先完成

- 设置页最小闭环：至少能配置 provider 和默认模型
- 聊天区流式输出
- 任务切换、发送消息、右侧上下文展示这条主链的稳定性打磨
- open files 的基础操作：打开、关闭、锁定 / 解锁
- notes 编辑从临时弹窗改为正式面板交互

### P1：建议纳入 MVP

- 左右栏折叠
- hints 手动关闭
- 任务重命名
- context usage 的高压提示展示
- 最近一轮调试观测面板，便于排查上下文拼接与 provider 行为
- 更统一的错误态、空态、加载态
- `+ 打开文件`，补齐 open files 的完整操作闭环

### P2：可放到 MVP 后

- 任务归档
- 工具调用完整详情视图
- 外观配置
- 更细的 freshness 视觉编码
- 更完整的标题栏和全局工作区状态信息

---

## 备注

- 当前文档描述的是“仓库里已经实现到哪一步”，不是最终 UI 设计本身。
- UI 目标形态、交互原则、版式和视觉语言仍以 [`../ui.md`](../ui.md) 为准。

---

## 当前偏好

以下偏好来自当前一轮实现和讨论结果，后续调整 UI 时应优先遵守：

- 聊天区应明确表现为“对话”，而不是一组样式相近的日志卡片
- AI 消息固定在左侧，用户消息固定在右侧，这比上下同构卡片更容易区分角色
- 聊天区顶部避免无信息增益的重复标题，例如 `Conversation` / `Chat`
- 调试原始上下文、provider request/response、tool call/result 应放在独立 debug 视图中，不污染普通聊天气泡
- open files 的状态应以“是否 locked”为核心，不应出现 `HIGH / MID / LOW` 这类缺乏明确语义的按钮式标签
- open files 的次信息优先服务于上下文管理，应优先显示估算 token 消耗，而不是时间
- 标题栏控制按钮应使用明确的图标资源，而不是字符或手搓几何线条
- 标题栏应保持紧凑，避免过高、过重，尽量接近桌面原生窗口的密度
- 无衬线字体优先使用 `system-ui, "Segoe UI", Roboto, Helvetica, Arial, sans-serif`
- 当一个信息只是状态而不是操作时，不要把它做成看起来像“可以点击的按钮”
