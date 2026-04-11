# 图片输入

> 从 [DESIGN.md](DESIGN.md) 延伸：图片作为消息级内容流转，不进入文件追踪通道。

## 前提

模型必须支持图片输入（multimodal vision）。March 在 session 初始化时通过 `ModelCapabilities.supports_vision` 判断当前模型是否支持图片（详见 → [模型能力解析](provider.md#模型能力解析)）。不支持时：

- `view_image` 工具不注入 tools 列表，AI 不会尝试调用
- 聊天框的图片粘贴/拖入入口禁用或隐藏
- 用户 `@` 引用图片文件时，March 返回提示：当前模型不支持图片输入

---

## 核心判断：图片是时间点快照，不是活文档

`open_files` 的设计基础是 Source of Truth——watcher 持续追踪磁盘状态，内容随文件变化自动刷新。图片文件不符合这个模型：

- 二进制文件已被 `open_file` 的 null byte 检测拒绝
- 图片内容无法以文本形式渲染进上下文
- 图片不需要持续追踪——看一次就够了，变了再看一次

因此图片走**消息通道**，不走**文件追踪通道**。

---

## 输入向量

| 来源 | 处理方式 |
|------|---------|
| 用户在聊天框粘贴/拖入图片 | 作为 user message 的 image content block |
| 用户 `@path.png` 引用图片文件 | March 读取文件，转为 image content block 注入 user message |
| AI 调用 `view_image(path)` | tool result 返回 image content block，轮内可见 |
| 浏览器截图 / 屏幕截图工具 | tool result 返回 image content block |

所有路径最终都归结为 API 的 image content block，不进 `open_files`。

---

## 与现有分层的关系

```
[open_files]     ← 纯文本，watcher 持续追踪，二进制已被拒绝，语义不变
[recent_chat]    ← 图片作为消息附件在这里流转
轮内历史          ← tool result 中的图片在当前轮可见，轮结束后丢弃
```

- **`open_file` 对图片的行为不变**：二进制检测命中 → 拒绝，返回错误提示 AI 改用 `view_image`
- **`recent_chat` 承载图片**：用户消息和 AI 回复中的图片随 `recent_chat` 保留，受同样的 10 轮窗口管理
- **轮内 tool result**：AI 通过工具获取的图片只在本轮可见；如果跨轮需要，AI 应在 Notes 里记录文字描述（图片本身不适合写进 Notes）

---

## 工具：`view_image`

```rust
view_image {
    path: String,                // 文件路径
    max_dimension: Option<u32>,  // 可选，长边缩放上限（省 token）
}
// → 返回 image content block 作为 tool result
```

不需要 `open_image` / `close_image`——看一次就够了，不需要持续追踪。图片变了想再看，再调一次 `view_image`。

---

## Token 预算管理

图片吃 token 很凶（一张 1080p 截图约 1500+ tokens），需要显式管控：

- **缩放**：注入前按 `max_dimension` 缩放，默认长边不超过 1568px（Anthropic 推荐上限）
- **格式**：统一转为 JPEG（除非是需要透明通道的 PNG），quality 80 左右
- **recent_chat 中的图片淘汰**：图片消息比纯文本消息老化更快——图片附件在 recent_chat 中只保留最近 3-5 轮，更老的轮次只保留文字部分
- **上下文压力**：图片 token 计入总量，`context_pressure` 机制照常生效

---

## 数据结构变更

### 多模态消息内容

`recent_chat` 的消息内容从纯 `String` 变为多模态内容块列表：

```rust
enum ContentBlock {
    Text(String),
    Image {
        data: Vec<u8>,                  // 已处理后的图片数据
        media_type: String,             // "image/jpeg" | "image/png"
        source_path: Option<PathBuf>,   // 来源路径（如有）
    },
}

struct ChatTurn {
    role: Role,
    content: Vec<ContentBlock>,  // 替代原来的 content: String
    tool_summaries: Vec<String>,
    timestamp: SystemTime,
}
```

### 持久化

`conversation_turns` 表的 `content` 列改为存 JSON 数组（`Vec<ContentBlock>` 序列化）。图片数据有两种存储策略：

- **内联**：图片 base64 编码直接存入 JSON，实现简单，适合图片数量少的场景
- **拆表**（推荐）：图片数据存单独的 `blobs` 表，`content` JSON 里只放引用 ID，避免大表扫描时加载图片二进制数据

```sql
-- 图片二进制数据独立存储
CREATE TABLE blobs (
    id         INTEGER PRIMARY KEY,
    task_id    INTEGER NOT NULL REFERENCES tasks(id),
    media_type TEXT    NOT NULL,  -- "image/jpeg" | "image/png"
    data       BLOB    NOT NULL,
    created_at INTEGER NOT NULL
);
```

`ContentBlock::Image` 序列化时只写 `{"type": "image", "blob_id": 42, "source_path": "..."}`, 渲染进上下文时再从 `blobs` 表按需加载。
