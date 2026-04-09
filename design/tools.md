# 工具设计

> 从 [DESIGN.md](DESIGN.md) 延伸：命令执行是核心通用能力，但基础文件读写必须是一级工具，不能完全退化为“全靠 shell 拼接”。

## 工具分层

March 的工具层分成两类：

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
    timeout_secs?: u64,
}
```

其中 `shell` 可以显式指定执行环境，例如：

- `bash`
- `sh`
- `powershell`
- `cmd`

但这个枚举只是“可能的 shell 类型”，不是说每个环境里都一定有这些程序。
March 在会话启动时应先扫描当前环境里实际可运行的 shell，再把结果注入给 AI。

把 shell 作为显式参数，同时把“当前环境下哪些 shell 可用”和“当前工作目录是什么”作为运行时信息注入提示词，有几个直接收益：

- AI 可以根据命令语法选择正确环境，避免把 PowerShell 语法发到 `cmd`
- AI 不会误用当前机器上根本不存在的 shell
- AI 不需要重复传 `working_directory`，减少无意义样板
- AI 可以按命令预估时长显式传 `timeout_secs`，避免短命令无限挂起，也避免长命令被过早杀掉
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
Default run_command timeout: 10 seconds
Only choose from the shells listed above.
```

`timeout_secs` 为可选参数；不传时默认 10 秒。命令正常返回时，tool result 需带上实际运行耗时；若超时，March 必须杀掉子进程并返回明确错误，例如“命令在 10 秒后超时被终止”。

### `run_command` 的流式输出协议

为了让聊天区和右栏能够在长时间命令执行时实时反馈，`run_command` 可以在工具执行过程中向 UI 推送 stdout/stderr 更新。但这里必须先固定语义：

- UI 事件里如果携带的是“截至当前的完整 stdout/stderr”，它就是 **snapshot**，不是 delta；命名、注释和前端消费方式都必须按 snapshot 语义设计
- 如果未来改成 delta 协议，也必须显式换名字，不能复用同一个事件结构暗中改变含义
- UI 默认应消费简化后的展示文本；完整原始字节仍由后端保留，用于最终 tool result 和错误报告

实现约束：

- 长时间命令的输出更新不能每次节流都对完整累积 buffer 做全量 decode / ANSI strip / 清洗；必须采用增量 decode、游标缓存或等价方案，避免输出越长越慢
- “正常结束 / 超时 / 取消”三条路径的子进程收尾必须共用同一个 helper，统一处理终止、drain pipe、等待 child、join reader 等步骤
- 若当前没有活跃 flush timer，应使用显式状态表示；不要用缺少注释的超长 sleep sentinel 伪装“禁用计时器”
- Windows 下通过 `taskkill /T /F` 终止进程树时，也必须受 timeout 保护，不能让 cancellation path 本身阻塞 turn 收口

### 2. 文件工具：`open_file` / `close_file` / `write_file` / 行号级编辑

没有 `read_file`——打开即追踪，上下文里的内容永远反映磁盘真实状态：

- `open_file(path)`：将文件纳入上下文，watcher 开始实时追踪；内容带行号注入到 [Open 文件列表] 层
- `close_file(path)`：从上下文移除该文件，停止追踪
- `write_file(path, content)`：整文件写入，适合新建文件或明确覆盖
- `replace_lines(path, start_line, end_line, new_content)`
- `insert_lines(path, after_line, new_content)`
- `delete_lines(path, start_line, end_line)`

**文件修改工具的返回值**：`write_file`、`replace_lines`、`insert_lines`、`delete_lines` 执行成功后，在 tool_result 中返回 unified diff，供 AI 即时验证改动是否符合预期。无需再发一次工具调用重读文件，轮内即可确认结果。diff 超过 200 行时截断，附 `[diff 共 XXX 行，仅显示前 200 行]`。

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

### 3. 用户可见输出

用户最终看到的回复不是某个专门的 `reply` 工具调用，而是**本轮 agent loop 自然结束时产出的 assistant 自然语言文本**。

这意味着：

- 中途出现的工具调用、执行结果、阶段性文本，都只是本轮推进过程的一部分
- 只有当 provider 返回文本且不再包含新的 tool calls 时，这段文本才会被视为本轮最终输出
- turn 的结束条件由 agent loop 是否继续产生 tool calls 决定，而不是由某个单独的“回复工具”决定

**没有循环，只有决策**：March 不设外部循环控制器，不计数工具调用次数，不强制插入检查点。每次 API 返回后，March 执行 AI 请求的工具，把结果拼回上下文，然后再发一次 API 请求——如此持续，直到 provider 返回的结果里不再有新的 tool calls 为止。"循环"是 AI 行为的自然结果，不是 March 强加的控制结构。

**用户中断**：用户点击取消时，March 立即断开当前 API 连接。上下文状态（open_files、notes、recent_chat）保持中断前的最新状态，AI 下一轮可以从这个状态继续。

**AI 运行中用户发新消息**：如果当前 turn 尚未自然结束，用户发来的新消息暂存。下一轮构建上下文时，新消息会被刷新到 `recent_chat`，AI 自然感知到。March 不打断当前正在进行的 API 请求，等它返回后再处理。

### 轮内消息历史与轮间清理

**两个层次的"历史"需要区分清楚：**

- **轮内消息历史**：从用户发消息到本轮不再产生新的 tool calls、agent loop 自然结束之间，agent loop 产生的所有 API 交互——中间 assistant 消息、tool_calls、tool_results——构成本轮的消息历史，每次 API 请求都带上完整的轮内历史以维持连贯性。
- **recent_chat**（跨轮）：只记录外层对话：用户消息 + 本轮最终 assistant 输出，最近 10 轮，并携带每条消息时间。轮内的中间过程不进入 `recent_chat`。

**`tool_calls.is_empty()` + `message.content` 的处理**：如果 API 返回了文本内容且没有工具调用，当前实现会将这段文本视为**本轮最终回复**。Ma 的处理方式：

1. 将该 assistant 消息追加到轮内消息历史
2. 将其作为对用户的最终输出持久化
3. 结束当前 turn，不再继续 agent loop

这与早期“`reply` 是唯一出口”的设想不同。当前代码的真实语义是：**是否结束由 tool calls 是否继续出现决定**，最终 assistant 文本只是“自然结束时的产物”，不是一个独立控制信号。

**turn 自然结束后的清理**：轮内消息历史整块丢弃——AI 的中间思考、工具调用记录、执行结果全部不保留。`recent_chat` 追加一条外层对话记录，下一轮从重新构建的 system prompt 上下文 + recent_chat 启动。

**轮内历史不做滚动窗口**：轮内连贯性不可截断——AI 刚 `open_file` 之后执行的 `replace_lines` 必须能看到之前的上下文。如果轮内 token 用量过高，上下文压力机制会提示 AI 主动收缩 `open_files` 和 Notes，但轮内历史本身不裁剪。轮内历史天然有生命周期：本轮一旦自然结束即整块丢弃，不会跨轮累积。

---

### 4. Notes 工具：AI 的跨轮工作记忆

工具执行结果默认在当前轮结束后丢弃。AI 可以主动将有价值的内容写入 Notes，使其跨轮持久保留：

- `write_note(id, content)`：新建或覆盖某条 note，id 由 AI 自己约定
- `remove_note(id)`：清除不再需要的条目

`write_note` 的关键语义是 **upsert**：

- 如果 `id` 不存在，就创建新 note
- 如果 `id` 已存在，就直接用新内容覆盖旧内容
- 因此 AI 在记录同一类事实时，应优先复用稳定 id，而不是为相近内容不断发明新 id

这条规则尤其重要，因为 Notes 会直接进入后续轮次上下文。如果 AI 把“当前目标”“用户身份”“最近一次构建错误”这类本应单点更新的信息拆成多条相似 note，后续上下文会同时出现多份接近但不完全一致的描述，增加歧义和 token 浪费。

推荐把 note id 视为“一个长期槽位”而不是一次性标签。例如：

- `target`：当前任务目标；目标变化时直接覆盖
- `user_identity`：用户身份或角色设定；理解修正时直接覆盖
- `build_output`：最近一次仍然相关的构建错误；重新构建后直接覆盖，问题解决后 remove
- `plan`：当前有效计划；计划调整时直接覆盖

不推荐这种做法：

- `target_v2`
- `latest_target`
- `boss_identity_new`

除非这些内容确实需要并列长期保留，否则应直接覆盖原 id 对应的 note。

典型用法：
- `write_note("target", "当前目标：修复登录模块的 token 刷新逻辑")`
- `write_note("target", "当前目标：补充登录模块 token 刷新测试并验证回归")`  ← 复用同一个 id 覆盖旧目标
- `write_note("build_output", "cargo build 输出：error[E0502] ...")`
- `remove_note("build_output")`  ← 问题解决后清除

### 5. LSP 工具集：语义层查询

补充文件系统 Source of Truth 无法直接提供的语义信息。结果作为 tool_result 在轮内可见，轮结束后丢弃。

```
lsp_hover(path, line, character)         → 类型签名、文档注释
lsp_goto_definition(path, line, char)    → 定义所在文件路径 + 行列号
lsp_find_references(path, line, char)    → 所有引用位置列表
lsp_code_action(path, line, character)   → 当前位置可用的自动修复建议列表
lsp_rename(path, line, character, new_name)  → 重命名符号，批量落盘
lsp_diagnostics(path)                    → 主动获取指定文件的完整诊断列表
```

`lsp_rename` 是其中唯一产生文件写入的工具：LSP 返回所有需要修改的位置后，改动统一通过文件工具落盘，触发 watcher 归因为 `ModifiedBy::Agent`，保持 Source of Truth 一致性。

Diagnostics 会自动附在 `open_files` 层对应文件的内容后渲染，AI 通常不需要主动调用 `lsp_diagnostics`——该工具主要用于单文件诊断超过截断阈值（20条）时获取完整列表。

详见 → [LSP 集成](lsp.md)

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

但这只是“间接修改文件”的归因方式，不应覆盖直接文件工具的价值。对 March 来说：

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

**工具层**：工具执行失败（文件不存在、命令非零退出、行号越界等）时，将完整的错误信息作为 tool result 返回给 AI，由 AI 决定如何响应（重试、换方案、向用户说明）。March 不在工具层做自动重试。

对 `run_command` 额外要求：

- 默认超时 10 秒
- AI 可按预估时长显式传 `timeout_secs`
- 成功返回时附带实际耗时
- 超时后终止进程，并以明确的 timeout 错误返回给 AI
