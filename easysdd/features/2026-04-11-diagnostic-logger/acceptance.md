# Diagnostic Logger 验收报告

> 阶段: 阶段五(验证闭环)
> 验收日期: 2026-04-11
> 关联 PRD: `easysdd/features/2026-04-11-diagnostic-logger/prd.md`
> 关联方案 doc: `easysdd/features/2026-04-11-diagnostic-logger/design.md`

## 1. 功能行为核对

按 PRD 第 3 节“成功长什么样”逐项验证:

- [x] 前端调试时可以手动留下独立日志，且不会与后端或 runtime 输出混在一起。
  实测结果: `src/lib/frontendDiagnosticLogger.ts` 通过 `write_frontend_diagnostic_log` 桥接到 `src-tauri/src/lib.rs`，运行期测试 `tests::persist_frontend_diagnostic_log_writes_frontend_log_file` 已验证记录写入 `.march/diagnostics/frontend.log`；后端文件仍是 `.march/diagnostics/backend.log`，现有 debug 输出仍在 `.march/debug/*.log`。
- [x] 后端关键链路无需临时加打印，就能直接回看关键事件记录并定位问题经过。
  实测结果: `crates/march-core/src/main.rs` 已接入 `turn.started`、`model.requested`、`tool.finished`、`round.completed`、`turn.completed` / `turn.failed`；`crates/march-core/src/agent/session.rs` 已接入 `command.started`、`command.finished`、`command.timed_out`、`command.cancelled`、`command.failed`。`cargo test -p march-core diagnostics` 与 `cargo test -p march-core run_command_writes` 均通过。

按 PRD 第 4 节“不做什么”逐项核对:

- [x] 不做自动抓取全量上下文、完整快照或大对象日志，确实没做。
  代码 review 结果: 后端写入的是事件摘要和少量字段；`main.rs`/`session.rs` 未默认写入完整 prompt、完整响应、完整 stdout/stderr。
- [x] 不做前端默认全局埋点，只支持调试时主动记录，确实没做。
  grep 结果: `frontendDiagnosticLogger` 只在 [src/main.ts](/D:/playground/MA/src/main.ts:5)、[src/App.vue](/D:/playground/MA/src/App.vue:204)、[src/composables/useWorkspaceApp.ts](/D:/playground/MA/src/composables/useWorkspaceApp.ts:19) 这几个显式点位接入；其余 `debugChat(...)` 调用仍保持 console-only。
- [x] 不做日志查看 UI 或可视化面板，确实没做。
  grep 结果: 本次变更未新增任何 diagnostics viewer/panel/settings 入口，UI 只新增了前端显式 logger 调用。
- [x] 不在第一版覆盖所有后端模块，只接入 3 到 5 个关键模块，确实没做。
  代码 review 结果: 第一版实际集中在 `diagnostics.rs`、CLI/agent 主链路、`run_command`、Tauri 桥接和少量前端点位，没有扩散到所有后端模块。

## 2. 不变量逐条核对

- [x] **I1**: Diagnostic Log 永远写入项目级 `.march/diagnostics/`，且不得回退到 `.march/debug/` 或调用方当前目录
  - 验证手段: 单测 + 代码 review
  - 结果: 通过。`DiagnosticLogger::new` 固定使用 `{project_root}/.march/diagnostics`；`diagnostics::tests::new_creates_project_scoped_diagnostics_directory` 已覆盖。
- [x] **I2**: Backend Diagnostic Log 与 Frontend Diagnostic Log 必须分文件持久化，且两者都不得与现有 `.march/debug/*.log` 混写
  - 验证手段: 单测 / 集成测试 + 代码 review
  - 结果: 通过。`backend.log` / `frontend.log` 分流，`diagnostics::tests::backend_and_frontend_records_are_written_to_separate_files` 与 `tests::persist_frontend_diagnostic_log_writes_frontend_log_file` 已覆盖。
- [x] **I3**: Diagnostic Logger 是新增诊断记录的单一写路径，业务模块不得各自直接 `fs::write` / `console` 落盘到 `.march/diagnostics/`
  - 验证手段: 代码 review + 模块测试
  - 结果: 通过。后端统一经 `crates/march-core/src/diagnostics.rs`，前端统一经 `src-tauri/src/lib.rs -> DiagnosticLogger`，未发现其他模块直写 `.march/diagnostics/`。
- [x] **I4**: Frontend Diagnostic Log 的 payload 缺省字段不得在桥接或序列化过程中被默默覆盖成错误值或 `undefined` 字面量
  - 验证手段: 类型检查 + 运行期测试
  - 结果: 通过。`src/lib/frontendDiagnosticLogger.ts` 会过滤 `undefined` 字段并把 `null` 归一化成字符串；`tests::persist_frontend_diagnostic_log_writes_frontend_log_file` 通过。
- [x] **I5**: `run_command` 的正常结束、超时、取消三条路径必须都产出同一语义体系下的摘要诊断记录
  - 验证手段: 集成测试
  - 结果: 通过。`agent::session::tests::run_command_writes_finished_diagnostic_log`、`...timeout...`、`...cancelled...` 全部通过。
- [ ] **I6**: Frontend Debug Logger 的落盘失败不得中断页面原有交互流程
  - 验证手段: 浏览器人工验收 + 控制台观察
  - 结果: 部分通过，但未完成最终签收。浏览器里可确认 `frontendDiagnosticLogger` 的失败被降级为 `console.warn`，没有形成未捕获的 logger 异常；但同一页面同时命中了与本功能无关的现有错误 `useWindowControls -> getCurrentWindow()`，导致 `App` 在纯 web 模式下 setup 失败，无法完成“页面原有交互流程未中断”的最终肉眼验收。需要单独修复该 web 模式问题后复验。
- [x] **I7**: 第一版 Diagnostic Log 默认只记录设计中选定的关键事件，不得自动升级为全量上下文/全局埋点采集
  - 验证手段: 代码 review + 单测
  - 结果: 通过。后端只接关键事件；前端只接少量显式点位；`tests::diagnostic_event_writer_records_minimal_backend_turn_flow` 与代码 grep 都支持该结论。

## 3. 对接点回归

按方案 doc 第 2 节“对接点梳理”逐项回归:

- [x] 项目级 `.march` 根目录定位([crates/march-core/src/paths.rs](/D:/playground/MA/crates/march-core/src/paths.rs:28)): 已回归。
  跑了 `diagnostics::tests::new_creates_project_scoped_diagnostics_directory`，确认日志目录固定落在项目级 `.march/diagnostics/`。
- [x] 现有 CLI debug 输出([crates/march-core/src/main.rs](/D:/playground/MA/crates/march-core/src/main.rs:198)): 已回归。
  代码 review 确认 `.march/debug/context.log` / `provider.log` 保留；diagnostics 新增为平行通道，没有替换现有 Debug Output。
- [x] 后端任务/轮次生命周期([crates/march-core/src/main.rs](/D:/playground/MA/crates/march-core/src/main.rs:347)): 已回归。
  `diagnostic_event_writer_records_minimal_backend_turn_flow` 覆盖 `turn.started`、`model.requested`、`tool.finished`、`round.completed`。
- [x] 命令执行链路([crates/march-core/src/agent/session.rs](/D:/playground/MA/crates/march-core/src/agent/session.rs:266)): 已回归。
  正常结束、超时、取消三条路径测试全部通过。
- [x] 前端桥接([src-tauri/src/lib.rs](/D:/playground/MA/src-tauri/src/lib.rs:1312)): 已回归。
  `parse_frontend_diagnostic_level_*` 与 `persist_frontend_diagnostic_log_writes_frontend_log_file` 全部通过。
- [x] 前端显式接入点([src/main.ts](/D:/playground/MA/src/main.ts:10), [src/App.vue](/D:/playground/MA/src/App.vue:308), [src/composables/useWorkspaceApp.ts](/D:/playground/MA/src/composables/useWorkspaceApp.ts:244)): 已回归。
  代码 review 确认仅少量点位显式调用，console 调试体验仍保留。

前端改动浏览器肉眼验证:

- [ ] 页面主区域: 未完成最终签收。
  浏览器证据: 通过 `agent-browser` 打开 `http://127.0.0.1:4173/` 后，控制台可见 `frontendDiagnosticLogger` 在纯浏览器环境中的失败被降级为 warning；但页面同时命中现有 `Vue warn: Unhandled error during execution of setup function at <App>`，根因指向 [src/composables/workspaceApp/useWindowControls.ts](/D:/playground/MA/src/composables/workspaceApp/useWindowControls.ts:5) 直接调用 `getCurrentWindow()`。因此无法在纯浏览器模式下完成完整 UI 冒烟。截图: [browser-check](/D:/playground/MA/.codex-acceptance-browser.png)
- [x] 交互行为“前端诊断写盘失败只降级 warning，不抛出未捕获 logger 异常”: 浏览器验证 OK。
  浏览器证据: `agent-browser console --json` 记录了 `[frontend-diagnostic-log] failed to write diagnostic log ...` warning，而不是未捕获异常；这是对 logger 失败降级路径的直接肉眼验证。

## 4. 术语一致性

- `Frontend Debug Logger`: 代码命中 14 处，均使用 `frontendDiagnosticLogger` 这一术语族，一致 ✓
- `Diagnostic Logger`: 代码命中 16 处，后端与桥接层都使用 `DiagnosticLogger`，一致 ✓
- `write_frontend_diagnostic_log`: 代码命中 4 处，桥接接口命名与方案 doc 一致 ✓
- `Debug Output`: 仅保留既有 `.march/debug/*.log` 语义，一致 ✓
- 防撞车:
  - 禁用词 `debug log` / `runtime log` 在业务新代码中无新增命中；当前 2 处命中都来自既有 `crates/march-core/src/main.rs` 对 `.march/debug/*.log` 的旧语义，符合方案第 0 节允许的既有概念复用。

## 5. 文档归档

- [x] 方案 doc 与最终实现一致
  第 7 节不变量的“验证测试用例”已回填完毕，未发现“阶段四回填”残留。
- [x] 项目级架构 doc 需要同步的地方已同步
  [easysdd/architecture/DESIGN.md](/D:/playground/MA/easysdd/architecture/DESIGN.md:166) 已补“诊断日志”章节，明确 `.march/diagnostics/` 与 `.march/debug/` 的职责边界。
- [x] PRD 第 4 节“不做什么”最终都没做
  已在第 1 节逐项复核。

## 6. 遗留

- 后续优化点(建议单开 issue):
  - 修复纯 web 模式下 [src/composables/workspaceApp/useWindowControls.ts](/D:/playground/MA/src/composables/workspaceApp/useWindowControls.ts:5) 直接调用 `getCurrentWindow()` 导致的 `App setup` 失败，然后补做一次完整前端浏览器验收。
- 已知限制:
  - 当前已验证的前端“失败降级”来自纯浏览器环境；在真实 Tauri 宿主中的完整页面交互，本次验收未借助桌面自动化工具做二次肉眼确认。
- 阶段四“顺手发现”列表:
  - `diagnostic_event_writer(...)` 仍留在 [crates/march-core/src/main.rs](/D:/playground/MA/crates/march-core/src/main.rs:347)，虽然功能正确，但后续若 diagnostics 继续扩张，建议再抽离以减轻入口文件职责。
