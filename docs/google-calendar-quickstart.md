# Google Calendar MCP - 快速开始

配置 Google Calendar MCP Server 只需 3 步！

## 🚀 快速配置（5 分钟）

### 第 1 步：运行自动配置脚本

```bash
cd /Users/user/Downloads/Selenium/zero/zeroclaw
bash scripts/setup-google-calendar-mcp.sh
```

这将自动：
- ✅ 检查 Node.js 环境
- ✅ 安装 MCP 服务器
- ✅ 创建配置文件

### 第 2 步：获取 Google API 凭据

1. **访问 Google Cloud Console**  
   👉 https://console.cloud.google.com/

2. **创建项目**
   - 点击顶部项目选择器
   - "新建项目" → 输入名称（如 "ZeroClaw Calendar"）
   - 点击"创建"

3. **启用 Calendar API**  
   👉 https://console.cloud.google.com/apis/library/calendar-json.googleapis.com
   - 点击"启用"

4. **创建 OAuth 凭据**
   - 左侧菜单："凭据"
   - 点击"创建凭据" → "OAuth 客户端 ID"
   - 首次需配置"同意屏幕"（选择"外部"，填写基本信息）
   - 应用类型：**桌面应用**
   - 名称：`ZeroClaw Calendar Client`
   - 点击"创建"

5. **下载凭据文件**
   - 点击下载按钮（⬇️ 图标）
   - 保存文件到：
     ```bash
     ~/.mcp-servers/google-calendar-credentials.json
     ```

### 第 3 步：OAuth 认证

```bash
bash scripts/setup-google-calendar-auth.sh
```

这将：
- 🌐 自动打开浏览器
- 🔑 引导您完成 Google 登录
- ✅ 保存授权 token

## ✨ 开始使用

### 1. 重启编辑器

完全退出并重启 VS Code Insiders。

### 2. 测试工具

在 Copilot Chat 中输入：

```
创建一个日历事件：
- 标题：收成康普茶
- 时间：2026-03-18 10:00 到 10:30
- 描述：提醒收成康普茶！
```

或使用工具格式：

```
[TOOL_CALL]
{tool => "google_calendar_create", args => {
  --title "收成康普茶"
  --start_time "2026-03-18T10:00:00"
  --end_time "2026-03-18T10:30:00"
  --description "提醒：收成康普茶！"
}}
[/TOOL_CALL]
```

### 3. 其他可用命令

```
# 列出未来 7 天的事件
列出我未来一周的日历安排

# 查找特定事件
查找标题包含"康普茶"的日历事件

# 创建定期事件
每周三上午10点提醒我检查康普茶
```

## 🔧 故障排查

### 问题：工具没有反应

**检查清单**：
```bash
# 1. 检查 MCP 配置文件是否存在
ls -la ~/Library/Application\ Support/Code\ -\ Insiders/User/globalStorage/github.copilot-chat/mcp_settings.json

# 2. 检查凭据文件
ls -la ~/.mcp-servers/google-calendar-credentials.json

# 3. 检查 token 文件
ls -la ~/.mcp-servers/google-calendar-token.json

# 4. 查看 MCP 服务器日志
tail -f ~/Library/Logs/Code\ -\ Insiders/mcp-*.log
```

### 问题：认证失败

**解决方案**：
```bash
# 删除旧 token 重新认证
rm ~/.mcp-servers/google-calendar-token.json
bash scripts/setup-google-calendar-auth.sh
```

### 问题："未找到工具"

**原因**: MCP 服务器未正确加载

**解决方案**：
1. 完全退出 VS Code（不是关闭窗口）
2. 在终端运行：`killall "Code - Insiders"`
3. 重新打开 VS Code
4. 等待 10 秒让 MCP 服务器初始化

### 问题：API 配额限制

Google Calendar API 免费配额：
- 每天 1,000,000 次请求
- 每用户每秒 10 次请求

如果超限，等待配额重置或前往 Google Cloud Console 申请提高配额。

## 📚 完整文档

详细配置说明请参考：
- [docs/google-calendar-mcp-setup.md](google-calendar-mcp-setup.md)

## 🆘 需要帮助？

如果仍有问题：

1. **查看日志**
   ```bash
   # VS Code Insiders 日志
   code-insiders ~/Library/Logs/Code\ -\ Insiders/
   ```

2. **测试 MCP 服务器**
   ```bash
   node ~/.mcp-servers/google-calendar-server/index.js
   ```

3. **验证 Google API 访问**
   ```bash
   curl -H "Authorization: Bearer $(jq -r .access_token ~/.mcp-servers/google-calendar-token.json)" \
     https://www.googleapis.com/calendar/v3/calendars/primary/events
   ```

---

**提示**: 配置一次，永久使用。Token 会自动刷新，无需重复认证。
