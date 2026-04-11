# AI 命令超时与手动中断均不生效 根因分析

> 阶段: 阶段二(根因分析)
> 状态: 已确认
> 分析日期: 2026-04-11
> 关联问题报告: `easysdd/issues/2026-04-11-cli-timeout-not-enforced/report.md`

> 历史检索: 已按 `track=pitfall` + `timeout cancel command process stream` 搜索 `easysdd/learnings/`，未找到同类历史记录。

## 1. 问题定位

| 关键位置 | 说明 |
|---|---|
| `crates/march-core/src/agent/tool_calls.rs:50-68` | `run_command` 工具会正确解析 `timeout_secs`，并把超时时间传入 `run_command_with_output`，所以问题不是参数丢失。 |
| `crates/march-core/src/agent/shells.rs:166-215` | `collect_child_output` 主循环负责同时处理 timeout、cancel、子进程退出和流式输出。用户日志表明主循环已经进入，并拿到了首段 stdout。 |
| `crates/march-core/src/agent/shells.rs:415-455` | 真正卡点在 `emit_output_update()` 内的输出解码：`StreamOutputDecoder::push()` 使用 `encoding_rs::Decoder::decode_to_string()` 时，旧实现没有处理 `CoderResult::OutputFull` 的扩容需求。 |
| `crates/march-core/src/agent/shells.rs:543-585` | `StreamOutputDecoder` 的修复点：在初始解码前预留容量，并在 `OutputFull` 分支里继续 `reserve()`，避免“未消费输入但无限 continue”的死循环。 |
| `crates/march-core/src/ui/backend/messaging.rs:582-650` | turn worker 需要等待工具调用返回，才能继续发出 `ToolFinished` / `TurnFinished`。因此底层 decoder 一旦卡住，UI 体感就会变成“命令不返回、timeout 不生效、中断也无效”。 |
| `crates/march-core/src/agent.rs:739-862` | 原有测试覆盖了 timeout / cancel 的语义，但没有覆盖 decoder `OutputFull` 这一条更底层的卡死路径。 |

## 2. 失败路径还原

**正常路径**:  
用户触发 `run_command` → PowerShell 子进程启动 → stdout/stderr chunk 被 reader 读出 → `emit_output_update()` 成功解码并回调上层 → 主循环继续推进 timeout / cancel / child exit → 工具返回结果或超时错误 → `ToolFinished` / `TurnFinished` 发给 UI。

**失败路径**:  
用户触发 `run_command` → PowerShell 子进程启动并很快产出第一段 stdout → `collect_child_output()` 进入 `emit_output_update()` → `StreamOutputDecoder::push()` 调用 `decode_to_string()` 时返回 `OutputFull` → 旧实现没有扩容，只是原地 `continue`，形成无限自旋 → 主循环被卡死在一次普通输出 flush 上 → timeout、cancel、child poll 都失去继续执行的机会 → UI 表现为命令一直挂住、超时和手动中断都像失效了一样。

**分叉点**: `crates/march-core/src/agent/shells.rs:556-566` — decoder 在 `OutputFull` 分支没有扩容，导致流式输出阶段直接死循环。

## 3. 根因

**根因类型**: 数据处理逻辑错误

**根因描述**:  
`StreamOutputDecoder::push()` 里使用 `encoding_rs::Decoder::decode_to_string()` 解码 stdout/stderr。该 API 在输出目标 `String` 容量不足时，会返回 `CoderResult::OutputFull`。旧实现没有为这种情况扩容，而是直接 `continue`；如果 decoder 此时又没有消费任何输入，就会反复返回 `OutputFull`，造成无限循环。由于这段逻辑位于 `emit_output_update()` 内部，位置早于 timeout / cancel / child exit 的下一轮 poll，因此会伪装成“命令本身卡死，超时和取消都没生效”。

**是否有多个根因**: 是

1. **主根因**: `StreamOutputDecoder` 对 `OutputFull` 分支缺少扩容逻辑，导致可能无限自旋。  
2. **次根因**: 该死循环发生在流式输出更新路径内，比 timeout / cancel 判断更早，掩盖了真实故障位置。  
3. **保障缺口**: 之前没有针对 decoder `OutputFull` 场景的回归测试，导致这个底层 release 态问题未被捕获。

## 4. 影响面

- **影响范围**: 不只影响报告里的 `npx skills find marketing`。任何 `run_command` 只要产生 stdout/stderr 并走到这段解码逻辑，都可能触发同类卡死，包括普通的 `Get-ChildItem` / `Get-Content`。
- **潜在受害模块**: 技能自动调用命令、目录搜索、测试命令、CLI 输出展示、任何依赖 `run_command` 流式输出的工具链路。
- **数据完整性风险**: 低。问题主要是运行时卡死和交互链路失去响应，不是持久化数据损坏。
- **严重程度复核**: 维持 `P1`。它直接破坏了命令工具最核心的可用性。

## 5. 修复方案

### 方案 A: 仅修 decoder `OutputFull` 分支

- **做什么**: 在 `StreamOutputDecoder` 中预留输出容量，并在 `CoderResult::OutputFull` 时继续扩容后重试；同时新增一条专门覆盖该分支的测试。
- **优点**: 直接命中真实根因，改动最小。
- **缺点/风险**: 只能解决这次 decoder 自旋问题，不能替代其他命令生命周期上的兜底保护。
- **影响面**: 主要修改 `crates/march-core/src/agent/shells.rs`。

### 方案 B: 保留有界 shutdown 改进，同时修正 decoder 死循环

- **做什么**: 在修复 decoder 的同时，保留此前已经增加的有界 child/reader 收尾保护，并将 `child.wait()` 改为 `try_wait()` 轮询，避免主循环再被阻塞式等待绑死。
- **优点**: 既修掉真实卡死点，也保留命令生命周期上的兜底。
- **缺点/风险**: 改动略大于只修 decoder，但仍然局限在 `shells.rs`。
- **影响面**: `crates/march-core/src/agent/shells.rs`。

### 方案 C: 对特定命令做包装/特判

- **做什么**: 对 `npx` / `npm` 等命令单独处理。
- **优点**: 对个别命令可快速缓解。
- **缺点/风险**: 由于真实根因在通用 decoder，这个方案无法解决 `Get-ChildItem` 这类普通命令的卡死，不成立。
- **影响面**: 会把平台兼容逻辑散落到工具层，不建议采用。

### 推荐方案

**推荐方案 B**，理由:  
最终定位显示真实卡点在 decoder，但此前加入的有界 shutdown 保护依然是合理兜底。保留收尾保护，同时补上 `OutputFull` 扩容和定向测试，能在不扩大范围的前提下把“真实根因 + 生命周期防御”一起收敛。

## 6. 修复记录

- 已按用户确认采用 **方案 B**
- 实际改动收敛在 `crates/march-core/src/agent/shells.rs`
- 落地方式:
  - 保留中断后 `child.wait()`、pipe reader join、pipe channel drain 的有界收尾保护
  - 将主循环中的 `child.wait()` 改为 `try_wait()` 轮询，避免阻塞式等待影响其他分支推进
  - 在 `StreamOutputDecoder` 中为 `OutputFull` 分支显式扩容，修复 stdout/stderr 解码死循环
  - 新增针对 decoder `OutputFull` 场景的定向回归测试，以及此前的 shutdown 回归测试
