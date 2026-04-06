# LSP 集成

> 从 [DESIGN.md](DESIGN.md) 延伸：Language Server Protocol 为 AI 提供语义级代码理解能力，补充文件系统 Source of Truth 的语义层。

## 两类 LSP 能力

LSP 对 March 的价值分两类：

1. **Diagnostics（持续存在）**：实时的类型错误、编译错误、lint warning，随文件变化而更新。AI 不需要主动跑 `cargo check` 或 `tsc --noEmit`，上下文里的文件内容就已经带有当前的诊断状态。

2. **按需语义查询（点对点）**：hover 类型推断、go-to-definition、find-references、code actions 等——AI 主动发起，结果作为 tool_result 在轮内可见，轮结束后丢弃，不跨轮保留。

---

## Diagnostics 的上下文位置

Diagnostics 附在 `open_files` 层，紧跟对应文件内容渲染，作为该文件的语义附注：

```
--- src/auth.rs ---
 1 | fn authenticate(user: &str) -> Result<Token, Error> {
 2 |     let config = load_config();
...

[诊断: 2 个错误, 1 个警告]
  error[E0507] 第15行: cannot move out of `*user` which is behind a shared reference
  error[E0308] 第23行: mismatched types: expected `Token`, found `()`
  warning 第8行: unused variable `config`
```

**无诊断时不渲染诊断块**：如果当前文件没有任何 error/warning，不追加任何内容，保持文件内容原样。这样 cache 行为与纯文件快照一致——没有额外的 `[诊断: 无]` 占位污染 prefix。

**为什么不独立成一层**：diagnostics 和文件内容是语义上的一体——文件内容变了，diagnostics 也跟着变。从 prefix cache 角度看，两者本来就绑定在一起，cache 失效是预期行为而非额外代价。放在文件内容旁边，AI 读代码时能直接感知当前的错误位置，不需要跨块关联。

**诊断信息截断**：单文件诊断条目超过 20 条时，只渲染前 20 条，并附注 `…[+N 条，建议用 lsp_diagnostics 获取完整列表]`。

---

## LSP 工具集

按需语义查询作为工具层的新成员，结果在轮内可见，轮结束后整体丢弃（与其他 tool_result 一致）：

```
lsp_hover(path, line, character)
  → 返回指定位置的类型签名、文档注释

lsp_goto_definition(path, line, character)
  → 返回定义所在文件路径 + 行列号

lsp_find_references(path, line, character)
  → 返回所有引用位置列表

lsp_code_action(path, line, character)
  → 返回当前位置可用的自动修复建议列表，AI 可选择执行

lsp_rename(path, line, character, new_name)
  → 重命名符号，LSP 返回所有需要修改的位置，March 批量落盘
  → 所有改动通过正常文件工具写入，触发 watcher 归因为 ModifiedBy::Agent

lsp_diagnostics(path)
  → 主动获取指定文件的完整诊断列表（用于截断后查看全量）
```

`lsp_rename` 是其中唯一会产生文件写入的工具，改动统一走文件工具落盘，保持 Source of Truth 一致性。

---

## LSP Server 生命周期

**自动发现**：session 初始化时，March 扫描工作目录，根据特征文件自动发现并启动对应 language server：

| 特征文件 | Language Server |
|----------|----------------|
| `Cargo.toml` | rust-analyzer |
| `package.json` + TypeScript 依赖 | typescript-language-server |
| `pyproject.toml` / `setup.py` | pylsp / pyright |
| `go.mod` | gopls |

可用的 language server 信息写入 `session_status`，AI 知道当前有哪些语言能力可用：

```
[session_status]
...
LSP: rust-analyzer (src/, crates/)
```

若工作目录没有已知 language server 可用，`session_status` 不显示 LSP 行，LSP 工具调用返回"当前工作目录无可用 language server"。

**与 watcher 的同步**：watcher 检测到文件变化时，同步向 LSP server 发送 `textDocument/didChange` 通知，确保 diagnostics 和 watcher 看到的文件状态始终一致，避免"watcher 已更新、LSP 还在分析旧版本"的窗口期。

**生命周期管理**：language server 与 session 绑定，session 结束时关闭。崩溃时自动重启（最多 3 次），重启失败后从 `session_status` 移除该条目，相关工具调用返回错误，AI 自然降级到 `run_command` 方案。

---

## 与 run_command 的关系

LSP 工具和 `run_command` 不是竞争关系：

- **LSP 工具**：语义层查询，结果结构化、精确，不依赖命令输出解析
- **run_command**：构建、测试、运行——需要实际执行代码的场景

AI 可以同时使用两者：用 LSP 查类型和引用，用 `run_command` 跑测试确认行为。

诊断信息从 LSP 来，不需要 AI 先跑编译命令再解析输出——但 `run_command cargo test` 仍然是验证逻辑正确性的唯一手段，两者覆盖的层次不同。
