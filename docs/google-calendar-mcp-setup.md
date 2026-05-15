# Google Calendar MCP Server 配置指南

本指南将帮您在 ZeroClaw 中配置 Google Calendar MCP Server，以便使用日历功能。

## 前置要求

1. **Node.js** (v18 或更高版本)
2. **Google Cloud Console 账号**
3. **GitHub Copilot** 或支持 MCP 的编辑器

## 第一步：安装 Google Calendar MCP Server

### 1.1 通过 npx 安装（推荐）

```bash
# 确保 Node.js 已安装
node --version  # 应显示 v18+ 

# 创建 MCP 服务器目录
mkdir -p ~/.mcp-servers
cd ~/.mcp-servers

# 安装 Google Calendar MCP Server（如果可用）
npm install -g @modelcontextprotocol/server-google-calendar
# 或使用社区版本
npm install -g mcp-google-calendar
```

### 1.2 配置 Google Cloud 项目

1. **访问 Google Cloud Console**:
   - 前往 https://console.cloud.google.com/
   - 创建新项目或选择现有项目

2. **启用 Google Calendar API**:
   ```
   https://console.cloud.google.com/apis/library/calendar-json.googleapis.com
   ```
   点击"启用"

3. **创建 OAuth 2.0 凭据**:
   - 导航到"凭据" → "创建凭据" → "OAuth 客户端 ID"
   - 应用类型选择"桌面应用"
   - 下载 JSON 文件，保存为 `~/.mcp-servers/google-calendar-credentials.json`

## 第二步：配置 MCP 客户端

### 2.1 VS Code Insiders 配置

创建或编辑 MCP 配置文件：

**位置**: `~/Library/Application Support/Code - Insiders/User/globalStorage/github.copilot-chat/mcp_settings.json`

```json
{
  "mcpServers": {
    "google-calendar": {
      "command": "npx",
      "args": [
        "-y",
        "@modelcontextprotocol/server-google-calendar"
      ],
      "env": {
        "GOOGLE_CALENDAR_CREDENTIALS": "/Users/user/.mcp-servers/google-calendar-credentials.json",
        "GOOGLE_CALENDAR_TOKEN": "/Users/user/.mcp-servers/google-calendar-token.json"
      }
    }
  }
}
```

### 2.2 Claude Desktop 配置（备选）

**位置**: `~/Library/Application Support/Claude/claude_desktop_config.json`

```json
{
  "mcpServers": {
    "google-calendar": {
      "command": "node",
      "args": [
        "/Users/user/.mcp-servers/node_modules/@modelcontextprotocol/server-google-calendar/dist/index.js"
      ],
      "env": {
        "GOOGLE_CALENDAR_CREDENTIALS": "/Users/user/.mcp-servers/google-calendar-credentials.json",
        "GOOGLE_CALENDAR_TOKEN": "/Users/user/.mcp-servers/google-calendar-token.json"
      }
    }
  }
}
```

## 第三步：OAuth 认证

首次使用时需要进行 OAuth 认证：

```bash
# 运行 MCP 服务器进行认证
npx @modelcontextprotocol/server-google-calendar

# 或使用 Node.js 直接运行
node ~/.mcp-servers/node_modules/@modelcontextprotocol/server-google-calendar/dist/index.js
```

这将：
1. 自动打开浏览器
2. 要求您登录 Google 账号
3. 授权日历访问权限
4. 生成 token 文件

## 第四步：验证配置

### 4.1 重启编辑器

完全退出并重启 VS Code Insiders 或 Claude Desktop。

### 4.2 测试工具调用

在 Copilot Chat 中测试：

```
创建一个日历事件：
- 标题：测试事件
- 时间：明天上午10点到11点
- 描述：这是一个测试事件
```

### 4.3 可用的工具

配置成功后，您可以使用：

- `google_calendar_create` - 创建事件
- `google_calendar_list` - 列出事件
- `google_calendar_update` - 更新事件
- `google_calendar_delete` - 删除事件
- `google_calendar_search` - 搜索事件

## 第五步：ZeroClaw 集成（可选）

如果您想在 ZeroClaw 中集成 Google Calendar，可以创建自定义工具：

```toml
# 在 config.toml 中添加
[tools.google_calendar]
enabled = true
mcp_server = "google-calendar"
```

## 故障排查

### 问题 1: "找不到 MCP 服务器"

**解决方案**:
```bash
# 检查 Node.js 版本
node --version

# 重新安装 MCP 服务器
npm install -g @modelcontextprotocol/server-google-calendar --force
```

### 问题 2: "认证失败"

**解决方案**:
```bash
# 删除旧的 token
rm ~/.mcp-servers/google-calendar-token.json

# 重新认证
npx @modelcontextprotocol/server-google-calendar
```

### 问题 3: "API 配额超限"

**解决方案**:
- 访问 Google Cloud Console
- 检查 API 配额和限制
- 考虑申请提高配额

## 替代方案：使用 macOS 原生日历

如果 Google Calendar MCP Server 不可用，可以直接使用 macOS 日历：

```bash
# 创建日历事件
osascript -e 'tell application "Calendar"
    tell calendar "Calendar"
        make new event with properties {
            summary:"收成康普茶", 
            start date:date "2026-03-18 10:00:00", 
            end date:date "2026-03-18 10:30:00", 
            description:"提醒：收成康普茶！"
        }
    end tell
end tell'
```

## 参考资源

- [MCP 官方文档](https://modelcontextprotocol.io/)
- [Google Calendar API 文档](https://developers.google.com/calendar/api)
- [VS Code MCP 支持](https://code.visualstudio.com/docs)

## 下一步

配置完成后，您可以：

1. 在 Copilot Chat 中自然语言创建日历事件
2. 查询未来的日程安排
3. 批量管理日历事件
4. 设置定期提醒

---

**注意**: 如果您使用的是企业 Google Workspace 账号，可能需要管理员授权某些权限。
