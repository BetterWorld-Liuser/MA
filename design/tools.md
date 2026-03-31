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
    command: String,
    working_directory: PathBuf,
    shell: Option<CommandShell>,
}
```

其中 `shell` 可以显式指定执行环境，例如：

- `bash`
- `sh`
- `powershell`
- `cmd`

但这个枚举只是“可能的 shell 类型”，不是说每个环境里都一定有这些程序。
Ma 在会话启动时应先扫描当前环境里实际可运行的 shell，再把结果注入给 AI。

如果调用时不指定，则走当前环境中探测到的默认 shell：

- Windows 默认 `powershell`
- Unix-like 默认 `sh`

把 shell 作为显式参数，同时把“当前环境下哪些 shell 可用”作为运行时信息注入提示词，有几个直接收益：

- AI 可以根据命令语法选择正确环境，避免把 PowerShell 语法发到 `cmd`
- AI 不会误用当前机器上根本不存在的 shell
- tool call 记录更完整，用户能看见“命令在哪个环境里执行”
- 后续做权限控制、审计、重放和跨平台兼容时，输入边界更稳定

注意这里的 `shell` 是“命令由哪个解释器执行”，不是要把环境抽象成更高层任务语义。

建议在 tool usage prompt 中注入类似信息：

```text
run_command available shells in this session:
- powershell
- cmd
- bash

Default shell: powershell
Only choose from the shells listed above.
```

### 2. 文件工具：`read_file` / `write_file` / 行号级编辑

用于最基础、最常见、也最需要稳定性的文件操作：

- `read_file(path)`：返回文件当前真实内容，默认带行号或可请求带行号视图
- `write_file(path, content)`：整文件写入，适合新建文件或明确覆盖
- `replace_lines(path, start_line, end_line, new_content)`
- `insert_lines(path, after_line, new_content)`
- `delete_lines(path, start_line, end_line)`

这里保留文件专用工具，而不是强迫 AI 用 shell 做所有文件操作，原因有三点：

- 基础读写是高频路径，应该尽量减少 shell 转义、here-doc、平台差异等噪音
- 文件工具可以直接接入 watcher / snapshot / ModifiedBy 归因逻辑，更贴近 Source of Truth
- 命令执行失败时往往混有环境因素；基础文件操作应该尽量确定、可预测、可审计

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
- 需要 shell 内建语法、管道、重定向、脚本片段时，显式指定对应 shell
- 只是运行单个可执行文件时，仍可经由默认 shell 执行；后续也可演进出更底层的 direct exec 能力
- 同一轮上下文里，AI 应尽量保持 shell 风格一致，减少语法来回切换

典型示例：

- `powershell`: `Get-ChildItem src | Select-Object Name`
- `cmd`: `dir src`
- `bash`: `ls src | grep rs`
