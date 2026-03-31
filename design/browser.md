# 浏览器与电脑操作

> 从 [DESIGN.md](DESIGN.md) 延伸：文本优先，截图按需，操作用户真实环境而非沙盒。

## 核心原则

文本优先，截图按需——大部分编程任务不需要看屏幕，文件内容、命令输出、错误日志直接拿文本即可。截图只在必要时由 AI 主动请求。

---

## 看：截图策略

不主动截图，AI 按需请求。截图 + 可交互元素列表结合，减少 token 消耗：

```
screenshot: <图片>
clickable_elements:
  [12, 45]   "登录按钮"
  [80, 200]  "搜索框"
  [300, 150] "导航链接: 首页"
```

AI 看截图理解页面，看坐标列表决定点哪里，不传完整 DOM。

---

## 动：操作用户自己的浏览器

不用 Playwright/Puppeteer——它们默认起空白浏览器，拿不到用户已有的 session、cookie、登录状态。

使用 **Chrome DevTools Protocol (CDP)**，直接连接用户正在运行的浏览器：

```bash
# 用户一次性设置
google-chrome --remote-debugging-port=9222
```

连上后 Ma 可以读取当前页面 DOM、截图，模拟点击输入滚动，完整保留用户的 session 和 cookie。

---

## 鼠标键盘操作（桌面 GUI）

优先用系统级 accessibility API 绕过坐标模拟：
- macOS：AXUIElement
- Windows：UIA (UI Automation)
- Linux：AT-SPI

鼠标坐标模拟（`xdotool` 等）作为最后手段，分辨率/窗口位置变化容易失效。
