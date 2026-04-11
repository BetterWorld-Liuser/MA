# AI 命令超时与手动中断均不生效 修复记录

> 阶段: 阶段三(修复验证)
> 修复日期: 2026-04-11
> 关联根因分析: `easysdd/issues/2026-04-11-cli-timeout-not-enforced/analysis.md`

## 1. 实际采用方案

方案 B。

最终修复不是命令参数、也不是取消桥接层，而是 `run_command` 流式输出解码里的死循环。我们保留了此前的有界 shutdown 防御，同时修正了 `StreamOutputDecoder` 在 `encoding_rs` 返回 `OutputFull` 时未扩容的问题，并把子进程退出检测改成 `try_wait()` 轮询，避免主循环被阻塞式等待绑死。

## 2. 改动文件清单

- `crates/march-core/src/agent/shells.rs` — 将主循环中的 `child.wait()` 改为 `try_wait()` 轮询，确保 timeout / cancel / child exit 能共同推进
- `crates/march-core/src/agent/shells.rs` — 保留中断后 child wait、reader join、channel drain 的有界 shutdown 逻辑
- `crates/march-core/src/agent/shells.rs` — 修复 `StreamOutputDecoder` 在 `CoderResult::OutputFull` 分支未扩容导致的无限自旋
- `crates/march-core/src/agent/shells.rs` — 新增 decoder `OutputFull` 回归测试，以及 shutdown 相关回归测试
- `easysdd/issues/2026-04-11-cli-timeout-not-enforced/analysis.md` — 更新为最终确认后的根因与修复记录

## 3. 验证结果

- 复现步骤验证: 通过 ✓
  - 用户在正式包里复现确认：基础 `run_command powershell Get-ChildItem ...` 已恢复正常
- 期望行为验证: 通过 ✓
  - 命令不再在普通输出 flush 上卡死，timeout / cancel 语义恢复
- 影响面回归: 通过 ✓
  - `cargo test -p march-core run_command_timeout_stays_within_timeout_plus_shutdown_buffer --lib`
  - `cargo test -p march-core run_command_returns_early_when_cancelled --lib`
  - `cargo test -p march-core finalize_child_output_stops_waiting_when_pipe_readers_never_finish --lib`
  - `cargo test -p march-core append_shutdown_warnings_adds_human_readable_cleanup_notes --lib`
  - `cargo test -p march-core stream_output_decoder_grows_buffer_when_decoder_reports_output_full --lib`
  - `cargo check -p march-ui`

## 4. 遗留事项

- 为定位问题临时加入的后端诊断日志已清理，不作为正式行为保留
- 当前已有针对 decoder `OutputFull` 的定向回归测试；若后续还担心 Windows 真机链路，可另开 issue 补更完整的集成测试
- 无
