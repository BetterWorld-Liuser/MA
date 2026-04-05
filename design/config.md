# 配置设计

> 配置分两类：用户通过 UI 操作的运行时设置，以及开发者放在项目里的文本配置。两者职责不同，存储位置不同。

---

## UI 配置（Provider 等）

Provider 相关的设置（API key、model、base_url）全部在 UI 里完成，March 自动持久化到用户级存储 `~/.march/settings.db`，用户不需要手动编辑任何文件。

这样设计的原因：
- API key 是敏感信息，不应出现在可能被提交进 git 的配置文件里
- Provider 设置是用户维度的，不属于项目
- UI 配置比手写 TOML 更友好，也更容易做校验（测试连通性等）

`~/.march/settings.db` 由 March 独占管理，不是用户可编辑的格式。文件权限设为 `600`。

Schema 及 Provider 数据结构详见 → [Provider 设计](provider.md)

---

## 文本配置（`config.toml`）

只保留适合以文本形式存在于项目仓库里的设置。**项目级覆盖用户级，同名字段以项目级优先。**

### 用户级 `~/.march/config.toml`

```toml
[context]
recent_turns       = 3   # recent_chat 保留轮数，默认 3
pressure_threshold = 80  # 上下文用量达到多少 % 时向 AI 发出压力提示，默认 80
```

### 项目级 `.march/config.toml`

```toml
[skills]
disable = ["git"]                 # 在该项目下屏蔽特定 skill
use_builtin_triggers = true       # 是否启用内置自动触发规则，默认 true

[[skills.triggers]]
paths = ["package.json", "tsconfig.json"]
skills = ["node", "typescript", "frontend"]

[[skills.triggers]]
paths = ["deploy.yaml", "infra.yaml"]
skills = ["k8s", "deploy"]
```

`skills.triggers` 支持**多对多**：

- 一个或多个文件命中后，可以同时激活多个 skill
- 同一个 skill 也可以被多条规则、多个文件共同激活
- 用户级和项目级的 `skills.triggers` 会合并；若项目级将 `use_builtin_triggers = false`，则只保留自定义规则

---

## 不配置的内容

以下内容硬编码为合理默认值，不暴露配置项：

| 内容 | 默认值 | 原因 |
|------|--------|------|
| Watcher debounce | 300ms | 用户几乎不需要调整 |
| 网络重试次数 | 5 次 | 固定策略即可 |
| 本地 socket 路径 | `.march/march.sock` | 约定优于配置 |
| 上下文窗口大小 | 从 provider 读取 | 模型自带元信息 |

---

## MVP 范围

多 Agent 配置（不同任务使用不同 model/provider）暂不实现，MVP 阶段单一 provider、单一 agent。
