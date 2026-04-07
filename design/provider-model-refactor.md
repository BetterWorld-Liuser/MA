# Provider / Model 架构重构变更说明

> 本文档描述对 [provider.md](provider.md) 的计划变更，实现完成后合并入主文档。

---

## 变更动机

现有设计以 Provider 为主要配置实体，Model 作为 Provider 的子配置。但用户的心智模型是"我想用某个模型"，而不是"我想配置某个 Provider"。此次重构将 Model 提升为运行时一级实体，Provider 降级为模型的来源通道（凭证 + 端点）。

---

## 核心概念变化

### 之前

```
Provider（一级实体，用户直接配置）
  └── Model（子配置，挂在 Provider 下）
```

### 之后

```
Provider（来源通道：凭证 + 端点 + 协议类型）
    ↓ 激活或挂接模型
    ↓
Model（运行实体，扁平列表，用户日常面对的）
```

两者关系是**来源 vs 实体**，不是父子层级。Model 记录自己从哪个 Provider 激活，但运行时只看 Model。

---

## Provider 两种类型的配置路径

Provider 分为**官方 Provider** 和**自定义 Provider（中转站）**，两者的配置路径完全不同。

### 官方 Provider（Anthropic / OpenAI / Gemini）

March 内置这些 provider 的全部细节：协议适配、可用模型列表、模型能力、server-side tools。用户只需填写 API key，无需探测。

```
Anthropic 可用模型

  [✓] claude-sonnet-4-6    200K · 16K · 工具 · 图片 · 搜索 · 代码执行   [覆盖]
  [✓] claude-opus-4-6      200K · 32K · 工具 · 图片 · 搜索 · 代码执行   [覆盖]
  [ ] claude-haiku-4-5     200K · 8K  · 工具 · 图片
  ─────────────────────────────────────────────────────
  [+ 添加自定义 Model ID]
```

- 能力字段由 March 预填，**默认只读**
- 提供 **[覆盖]** 入口，供 provider 更新了模型能力但 March 版本尚未跟上时手动修正；覆盖后显示标记，可一键恢复内置值
- **[+ 添加自定义 Model ID]**：用于内置列表里尚未收录的新模型（如预览版），走手动填写路径，不走自动探测

### 自定义 Provider（中转站，OpenAICompat）

用户提供 base URL 和 API key 后，**供应商类型就已经决定了协议族**。这里不再额外做协议自动探测，也不把“协议识别”包装成供应商侧的自动流程；provider 页只负责让用户显式确认类型、填写凭据和测试连通性。后续模型候选列表、能力填写与 server-side tools 确认，都建立在这个已确认的 provider type 之上。

---

## ProviderType 统一承担协议语义

`ProviderType` 现在同时承担“来源通道类型”和“运行协议选择”两层语义。当前产品里，一个 `provider_type` 就对应唯一的请求协议分支，因此不再单独持久化 `WireFormat` 字段，也不在模型表单里暴露单独的协议项。运行时翻译层直接按 `ProviderType` 选择 `WireAdapter` 与端点分支；设置页需要展示协议时，也直接在“来源供应商”名称上带出，例如 `益丰 · OpenAI-compatible`。

---

## 数据模型变更

### ProviderConfig（基本不变）

```rust
struct ProviderConfig {
    id:            i64,
    name:          String,
    provider_type: ProviderType,
    api_key:       String,
    base_url:      Option<String>,
}
```

### ModelConfig（原 ProviderModel，提升为一级实体）

```rust
struct ModelConfig {
    id:            i64,
    display_name:  String,           // 用户可重命名
    model_id:      String,           // API 调用实际使用的 ID
    provider_id:   i64,              // 来源通道
    capabilities:  ModelCapabilities,// 内置预填或用户手动确认
}
```

### DefaultModel（简化）

```rust
// 之前：(provider_id, model_id) 组合
// 之后：直接引用 ModelConfig.id
struct DefaultModel {
    model_config_id: i64,
}
```

### Storage Schema 变更

```sql
-- 原 provider_models 表重命名并扩展
CREATE TABLE model_configs (
    id               INTEGER PRIMARY KEY,
    provider_id      INTEGER NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    model_id         TEXT    NOT NULL,
    display_name     TEXT    NOT NULL,
    context_window   INTEGER NOT NULL DEFAULT 131072,
    max_output       INTEGER NOT NULL DEFAULT 4096,
    supports_tool_use  INTEGER NOT NULL DEFAULT 0,
    supports_vision    INTEGER NOT NULL DEFAULT 0,
    supports_audio     INTEGER NOT NULL DEFAULT 0,
    supports_pdf       INTEGER NOT NULL DEFAULT 0,
    UNIQUE(provider_id, model_id)
);

-- model_server_tools 表的 provider_id/model_id 对改为引用 model_config_id
-- 原：REFERENCES providers(id) + model_id TEXT
-- 新：
CREATE TABLE model_server_tools (
    id               INTEGER PRIMARY KEY,
    model_config_id  INTEGER NOT NULL REFERENCES model_configs(id) ON DELETE CASCADE,
    capability       TEXT    NOT NULL,
    format           TEXT    NOT NULL,
    UNIQUE(model_config_id, capability)
);

-- settings 表中 default_model 的值改为 model_config_id（整数字符串）
-- 原：default_provider_id + default_model_id 两个 key
-- 新：default_model_config_id 一个 key
```

---

## 新增：接入与确认流程

### 连通性测试（Provider 级）

用户配置好 provider type、base URL 和 API key 后，March 直接按该类型对应的协议发一个最小请求，确认"这条通道是否可用"：

| Provider Type | 连通性测试方式 |
|---------------|----------------|
| OpenAI / OpenAICompat | `POST {base_url}/v1/chat/completions`（或官方默认端点） |
| Anthropic | `POST {base_url}/v1/messages` |
| Gemini | `POST {base_url}/v1beta/models/{model}:generateContent` |

请求体仍然保持最小：`system: ""` + `user: "hi"` + `max_tokens: 1`。  
这里的测试结论只有"可连通 / 不可连通"，**不再顺便做协议探测**。供应商页的职责到这里为止：类型由用户确认，连通性由程序验证。

### 候选模型发现（Provider 级，可选）

连通性确认后，可以按 provider type 对应端点拉取候选模型列表，作为模型接入时的辅助信息：

| Provider Type | 端点 |
|---------------|------|
| OpenAI-compat | `GET {base_url}/v1/models`（若端点支持） |
| Anthropic | （无标准 list endpoint，返回内置模型列表） |
| Gemini | `GET {base_url}/v1beta/models` |

已知 provider（Anthropic / OpenAI / Gemini）直接展示内置模型列表，不需要网络请求。  
自定义 provider 的候选模型列表只是“辅助选择”，不是自动探测的一部分；拿不到列表时，仍然允许用户手动填写 `model_id` 完成接入。

### 能力确认（Model 级）

用户从列表中选择一个模型，或手动填写 `model_id` 后，直接进入能力确认表单。这里不再发起自动能力探测，而是按来源类型走两条路径：

- 已知 provider（Anthropic / OpenAI / Gemini）的已知模型：由 March 预填能力与 server-side tools，用户可按需覆盖
- 自定义 provider，或已知 provider 下的自定义 `model_id`：给出一组保守默认值，由用户手动勾选和填写

**基础能力字段：**

| 字段 | 来源 | 默认值策略 |
|------|------|-----------|
| tool_use | 内置表或用户输入 | 未知模型默认关闭 |
| vision | 内置表或用户输入 | 未知模型默认关闭 |
| audio | 内置表或用户输入 | 未知模型默认关闭 |
| pdf | 内置表或用户输入 | 未知模型默认关闭 |
| context_window | 内置表或用户输入 | 未知模型给保守默认值 |
| max_output | 内置表或用户输入 | 未知模型给保守默认值 |

**Server-side tools：**

Server-side tools 同样不做自动探测，改为按 provider type 给出候选项，再由用户确认：

| Provider Type | 可能支持的 server-side tools |
|---------------|---------------------------|
| Anthropic | web_search、code_execution |
| OpenAI | web_search_preview、code_interpreter、file_search |
| Gemini | google_search、code_execution |

确认表单中，对应 provider type 下的 server-side tools 默认全部**关闭**，用户手动勾选实际可用的项（与 `ServerToolFormat` 一起写入 `model_server_tools` 表）。

已知 provider（Anthropic / OpenAI / Gemini）的内置模型由 March 预填，用户只在需要修正时才改动。

这样做的目的，是把“模型能做什么”收敛为一份稳定配置，而不是引入一套昂贵且不可靠的探测流水线。

### 确认结果处理

```
连通性测试完成
    ↓
展示能力表单供用户确认（可手动修改任意项）
┌──────────────────────────────────────────────────┐
│ 通道类型    Anthropic                            │
│ 工具调用    ✓                                    │
│ 图片        ✓                                    │
│ 流式        ✓                                    │
│ PDF         ✗                                    │
│                                                  │
│ Server-side Tools（按 Anthropic 类型预填，默认关闭）│
│   [ ] Web Search                                 │
│   [ ] Code Execution                             │
└──────────────────────────────────────────────────┘
    ↓ 用户确认
写入 model_configs 表，model 出现在[模型列表]中
```

### 可靠性边界

- **未知模型信息缺失**：中转站或预览模型没有可靠元数据时，March 只能给保守默认值，最终以用户确认为准
- **供应商实现差异**：同属某个 `provider_type` 的中转站，server-side tools 与多模态支持可能并不完整，因此默认不自动打开
- **后续能力变化**：provider 侧升级或降级模型能力后，March 不主动追踪；若用户发现配置过时，应手动编辑修正

---

## 设置页 UI 变更

### 收敛原则

这次重构之后，设置页只保留一套心智模型：

- **模型**：用户真正选择和绑定的运行实体
- **连接通道**：模型的来源通道，只负责凭据、端点、扫描/激活来源
- **默认运行**：应用级默认值，但入口收进模型页，由用户直接把某个 `ModelConfig` 标记为默认

设置页导航也应跟着职责拆开，而不是继续用一个“模型与通道”的混合入口：

- **模型页**：负责浏览已接入模型、设置默认标记、编辑模型元数据、确认模型能力与 server-side tools
- **供应商页**：负责配置供应商凭据与端点、确认供应商类型、测试连通性，并在可用时提供候选模型列表

不再让用户在单独的“默认运行”页面里先选 provider 再选 model，也不再把聊天里的模型下拉理解成“某个 provider 当前可读到的原始模型列表”。

聊天输入框和默认运行使用的都是**已接入的模型列表**，也就是已经写入 `model_configs` 的实体；如果用户想新增模型，应回到“连接通道”中扫描、激活或手动补充，而不是在运行时临时从 provider 原始列表里挑。

### 之前（Provider 为主）

```
[Provider 配置]
  Anthropic          claude-sonnet-4-6   [编辑] [删除]
  Local Ollama       qwen2.5-coder:32b   [编辑] [删除]
  [+ 新增 Provider]

[默认模型]
  Anthropic / claude-sonnet-4-6   [修改]
```

### 之后（Model 为主）

```
[模型]
  Claude Sonnet 4.6     Anthropic · 工具 · 图片 · 搜索 · 代码执行   [编辑能力] [删除]
  Qwen 2.5 Coder        中转站A · OpenAI-compatible · 工具          [编辑能力] [删除]

[连接通道]
  Anthropic        ✓ 已配置    [管理模型] [编辑 Key] [删除]
  中转站A · OpenAI-compatible  [管理模型] [编辑]     [删除]
  [+ 添加通道]
```

**[管理模型]**（官方 Provider）：展示内置模型列表，勾选激活，支持覆盖能力和添加自定义 Model ID。无需探测，能力由 March 内置。

**[管理模型]**（中转站）流程：

```
1. 按已选 provider type 做连通性测试
2. 若端点支持，则拉取候选模型列表；否则直接进入手动填写 model_id
3. 用户选择或填写要添加的模型
4. March 按 provider type 给出能力表单默认值
5. 用户确认能力与 server-side tools 后加入[模型]列表
```

**[编辑能力]**：对所有模型都可用。官方 Provider 的内置模型以 March 预填值为起点，修改后显示“已覆盖”；自定义 provider 的模型则直接编辑当前确认值。

### 模型编辑区中的“来源供应商”与“运行协议”

模型编辑区里不再单独放一个“运行协议”编辑块。协议信息属于来源通道的派生属性，应该跟着“来源供应商”一起展示，而不是让用户误以为这里还有第二次确认或自动识别：

```
来源供应商
  [ 益丰 · OpenAI-compatible ∨ ]

说明：
- `OpenAI-compatible` 直接来自该 provider 的 `provider_type`
- 切换来源供应商时，协议显示随之更新
- 运行协议不在这里单独编辑，也不宣称来自供应商侧自动探测
```

模型选择器下拉列表（聊天输入框）展示扁平的 `ModelConfig` 列表，不再按 Provider 分组：

```
✓ Claude Sonnet 4.6
  Qwen 2.5 Coder
  GPT-5.4
```

默认运行配置也改成直接绑定模型，但入口收进模型卡片：

```
[模型]
  Claude Sonnet 4.6   [设为默认]
  Qwen 2.5 Coder
  GPT-5.4

已设为默认的模型显示“默认”标记，不再重复出现单独的默认运行表单。

说明：
- 这里只决定之后新建任务的初始模型
- provider / server-side tools 都从该 ModelConfig 与其来源 provider 反查；协议分支由来源 provider type 直接决定
- 已存在任务保持自己的模型绑定，不会被新的默认值回刷
```

设置页的信息结构收敛为：

```
左侧导航
  外观
  模型
  供应商
  角色

模型
  扁平 ModelConfig 列表
  默认模型标记入口
  模型能力编辑表单

供应商
  Provider 列表
  当前选中供应商的编辑表单
  连通性测试 / 候选模型发现（可选）
```

也就是说：

- **模型页负责浏览、默认标记、能力摘要、编辑能力、删除**
- **供应商页负责新增 provider、编辑凭据、确认类型、管理模型来源；不承担协议自动探测**
- 若用户点“编辑模型”，界面可以联动带出所属供应商上下文，但不应把模型编辑表单塞回供应商页里

---

## 运行时变化

- `ProviderClient` 初始化时直接按 `ProviderType` 选择 `WireAdapter`
- `ModelCapabilities` 来源直接读 `ModelConfig`，解析优先级链简化为：
  ```
  1. 用户手动覆盖（通过设置页"编辑能力"修改）
  2. 内置模型能力表（已知 provider 的已知模型）
  3. 保守默认值（未知模型）
  ```
  内置模型能力表保留，作为"已知 provider 的已知模型"的默认值来源

---

## 对现有代码的影响

| 模块 | 变化 |
|------|------|
| `storage/tasks.rs` | task 的 `selected_model` 改为引用 `model_config_id` |
| `provider/transport.rs` | 继续由 `ProviderType` 决定底层协议分支与请求端点 |
| `provider/wire.rs` | 不变 |
| `storage.rs` / provider CRUD | 新增 `ModelConfig` CRUD，`provider_models` 表迁移为 `model_configs` |
| 前端 `SettingsPage` | 按新 UI 结构重组，Provider 和 Model 分开展示，默认运行通过模型页中的默认标记动作直接绑定 `model_config_id` |
| 前端模型选择器 | 数据源从“按 Provider 分组的可读模型列表”改为扁平 `ModelConfig` 列表 |
