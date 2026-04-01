# 工具设计

> 从 [DESIGN.md](DESIGN.md) 延伸：命令执行是核心通用能力，但基础文件读写必须是一级工具，不能完全退化为“全靠 shell 拼接”。

## 工具分层

Ma 的工具层分成两类：

### 1. 环境工具：`run_command`

用于调用工作区外部能力，例如：

- 编译、测试、lint
- `git`、`grep`、构建脚本
- 调用用户本机已有的 CLI 工具

`run_command` 的价值在于通用性，它让 AI 可以借助现有开发环境做事，而不是为每个能力都造专用工具。

建议接口形态：

```rust
run_command {
    shell: CommandShell,
    command: String,
}
```

其中 `shell` 可以显式指定执行环境，例如：

- `bash`
- `sh`
- `powershell`
- `cmd`

但这个枚举只是“可能的 shell 类型”，不是说每个环境里都一定有这些程序。
Ma 在会话启动时应先扫描当前环境里实际可运行的 shell，再把结果注入给 AI。

把 shell 作为显式参数，同时把“当前环境下哪些 shell 可用”和“当前工作目录是什么”作为运行时信息注入提示词，有几个直接收益：

- AI 可以根据命令语法选择正确环境，避免把 PowerShell 语法发到 `cmd`
- AI 不会误用当前机器上根本不存在的 shell
- AI 不需要重复传 `working_directory`，减少无意义样板
- tool call 记录更完整，用户能看见“命令在哪个环境里执行”
- 后续做权限控制、审计、重放和跨平台兼容时，输入边界更稳定

注意这里的 `shell` 是“命令由哪个解释器执行”，不是要把环境抽象成更高层任务语义。

建议在 tool usage prompt 中注入类似信息：

```text
run_command available shells in this session:
- powershell
- cmd
- bash

Current working directory: /workspace/project
Only choose from the shells listed above.
```

### 2. 文件工具：`open_file` / `close_file` / `write_file` / 行号级编辑

没有 `read_file`——打开即追踪，上下文里的内容永远反映磁盘真实状态：

- `open_file(path)`：将文件纳入上下文，watcher 开始实时追踪；内容带行号注入到 [Open 文件列表] 层
- `close_file(path)`：从上下文移除该文件，停止追踪
- `write_file(path, content)`：整文件写入，适合新建文件或明确覆盖
- `replace_lines(path, start_line, end_line, new_content)`
- `insert_lines(path, after_line, new_content)`
- `delete_lines(path, start_line, end_line)`

**文件内容限制**：渲染进上下文时，三个维度取先到者：

- **二进制检测**：扫描前 8KB，发现 null byte 即拒绝 `open_file`，返回错误给 AI
- **按行截断**：超过 2,000 行，截断剩余行
- **按字符截断**：单行超过 1,000 字符，截断该行并附 `…[+X chars]`
- **总量兜底**：渲染内容超过 100KB，停止添加新行

截断时在文件头部注入说明，例如：

```
[文件共 8,432 行，仅显示前 2,000 行。如需查看其他部分，请用 run_command 配合 grep / head / tail。]
```

单行超长文件（minified JS、单行 JSON dump 等）无法靠扩展名可靠识别，统一走渲染时截断，AI 见到截断提示自然会换方案。截断逻辑只影响上下文渲染，不影响 watcher 对文件的真实追踪。

---

这里保留文件专用工具，而不是强迫 AI 用 shell 做所有文件操作，原因有三点：

- 基础读写是高频路径，应该尽量减少 shell 转义、here-doc、平台差异等噪音
- 文件工具直接接入 watcher / snapshot / ModifiedBy 归因逻辑，是 Source of Truth 的一部分
- 命令执行失败时往往混有环境因素；基础文件操作应该尽量确定、可预测、可审计

### 3. 通信工具：`reply`

AI 通过 `reply` 向用户发送消息，这是 AI 与用户交互的唯一出口：

```rust
reply {
    message: String,
    wait: bool,
}
```

- `wait: true`：发送消息后暂停，等待用户下一次输入。这是 AI"完成了这一步"的信号。
- `wait: false`：发送进度更新后继续执行，不等待用户。适合长任务中途汇报状态。

**没有循环，只有决策**：Ma 不设外部循环控制器，不计数工具调用次数，不强制插入检查点。每次 API 返回后，Ma 执行 AI 请求的工具，把结果拼回上下文，然后再发一次 API 请求——如此持续，直到 AI 自己调用 `reply(wait=true)` 为止。"循环"是 AI 行为的自然结果，不是 Ma 强加的控制结构。

**用户中断**：用户点击取消时，Ma 立即断开当前 API 连接。上下文状态（open_files、notes、recent_chat）保持中断前的最新状态，AI 下一轮可以从这个状态继续。

**AI 运行中用户发新消息**：如果 AI 尚未 `reply(wait=true)`，用户发来的新消息暂存。下一轮构建上下文时，新消息会被刷新到 `recent_chat`，AI 自然感知到。Ma 不打断当前正在进行的 API 请求，等它返回后再处理。

---

### 4. Notes 工具：AI 的跨轮工作记忆

工具执行结果默认在当前轮结束后丢弃。AI 可以主动将有价值的内容写入 Notes，使其跨轮持久保留：

- `write_note(id, content)`：新建或覆盖某条 note，id 由 AI 自己约定
- `remove_note(id)`：清除不再需要的条目

典型用法：
- `write_note("target", "当前目标：修复登录模块的 token 刷新逻辑")`
- `write_note("build_output", "cargo build 输出：error[E0502] ...")`
- `remove_note("build_output")`  ← 问题解决后清除

---

## 文件修改：按行号操作

AI 拿到的文件内容始终带行号，修改时通过行号精确定位，无需匹配文本内容。

```
replace_lines(path, start_line, end_line, new_content)
insert_lines(path, after_line, new_content)
delete_lines(path, start_line, end_line)
```

**优点：**
- 无匹配失败问题（行号是绝对定位）
- AI 只需输出改动的行，token 消耗小
- 人类审查时直观易懂
- 纯 Rust 实现，不依赖任何外部工具

**风险与对策：**
AI 读取文件后、执行替换前，如果用户手动修改了文件导致行号错位 → watcher 检测到文件变动，执行前自动报警并重新提供最新内容给 AI。

---

## Source of Truth 一致性

无论文件是通过 `write_file` / 行号编辑修改，还是通过 `run_command` 间接修改，最终都必须回到同一套文件状态归因流程：

1. 写入磁盘
2. watcher 感知变化
3. 刷新对应 `FileSnapshot`
4. 标记 `ModifiedBy`
5. 下一轮上下文基于最新快照重新构建

也就是说，文件工具不是绕开 watcher 的捷径，而是 watcher 生态的一部分。

---

## 命令执行归因

命令执行期间发生的文件变动，通过时间窗口归因为 `ModifiedBy::Agent`。

但这只是“间接修改文件”的归因方式，不应覆盖直接文件工具的价值。对 Ma 来说：

- 能直接表达为文件操作的，就优先走文件工具
- 只有确实需要外部环境能力时，才走 `run_command`

---

## Shell 选择原则

`run_command` 应把 shell 选择视为用户/模型可见的显式决策，而不是内部黑盒：

- 会话启动时扫描当前环境，把可用 shell 列表写入工具提示词
- 当前工作目录也应直接写入工具提示词，而不是要求模型每次显式传参
- 需要 shell 内建语法、管道、重定向、脚本片段时，显式指定对应 shell
- 同一轮上下文里，AI 应尽量保持 shell 风格一致，减少语法来回切换

典型示例：

- `powershell`: `Get-ChildItem src | Select-Object Name`
- `cmd`: `dir src`
- `bash`: `ls src | grep rs`

---

## 错误处理

**网络层**：API 请求失败时自动重试，最多 5 次，指数退避。5 次均失败后向用户报错，不进入 AI 上下文。

**工具层**：工具执行失败（文件不存在、命令非零退出、行号越界等）时，将完整的错误信息作为 tool result 返回给 AI，由 AI 决定如何响应（重试、换方案、向用户说明）。Ma 不在工具层做自动重试。
