---
track: pitfall
date: 2026-04-11
slug: run-command-outputfull-decoder-spin
component: march-core/agent-run-command
severity: high
tags: [run_command, timeout, cancel, decoder, encoding_rs, stdout, windows, debugging]
---

## 问题
`run_command` 看起来像是 timeout 和手动中断都失效了，但真实根因可能根本不在超时/取消链路，而是在 stdout/stderr 的增量解码里卡死。

## 症状
- AI 工具调用里能看到 `run_command ... (timeout 10s/20s)`，但实际一直不返回
- 点击中断也没反应
- UI 停在 tool running 状态
- 后端日志显示：
  - 已经 `spawned`
  - 已经收到第一段 stdout
  - 卡在一次普通 `flush-output`
  - 后续再也看不到 timeout / cancel / turn finished

## 没用的做法
- 一开始把问题归因到 `timeout_secs` 没传进去
- 怀疑 Windows `taskkill /T /F` 没有正确收进程树
- 先修 `child.wait()` / pipe reader 收尾，虽然是合理兜底，但并不是这次真正卡死点
- 只盯着 UI 和取消按钮，看不到底层 actually 卡在输出解码

## 解法
在 `crates/march-core/src/agent/shells.rs` 的 `StreamOutputDecoder` 里，处理 `encoding_rs::Decoder::decode_to_string()` 返回的 `CoderResult::OutputFull`：

1. 解码前先给目标 `String` 预留容量
2. 当返回 `OutputFull` 时继续 `reserve()` 扩容
3. 然后再重试，而不是原地 `continue`

同时补一条定向回归测试，专门覆盖 decoder `OutputFull` 分支。

## 为什么有效
`encoding_rs` 在输出 buffer 容量不足时会返回 `OutputFull`。如果此时实现没有扩容，而 decoder 又没有消费任何输入，就会在同一段输入上无限重复返回 `OutputFull`。由于这段逻辑发生在 `emit_output_update()` 内部，位置早于 timeout / cancel 的下一轮 poll，外部体感就会被误导成“命令卡死、超时失效、中断也失效”。

## 预防
- 调 `encoding_rs::decode_to_string()` 这类 API 时，不要假设 `OutputFull` 只是偶发分支；必须显式处理 buffer 扩容
- 命令链路出现“看起来像 timeout/cancel 失效”时，先加日志确认卡点是在：
  - 命令主循环
  - 输出 flush
  - 解码
  - UI 回调
- 对 `run_command` 这种流式链路，除了 timeout/cancel 测试，还要补“decoder `OutputFull` 不会自旋”的定向测试

## 相关文档
- `easysdd/issues/2026-04-11-cli-timeout-not-enforced/report.md`
- `easysdd/issues/2026-04-11-cli-timeout-not-enforced/analysis.md`
- `easysdd/issues/2026-04-11-cli-timeout-not-enforced/fix-note.md`
