# Diagnostic Logger Design

> 阶段: 阶段二 (设计)
> 状态: 草稿
> 创建日期: 2026-04-11

## 0. 术语约定

### Diagnostic Log
- **定义**: 为问题定位而持久化到 `.march` 独立位置的一类诊断记录，和现有 runtime 输出、`.march/debug/*.log` 调试输出分流。
- **出现层**: 持久化 / 协议 / 数据模型 / 前端类型 / UI 文案
- **外部对应字段**: 无

Grep 防撞车检查:
- `diagnostic log|diagnostic logger|诊断日志` 只命中本 feature 的 `brainstorm.md` / `prd.md`，现有代码与架构文档中未被占用。
- 结论: 可作为本功能的新术语引入。

### Diagnostic Logger
- **定义**: 负责把 Diagnostic Log 写入目标位置的记录器抽象；后端与前端分别有自己的实现，但共享同一组产品语义。
- **出现层**: 数据模型 / 协议 / 前端类型 / 持久化
- **外部对应字段**: 无

Grep 防撞车检查:
- `diagnostic logger|诊断日志` 未命中现有代码与架构文档，仅命中本 feature 文档。
- 结论: 可作为本功能的新术语引入。

### Runtime Output
- **定义**: March 运行过程中原本就会产生、以运行反馈为目的的输出流，不承担本功能定义的独立诊断记录职责。
- **出现层**: 协议 / UI 文案
- **外部对应字段**: 无

Grep 防撞车检查:
- `runtime output|运行时输出` 仅命中本 feature 文档。
- 现有架构中更稳定的既有术语是 `runtime_status`，表示上下文里的运行态快照，不等于“运行时输出”。
- 结论: 本 doc 保留 `Runtime Output` 作为用户视角术语，但实现设计中不得把它和 `runtime_status` 混用。

### Debug Output
- **定义**: 现有 CLI 调试能力产出的 `.march/debug/context.log` 与 `.march/debug/provider.log` 一类输出，用于开发期查看 provider/context 细节，不等于本功能新增的 Diagnostic Log。
- **出现层**: 持久化 / UI 文案
- **外部对应字段**: 无

Grep 防撞车检查:
- `debug log|debug logs` 已命中 [crates/march-core/src/main.rs](/D:/playground/MA/crates/march-core/src/main.rs:210) 中的现有实现。
- 结论: `Debug Output` 视为既有概念复用，不再把新能力命名为新的 “debug log”，避免与 `.march/debug/*.log` 混淆。

### Frontend Debug Logger
- **定义**: 仅供前端调试时显式调用的轻量记录器，不承担默认全局埋点职责，输出进入前端侧 Diagnostic Log 通道。
- **出现层**: 前端类型 / 持久化
- **外部对应字段**: 无

Grep 防撞车检查:
- `frontend debug logger|前端调试日志|前端调试 logger` 未命中现有代码、架构文档和既有 feature 方案。
- 结论: 可作为本功能的新术语引入。

### 禁用词列表

以下词在新代码和新文档里禁止直接裸用，除非明确指向既有概念：

- `log`
  原因: 过于宽泛，无法区分 runtime output / debug output / diagnostic log。
- `debug log`
  原因: 已被现有 `.march/debug/context.log` / `provider.log` 语义占用。
- `runtime log`
  原因: 容易和 `runtime_status`、命令流式输出、诊断日志三者混淆。

## 1. 需求摘要

March 需要一条独立于现有 runtime 输出和 `.march/debug/*.log` 的诊断日志通道，让开发者在排查问题时可以稳定回看关键过程，而不是继续依赖临时加打印。第一版覆盖后端关键链路与前端调试场景：后端默认保留 3 到 5 个关键模块的事件记录，前端只在调试时显式留下独立记录。整个能力必须支持日志等级，并且统一落在 `.march` 下但与现有 debug/runtime 输出分流。第一版明确不做自动抓全量上下文、不做前端默认全局埋点、不做日志查看 UI，也不追求一次覆盖所有后端模块。

## 2. 对接点梳理

### 2.1 读哪些既有数据

- **项目级 `.march` 根目录定位**
  - 代码指针: [crates/march-core/src/paths.rs](/D:/playground/MA/crates/march-core/src/paths.rs:28) `resolve_project_root`
  - 作用: 新的后端 Diagnostic Logger 需要复用项目根 `.march` 的定位语义，避免 task 在子目录运行时把日志错误写到嵌套目录。

- **现有 CLI debug 输出位置**
  - 代码指针: [crates/march-core/src/main.rs](/D:/playground/MA/crates/march-core/src/main.rs:198) `DebugLogs::new` / `DebugLogs::reset` / `DebugLogs::write_rounds`
  - 作用: 方案需要读取并继承当前 `.march/debug/context.log`、`provider.log` 的职责边界，确保新 Diagnostic Log 不与其混用。

- **前端现有调试调用习惯**
  - 代码指针: [src/lib/chatDebug.ts](/D:/playground/MA/src/lib/chatDebug.ts:10) `debugChat`
  - 作用: 前端 Diagnostic Logger 第一版要承接当前“显式调试时调用”的使用习惯，而不是引入一套完全陌生的调用姿势。

- **前端调试调用分布**
  - 代码指针: [src/App.vue](/D:/playground/MA/src/App.vue:307), [src/main.ts](/D:/playground/MA/src/main.ts:9), [src/composables/workspaceApp/messageActions.ts](/D:/playground/MA/src/composables/workspaceApp/messageActions.ts:63), [src/composables/useWorkspaceApp.ts](/D:/playground/MA/src/composables/useWorkspaceApp.ts:243)
  - 作用: 这些现有 `debugChat(...)` 调用点是前端 logger 的首批实际消费方候选，但第一版不要求全部迁移。

### 2.2 写哪些既有数据

- **向项目级 `.march` 新增诊断日志目录与文件**
  - 代码指针: [crates/march-core/src/storage.rs](/D:/playground/MA/crates/march-core/src/storage.rs:88) `MarchStorage::open`, [crates/march-core/src/main.rs](/D:/playground/MA/crates/march-core/src/main.rs:265) `append_text`
  - 新写路径性质: 新增一条项目级写路径，写入目标仍属于 `.march/` 范围，但不能复用 `.march/debug/` 目录。
  - 单一写路径判断: 后端 Diagnostic Log 需要收敛为单一 writer 抽象，不能让各模块各自直接 `fs::write` 到任意文件。

- **保留现有 `.march/debug/*.log` 写路径**
  - 代码指针: [crates/march-core/src/main.rs](/D:/playground/MA/crates/march-core/src/main.rs:210) `DebugLogs::reset`, [crates/march-core/src/main.rs](/D:/playground/MA/crates/march-core/src/main.rs:218) `DebugLogs::write_rounds`
  - 作用: 现有 Debug Output 继续存在，本功能不替换其写路径，只补独立 Diagnostic Log 写路径。

- **前端新增落盘桥接**
  - 代码指针: [src/lib/chatDebug.ts](/D:/playground/MA/src/lib/chatDebug.ts:10) `debugChat`, [src-tauri/src/lib.rs](/D:/playground/MA/src-tauri/src/lib.rs:1276) `std::env::set_current_dir`
  - 新写路径性质: 前端不能直接写磁盘，需要通过 Tauri/Rust 后端桥接到项目级 `.march`；这是一条新增写路径，但不能破坏前端“显式调用才记录”的使用方式。

### 2.3 订阅哪些既有事件

- **后端任务/轮次生命周期**
  - 代码指针: [crates/march-core/src/agent/runner.rs](/D:/playground/MA/crates/march-core/src/agent/runner.rs:47) `handle_user_message_with_events_and_cancel`
  - 作用: 后端首批 3 到 5 个关键模块之一应覆盖任务处理主链路，这里是关键事件的天然入口。

- **前端 Agent 运行事件**
  - 代码指针: [src/composables/useWorkspaceApp.ts](/D:/playground/MA/src/composables/useWorkspaceApp.ts:297) 事件订阅中的 `debugChat('workspace-app', 'event:received', ...)`
  - 作用: 若前端调试时需要对运行事件留痕，应复用现有事件消费点按需调用 Frontend Debug Logger，而不是另起一条平行订阅链。

### 2.4 发射哪些既有事件

- **无新增项目级事件要求**
  - 第一版目标是把诊断信息落盘，而不是改变现有 UI 事件模型。
  - 代码指针: [easysdd/architecture/ui-events.md](/D:/playground/MA/easysdd/architecture/ui-events.md:423) 已明确 `runtime` / `debug_trace` 走独立通道事件；本方案不在第一版向该协议新增专用 diagnostic 事件。

### 2.5 改动哪些既有 UI 区域

- **无新增 UI 区域**
  - PRD 已明确第一版不做日志查看 UI 或可视化面板，因此不新增右栏、聊天区或设置页入口。

- **可能调整的前端调试调用点**
  - 代码指针: [src/lib/chatDebug.ts](/D:/playground/MA/src/lib/chatDebug.ts:10) `debugChat`
  - 作用: 现有 `console.log` 型调试工具可能需要抽象成同时支持 console 与 Diagnostic Log 的前端调试入口，但不改变用户可见界面。

### 2.6 完全新建、不 touch 既有代码的部分

- **后端 Diagnostic Logger 模块**
  - 需要新建独立模块来封装目录、文件分流、等级与追加写入规则，避免把逻辑继续堆在 CLI `main.rs`。

- **前端 Frontend Debug Logger 抽象**
  - 需要新建独立封装来承接前端“调试时显式记录”的调用方式，并与现有 `debugChat` 使用姿势兼容。

## 3. 目标模型

### 3.1 后端核心类型（Rust 伪代码）

```rust
pub enum DiagnosticLevel {
    Debug,
    Info,
    Warn,
    Error,
}

pub enum DiagnosticChannel {
    Backend,
    Frontend,
}

pub struct DiagnosticRecord {
    pub timestamp_ms: u64,
    pub level: DiagnosticLevel,
    pub channel: DiagnosticChannel,
    pub scope: String,
    pub event: String,
    pub message: String,
    pub fields: BTreeMap<String, String>,
}

pub struct DiagnosticPaths {
    pub root_dir: PathBuf,      // {project_root}/.march/diagnostics
    pub backend_path: PathBuf,  // backend.log
    pub frontend_path: PathBuf, // frontend.log
}

pub struct DiagnosticLogger {
    pub paths: DiagnosticPaths,
}
```

```rust
impl DiagnosticLogger {
    pub fn new(project_root: &Path) -> Result<Self>;

    pub fn write_backend(&self, record: DiagnosticRecord) -> Result<()>;

    pub fn write_frontend(&self, record: DiagnosticRecord) -> Result<()>;
}
```

### 3.2 前端核心类型（TypeScript 伪代码）

```ts
export type DiagnosticLevel = 'debug' | 'info' | 'warn' | 'error';

export type FrontendDiagnosticPayload = {
  level: DiagnosticLevel;
  scope: string;
  event: string;
  message?: string;
  fields?: Record<string, string | number | boolean | null>;
};

export type FrontendDebugLogger = {
  log(payload: FrontendDiagnosticPayload): Promise<void>;
  debug(scope: string, event: string, fields?: Record<string, unknown>): Promise<void>;
  info(scope: string, event: string, fields?: Record<string, unknown>): Promise<void>;
  warn(scope: string, event: string, fields?: Record<string, unknown>): Promise<void>;
  error(scope: string, event: string, fields?: Record<string, unknown>): Promise<void>;
};
```

### 3.3 Tauri 桥接接口（Rust / TS 伪代码）

```rust
#[derive(Deserialize)]
pub struct FrontendDiagnosticLogRequest {
    pub level: String,
    pub scope: String,
    pub event: String,
    pub message: Option<String>,
    pub fields: BTreeMap<String, String>,
}

#[tauri::command]
pub async fn write_frontend_diagnostic_log(
    app_state: State<'_, AppState>,
    request: FrontendDiagnosticLogRequest,
) -> Result<(), String>;
```

```ts
import { invoke } from '@tauri-apps/api/core';

export async function writeFrontendDiagnosticLog(
  payload: FrontendDiagnosticPayload,
): Promise<void> {
  await invoke('write_frontend_diagnostic_log', { request: normalizeDiagnosticPayload(payload) });
}
```

### 3.4 文件分流规则（伪代码）

```rust
fn file_path_for(channel: DiagnosticChannel) -> PathBuf {
    match channel {
        DiagnosticChannel::Backend => paths.backend_path,
        DiagnosticChannel::Frontend => paths.frontend_path,
    }
}

fn serialize_record(record: &DiagnosticRecord) -> String {
    // 单行文本协议，便于直接查看与追加写入：
    // [2026-04-11T13:20:15.123Z] INFO workspace-app event:received key=value ...
}
```

### 3.5 首批后端接入面（伪代码）

```rust
pub enum BackendDiagnosticSource {
    AgentLoop,          // handle_user_message / run_agent_loop
    ToolExecution,      // tool start / tool finish / tool error
    CommandExecution,   // run_command timeout / cancel / exit
    CliDebugToggle,     // /debug enable / disable
    PersistenceBoundary // save_task_state 等关键收尾节点
}
```

### 3.6 数据流向

```text
后端关键模块事件
  -> DiagnosticLogger::write_backend(record)
  -> .march/diagnostics/backend.log

前端显式 debug 调用
  -> FrontendDebugLogger.log(payload)
  -> Tauri invoke("write_frontend_diagnostic_log")
  -> DiagnosticLogger::write_frontend(record)
  -> .march/diagnostics/frontend.log

现有 CLI debug rounds
  -> DebugLogs::write_rounds(...)
  -> .march/debug/context.log + provider.log
  -> 不进入 Diagnostic Log 通道

## 4. 关键交互

### 4.1 后端关键链路默认记录

```text
用户触发一次任务
  -> AgentSession::handle_user_message(...)
  -> 写一条 backend info 日志: turn.started
  -> 进入 run_agent_loop
  -> 关键节点继续写日志:
     - context.built
     - model.requested
     - tool.started / tool.finished
     - turn.completed
  -> 开发者排障时直接查看 .march/diagnostics/backend.log
```

预期行为:
- 默认只记录选定的关键事件，不记录整段 prompt、完整响应、完整 stdout/stderr。
- 同一次任务的多条记录按时间追加，形成可回看的过程链。
- 新日志文件与 `.march/debug/*.log` 并行存在，互不覆盖。

### 4.2 前端调试时显式记录

```text
开发者在前端调试某个交互点
  -> 调用 FrontendDebugLogger.debug/info/warn/error(...)
  -> 先保持现有 console 可见调试体验
  -> 再 invoke Tauri command 写入 frontend.log
  -> 开发者排障时查看 .march/diagnostics/frontend.log
```

预期行为:
- 前端不做默认全局埋点，只有显式调用才会产生 Diagnostic Log。
- 单次前端记录失败不应影响页面主流程；落盘失败只允许降级为 console 可见告警。
- 前端写入目标与后端写入目标分开，避免同文件混杂。

### 4.3 命令异常 / 取消 / 超时

```text
run_command 开始
  -> backend info: command.started
  -> 命令运行中
  -> 分支:
     A. 正常结束 -> backend info/warn: command.finished
     B. 超时 -> backend warn/error: command.timed_out
     C. 取消 -> backend info/warn: command.cancelled
  -> 统一走现有收尾流程
```

预期行为:
- 三条路径都进入同一套 Diagnostic Logger 调用约定，避免正常/取消/超时分支各打一套不同格式的日志。
- 只记录足够定位问题的摘要字段，如命令类型、退出原因、时长；不默认灌入完整输出。

### 4.4 边界情况

- **`.march/diagnostics/` 不存在**
  - 预期: Logger 在首次写入前自动创建目录，不要求用户手动准备。

- **前端桥接调用失败**
  - 预期: 不阻塞原本交互；调用方可看到 console warn，但不会导致页面逻辑失败。

- **日志级别传入非法值**
  - 预期: 桥接层拒绝非法值并返回错误；调用方不能静默写入未知等级。

- **高频事件误接入**
  - 预期: 第一版通过“只允许关键模块 + 显式调用”的接入策略控制噪音，而不是依赖事后筛选。

### 4.5 被否的替代方案

- **方案 A: 直接复用 `.march/debug/`，把诊断日志也写进去**
  - 否决原因: 与 PRD 明确要求冲突。诊断日志必须与现有 Debug Output 分流，否则 runtime/debug 噪音会继续掩盖排障记录。

- **方案 B: 前后端统一写同一个 `diagnostic.log`**
  - 否决原因: 与用户明确要求“前后端 logger 不是一个、输出位置不同”冲突，也会让排障时重新面对混杂问题。

## 5. 改动计划

### 5.1 新增

- **后端诊断日志模块**
  - 路径: `crates/march-core/src/diagnostics.rs`
  - 职责: 定义 `DiagnosticLevel`、`DiagnosticRecord`、`DiagnosticLogger`、目录/文件分流规则，以及统一追加写入逻辑。

- **前端诊断日志桥接命令**
  - 路径: `src-tauri/src/lib.rs`
  - 职责: 暴露 `write_frontend_diagnostic_log` 一类 Tauri command，把前端显式调试请求转换成后端 `DiagnosticRecord` 并落盘。

- **前端调试 logger 抽象**
  - 路径: `src/lib/frontendDiagnosticLogger.ts`
  - 职责: 提供 `debug/info/warn/error` 级别 API，承接前端显式调试调用，并负责 invoke Tauri command。

### 5.2 调整

- **CLI 现有 debug 输出与新诊断日志职责拆分**
  - 路径: [crates/march-core/src/main.rs](/D:/playground/MA/crates/march-core/src/main.rs:49)
  - 调整点: 保留 `DebugLogs` 的 `.march/debug/` 职责；若需要在 CLI 主链路接入后端 Diagnostic Logger，应把新逻辑从 `main.rs` 抽走，避免继续把文件写入细节堆在入口文件。

- **后端运行主链路接入诊断日志**
  - 路径: [crates/march-core/src/agent/runner.rs](/D:/playground/MA/crates/march-core/src/agent/runner.rs:47)
  - 调整点: 在 `run_agent_loop` 关键节点补 backend diagnostics 记录，如 `turn.started`、`model.requested`、`tool.started`、`tool.finished`、`turn.completed`。

- **命令执行链路接入诊断日志**
  - 路径: `crates/march-core/src/agent/shells.rs`, `crates/march-core/src/agent.rs`
  - 调整点: 对 `run_command` 的开始、完成、取消、超时统一走同一套诊断日志调用，满足 AGENTS.md 对三条收尾路径收敛的要求。

- **项目级 `.march` 路径解析复用**
  - 路径: [crates/march-core/src/paths.rs](/D:/playground/MA/crates/march-core/src/paths.rs:28)
  - 调整点: 新的 Diagnostic Logger 统一基于 `resolve_project_root` 定位项目级 `.march`，不直接依赖调用方 `current_dir()`。

- **前端现有 `debugChat` 调用入口适配**
  - 路径: [src/lib/chatDebug.ts](/D:/playground/MA/src/lib/chatDebug.ts:10)
  - 调整点: 保留现有 console 调试体验，同时为需要落盘的显式调试场景提供升级入口；避免让所有现有 `debugChat` 默认开始写盘。

- **首批前端显式接入点**
  - 路径: [src/main.ts](/D:/playground/MA/src/main.ts:9), [src/App.vue](/D:/playground/MA/src/App.vue:307), [src/composables/useWorkspaceApp.ts](/D:/playground/MA/src/composables/useWorkspaceApp.ts:243)
  - 调整点: 仅挑少量高价值调试点验证前端 logger 调用链，不默认扩散到所有 `debugChat` 调用点。

### 5.3 删除

- **无**

## 6. 推进顺序

### 步骤 1：落地后端 Diagnostic Logger 骨架
- **前置依赖**: 无
- **改动范围**: `crates/march-core/src/diagnostics.rs`，必要时调整模块导出
- **退出信号**: 能基于项目根 `.march` 自动创建 `diagnostics/` 目录，并支持把一条 backend/frontend 伪造记录分别追加到不同文件
- **人工 checkpoint**: 确认目录结构、文件命名和等级枚举都与本方案 doc 一致

### 步骤 2：把 CLI / 后端主链路接入 backend diagnostics
- **前置依赖**: 步骤 1
- **改动范围**: `crates/march-core/src/main.rs`、`crates/march-core/src/agent/runner.rs`
- **退出信号**: 一次真实或测试任务跑通后，`backend.log` 中能看到最小闭环的关键事件（例如 turn started / tool finished / turn completed）
- **人工 checkpoint**: 确认新日志没有与 `.march/debug/*.log` 混写，且没有默认记录整段大对象内容

### 步骤 3：把命令执行异常链路并入同一套日志
- **前置依赖**: 步骤 2
- **改动范围**: `crates/march-core/src/agent/shells.rs`、`crates/march-core/src/agent.rs` 或相关命令执行封装
- **退出信号**: 正常结束、超时、取消三条路径都能产出同一格式的 backend diagnostics 摘要记录
- **人工 checkpoint**: 确认三条路径共用同一套记录语义，没有分支级格式漂移

### 步骤 4：新增前端桥接 command
- **前置依赖**: 步骤 1
- **改动范围**: `src-tauri/src/lib.rs`
- **退出信号**: 前端可通过 invoke 成功写入一条 `frontend.log` 记录；非法等级会被桥接层拒绝
- **人工 checkpoint**: 确认桥接失败不会影响页面主流程

### 步骤 5：落地前端 Frontend Debug Logger 并接入少量调试点
- **前置依赖**: 步骤 4
- **改动范围**: `src/lib/frontendDiagnosticLogger.ts`、`src/lib/chatDebug.ts`、少量前端调用点
- **退出信号**: 至少一个前端调试场景能显式写入 `frontend.log`，同时保留现有 console 调试体验
- **人工 checkpoint**: 确认不是所有 `debugChat` 调用都开始默认落盘

### 步骤 6：补充测试与回归验证
- **前置依赖**: 步骤 2-5
- **改动范围**: 后端单元测试/集成测试、必要的前端调用链验证
- **退出信号**: 能验证 `.march/diagnostics/` 的目录创建、后端/前端文件分流、等级校验、以及命令异常路径记录
- **人工 checkpoint**: 确认 PRD 的“不做什么”仍然成立，没有偷偷扩展到 UI 或全局埋点

## 7. 不变量清单

### I1: Diagnostic Log 永远写入项目级 `.march/diagnostics/`，且不得回退到 `.march/debug/` 或调用方当前目录

**违反后果**: 开发者会在不同 task cwd、嵌套目录或既有 debug 目录中看到分裂的日志文件，导致排障时找错位置或把 Diagnostic Log 与 Debug Output 混淆。

**验证手段**:
- [x] 类型系统保证(编译期)
  通过把后端 `DiagnosticLogger::new` 的输入收敛为项目根路径，并统一复用 `resolve_project_root`，避免调用方自行拼路径。
- [x] 单测 / 集成测试断言(运行期)
  增加测试验证从嵌套工作目录触发时，日志仍写入工作区根目录下的 `.march/diagnostics/`。
- [ ] 运行时 debug assert + 日志(兜底,要说明为什么前两种不可行)
- [ ] 人工走查(最后兜底,要说明为什么无法自动化)

**验证测试用例**:
- `diagnostics::tests::new_creates_project_scoped_diagnostics_directory`

### I2: Backend Diagnostic Log 与 Frontend Diagnostic Log 必须分文件持久化，且两者都不得与现有 `.march/debug/*.log` 混写

**违反后果**: 前后端排障记录重新被噪音混在一起，用户可感知行为退化回“先翻混杂日志”，直接违反 PRD 成功标准。

**验证手段**:
- [ ] 类型系统保证(编译期)
  路径是运行时值，无法仅靠类型系统证明不混写。
- [x] 单测 / 集成测试断言(运行期)
  通过后端文件分流测试与前端桥接集成测试同时断言 `backend.log` / `frontend.log` / `.march/debug/*.log` 各自独立。
- [ ] 运行时 debug assert + 日志(兜底,要说明为什么前两种不可行)
- [ ] 人工走查(最后兜底,要说明为什么无法自动化)

**验证测试用例**:
- `diagnostics::tests::backend_and_frontend_records_are_written_to_separate_files`
- `tests::diagnostic_event_writer_records_minimal_backend_turn_flow`

### I3: Diagnostic Logger 是新增诊断记录的单一写路径，业务模块不得各自直接 `fs::write` / `console` 落盘到 `.march/diagnostics/`

**违反后果**: 不同模块会逐渐产生格式漂移、目录漂移和等级语义漂移，后续新增点位时无法保证日志可检索、可对比。

**验证手段**:
- [x] 类型系统保证(编译期)
  通过把 `.march/diagnostics/` 写入能力封装在独立模块/接口中，并让接入模块只依赖该抽象。
- [x] 单测 / 集成测试断言(运行期)
  用模块级测试覆盖统一 writer 的序列化与追加写入行为，避免调用方复制写文件逻辑。
- [ ] 运行时 debug assert + 日志(兜底,要说明为什么前两种不可行)
- [ ] 人工走查(最后兜底,要说明为什么无法自动化)

**验证测试用例**:
- `diagnostics::tests::backend_and_frontend_records_are_written_to_separate_files`

### I4: Frontend Diagnostic Log 的 payload 缺省字段不得在桥接或序列化过程中被默默覆盖成错误值或 `undefined` 字面量

**违反后果**: 日志记录会出现字段丢失、`undefined` 被当成字符串写入、或 message/fields 语义漂移，导致排障信息不可信。

**验证手段**:
- [x] 类型系统保证(编译期)
  通过前端 payload 类型与桥接请求结构把可选字段边界写死，禁止未声明字段直接透传。
- [x] 单测 / 集成测试断言(运行期)
  增加序列化/桥接测试，验证 `message`、`fields` 缺省时被正确省略或保留为空，而不是被写成错误值。
- [ ] 运行时 debug assert + 日志(兜底,要说明为什么前两种不可行)
- [ ] 人工走查(最后兜底,要说明为什么无法自动化)

**验证测试用例**:
- `tests::persist_frontend_diagnostic_log_writes_frontend_log_file`
- 阶段五人工浏览器验证：前端诊断写盘失败时，页面初始化与交互不应中断

### I5: `run_command` 的正常结束、超时、取消三条路径必须都产出同一语义体系下的摘要诊断记录

**违反后果**: 命令相关问题在不同结束路径下会留下不一致的诊断痕迹，用户会重新遇到“某些问题只能靠临时加打印才能看懂”的情况。

**验证手段**:
- [ ] 类型系统保证(编译期)
  三条路径的一致性涉及运行时分支，无法仅靠类型系统保证。
- [x] 单测 / 集成测试断言(运行期)
  增加命令执行集成测试，分别覆盖正常、超时、取消三条路径，验证都能写出统一格式的 backend diagnostics。
- [x] 运行时 debug assert + 日志(兜底,要说明为什么前两种不可行)
  命令链路涉及异步子进程与取消收尾；即使有测试，也保留运行时错误日志作为现场兜底。
- [ ] 人工走查(最后兜底,要说明为什么无法自动化)

**验证测试用例**:
- `agent::session::tests::run_command_writes_finished_diagnostic_log`
- `agent::session::tests::run_command_writes_timeout_diagnostic_log`
- `agent::session::tests::run_command_writes_cancelled_diagnostic_log`

### I6: Frontend Debug Logger 的落盘失败不得中断页面原有交互流程

**违反后果**: 调试辅助能力反过来污染正常 UI 行为，用户会因为写日志失败而遇到页面报错、事件中断或启动失败。

**验证手段**:
- [ ] 类型系统保证(编译期)
  失败降级是运行时行为，无法仅靠类型系统保证。
- [x] 单测 / 集成测试断言(运行期)
  需要覆盖前端 invoke 失败时的降级路径，验证调用点仍继续执行且只留下 console warn。
- [x] 人工走查(最后兜底,要说明为什么无法自动化)
  该不变量涉及前端交互与调试体验，阶段五需要在真实界面中确认失败不会破坏 UI 流程。
- [ ] 运行时 debug assert + 日志(兜底,要说明为什么前两种不可行)

**验证测试用例**:
- `tests::persist_frontend_diagnostic_log_writes_frontend_log_file`

### I7: 第一版 Diagnostic Log 默认只记录设计中选定的关键事件，不得自动升级为全量上下文/全局埋点采集

**违反后果**: 日志体量迅速膨胀，真正有价值的诊断记录再次被噪音淹没，并直接突破 PRD 的“不做什么”边界。

**验证手段**:
- [ ] 类型系统保证(编译期)
  记录范围属于产品边界，无法单靠类型系统证明。
- [x] 单测 / 集成测试断言(运行期)
  通过接入点测试与回归测试约束只有设计列出的关键模块/显式调用点会产生记录。
- [x] 人工走查(最后兜底,要说明为什么无法自动化)
  需要在阶段五人工检查首批前端/后端接入面，确认没有把所有现有 `debugChat` 或高频事件一口气接进来。
- [ ] 运行时 debug assert + 日志(兜底,要说明为什么前两种不可行)

**验证测试用例**:
- `tests::diagnostic_event_writer_records_minimal_backend_turn_flow`

## 8. 与项目级架构文档的关系

- **引用的项目级架构文档**
  - [easysdd/architecture/DESIGN.md](/D:/playground/MA/easysdd/architecture/DESIGN.md:1)
    - 依赖点: `.march` 作为项目级 Source of Truth 目录、`runtime_status` / `debug_trace` 与聊天内容分层、运行态与持久化事实分离。
  - [easysdd/architecture/INDEX.md](/D:/playground/MA/easysdd/architecture/INDEX.md:1)
    - 依赖点: 确认当前没有独立的“日志/诊断”子系统文档，本方案需在 design doc 中先显式声明与现有架构文档的关系。
  - [easysdd/architecture/context.md](/D:/playground/MA/easysdd/architecture/context.md:16)
    - 依赖点: `runtime_status` 是上下文里的运行态快照，不应和本次新增的 Diagnostic Log 混成同一概念。
  - [easysdd/architecture/ui-events.md](/D:/playground/MA/easysdd/architecture/ui-events.md:423)
    - 依赖点: `runtime` / `debug_trace` 已有独立事件通道；第一版 Diagnostic Log 不新增新的 UI 事件协议。

- **是否需要更新项目级架构文档**
  - **需要**。
  - 原因: 当前项目级架构文档已经对 `.march` 下的 `memories/`、`agents/`、`skills/`、`config.toml`、`march.db` 等项目级内容给出了稳定语义，但尚未定义 `diagnostics/` 这一新目录及其与 `.march/debug/` 的职责边界。根据 AGENTS.md，架构讨论的最终落点必须在 `easysdd/architecture/`，不能只留在 feature 方案 doc。

- **计划同步方式**
  - 在项目级架构文档中补一段关于 `.march/diagnostics/` 的职责说明，至少包括：
    - 这是项目级持久化诊断记录目录；
    - 它与 `.march/debug/` 的区别；
    - 前后端分文件而非同文件混写；
    - 它不等于 `runtime_status`、`debug_trace` 或 UI 事件流。
  - 优先落点建议:
    - `easysdd/architecture/DESIGN.md` 中补一段高层职责说明；
    - 如实现后内容较多，再补独立子文档并在 `INDEX.md` 与 `DESIGN.md` 中引用。
```
