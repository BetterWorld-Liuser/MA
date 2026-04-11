# memorize 首次调用因 tags 类型不匹配失败 Issue Report

> 阶段:阶段一(问题报告)
> 状态:草稿
> 创建日期:2026-04-11
> 严重程度:P1

## 1. 问题现象

AI 在第一次调用 `memorize` 工具时失败，报错信息为：

`Tool 'memorize' failed. Error: invalid memorize args: invalid type: string "称呼,偏好,老大", expected a sequence`

现象表现为：AI 尝试写入长期记忆时，`tags` 参数以单个字符串形式传入，工具执行层在反序列化参数时拒绝该请求，导致本次记忆写入失败。

## 2. 复现步骤

1. 启动一个会触发长期记忆写入的 AI 对话。
2. 让 AI 首次调用 `memorize` 工具写入用户偏好类记忆。
3. 观察 AI 生成的 `memorize` 调用参数，其中 `tags` 使用类似 `"称呼,偏好,老大"` 的逗号分隔字符串。
4. 观察到:工具返回 `invalid memorize args`，并指出 `expected a sequence`。

复现频率:稳定复现（当模型按字符串而非字符串数组传 `tags` 时）

## 3. 期望 vs 实际

**期望行为**:AI 调用 `memorize` 时，工具 schema 和执行层对 `tags` 的类型约定一致；若传入的是标签列表，应被正确识别并写入记忆。

**实际行为**:工具定义把 `tags` 描述成字符串类型，模型容易输出逗号分隔字符串；执行层实际只接受 `Vec<String>`，于是首次调用直接失败。

## 4. 环境信息

- 涉及模块/功能:记忆系统 / `memorize` 工具调用链
- 相关文件/函数:`crates/march-core/src/tools.rs`、`crates/march-core/src/provider/messages.rs`、`crates/march-core/src/agent/tool_calls.rs`
- 运行环境:dev
- 其他上下文:当前定位到 `memorize` 的 tool definition 将 `tags` 标注为 `kind: "string"`，而执行层 `MemorizeArgs.tags` 为 `Vec<String>`；provider 生成 JSON schema 时也因此把该字段暴露成 `type: "string"`。

## 5. 严重程度

**P1** — 长期记忆写入是核心能力的一部分，首次调用即失败会直接破坏偏好/项目知识沉淀流程，虽然不阻塞整个对话，但会持续影响记忆功能可用性。

## 备注

已在代码中补了两层修复草案：

- 把 `memorize` / `update_memory` 的 `tags` schema 改为字符串数组
- 在执行层兼容旧格式的逗号分隔字符串，避免历史模型行为继续触发失败
