#!/bin/bash
# Google Calendar MCP Server 一键配置脚本
# 用法: bash setup-google-calendar-mcp.sh

set -e

echo "🗓️  开始配置 Google Calendar MCP Server..."
echo ""

# 颜色定义
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# 检查 Node.js
echo "📦 检查 Node.js..."
if ! command -v node &> /dev/null; then
    echo -e "${RED}❌ 未找到 Node.js${NC}"
    echo "请先安装 Node.js: https://nodejs.org/"
    exit 1
fi

NODE_VERSION=$(node --version | cut -d'v' -f2 | cut -d'.' -f1)
if [ "$NODE_VERSION" -lt 18 ]; then
    echo -e "${RED}❌ Node.js 版本过低 (需要 v18+)${NC}"
    echo "当前版本: $(node --version)"
    exit 1
fi

echo -e "${GREEN}✅ Node.js 版本: $(node --version)${NC}"
echo ""

# 创建目录
echo "📁 创建 MCP 服务器目录..."
mkdir -p ~/.mcp-servers
cd ~/.mcp-servers

# 安装 MCP Server
echo "📥 安装 Google Calendar MCP Server..."
echo -e "${YELLOW}注意: 如果官方包不存在，我们将创建一个本地实现${NC}"

# 尝试安装官方包（可能不存在）
if npm install -g @modelcontextprotocol/server-google-calendar 2>/dev/null; then
    echo -e "${GREEN}✅ 已安装官方 Google Calendar MCP Server${NC}"
    MCP_COMMAND="npx @modelcontextprotocol/server-google-calendar"
else
    echo -e "${YELLOW}⚠️  官方包不可用，将创建本地实现...${NC}"
    
    # 创建本地 MCP 服务器实现
    mkdir -p ~/.mcp-servers/google-calendar-server
    cat > ~/.mcp-servers/google-calendar-server/package.json << 'EOF'
{
  "name": "google-calendar-mcp-server",
  "version": "1.0.0",
  "description": "Google Calendar MCP Server for ZeroClaw",
  "main": "index.js",
  "type": "module",
  "dependencies": {
    "googleapis": "^129.0.0",
    "@modelcontextprotocol/sdk": "^0.5.0"
  }
}
EOF

    cat > ~/.mcp-servers/google-calendar-server/index.js << 'EOF'
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { google } from 'googleapis';
import fs from 'fs/promises';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

class GoogleCalendarServer {
  constructor() {
    this.server = new Server(
      {
        name: 'google-calendar-server',
        version: '1.0.0',
      },
      {
        capabilities: {
          tools: {},
        },
      }
    );

    this.setupToolHandlers();
    this.server.onerror = (error) => console.error('[MCP Error]', error);
    process.on('SIGINT', async () => {
      await this.server.close();
      process.exit(0);
    });
  }

  setupToolHandlers() {
    this.server.setRequestHandler('tools/list', async () => ({
      tools: [
        {
          name: 'google_calendar_create',
          description: '创建 Google Calendar 事件',
          inputSchema: {
            type: 'object',
            properties: {
              title: { type: 'string', description: '事件标题' },
              start_time: { type: 'string', description: 'ISO 8601 格式开始时间' },
              end_time: { type: 'string', description: 'ISO 8601 格式结束时间' },
              description: { type: 'string', description: '事件描述' },
              location: { type: 'string', description: '地点（可选）' },
            },
            required: ['title', 'start_time', 'end_time'],
          },
        },
        {
          name: 'google_calendar_list',
          description: '列出未来的日历事件',
          inputSchema: {
            type: 'object',
            properties: {
              max_results: { type: 'number', description: '最大结果数（默认10）' },
              days_ahead: { type: 'number', description: '未来多少天（默认7）' },
            },
          },
        },
      ],
    }));

    this.server.setRequestHandler('tools/call', async (request) => {
      const { name, arguments: args } = request.params;

      try {
        const auth = await this.getAuth();
        const calendar = google.calendar({ version: 'v3', auth });

        switch (name) {
          case 'google_calendar_create':
            return await this.createEvent(calendar, args);
          case 'google_calendar_list':
            return await this.listEvents(calendar, args);
          default:
            throw new Error(`Unknown tool: ${name}`);
        }
      } catch (error) {
        return {
          content: [
            {
              type: 'text',
              text: `Error: ${error.message}`,
            },
          ],
        };
      }
    });
  }

  async getAuth() {
    const credentialsPath = process.env.GOOGLE_CALENDAR_CREDENTIALS || 
                           path.join(process.env.HOME, '.mcp-servers', 'google-calendar-credentials.json');
    const tokenPath = process.env.GOOGLE_CALENDAR_TOKEN || 
                     path.join(process.env.HOME, '.mcp-servers', 'google-calendar-token.json');

    const credentials = JSON.parse(await fs.readFile(credentialsPath, 'utf-8'));
    const { client_secret, client_id, redirect_uris } = credentials.installed || credentials.web;

    const oAuth2Client = new google.auth.OAuth2(client_id, client_secret, redirect_uris[0]);

    try {
      const token = JSON.parse(await fs.readFile(tokenPath, 'utf-8'));
      oAuth2Client.setCredentials(token);
    } catch (error) {
      throw new Error('需要 OAuth 认证。请运行 setup-google-calendar-auth.sh');
    }

    return oAuth2Client;
  }

  async createEvent(calendar, args) {
    const event = {
      summary: args.title,
      description: args.description || '',
      location: args.location || '',
      start: {
        dateTime: args.start_time,
        timeZone: 'Asia/Shanghai',
      },
      end: {
        dateTime: args.end_time,
        timeZone: 'Asia/Shanghai',
      },
    };

    const response = await calendar.events.insert({
      calendarId: 'primary',
      resource: event,
    });

    return {
      content: [
        {
          type: 'text',
          text: `✅ 已创建日历事件: ${args.title}\n链接: ${response.data.htmlLink}`,
        },
      ],
    };
  }

  async listEvents(calendar, args) {
    const maxResults = args.max_results || 10;
    const daysAhead = args.days_ahead || 7;
    const timeMin = new Date().toISOString();
    const timeMax = new Date(Date.now() + daysAhead * 24 * 60 * 60 * 1000).toISOString();

    const response = await calendar.events.list({
      calendarId: 'primary',
      timeMin,
      timeMax,
      maxResults,
      singleEvents: true,
      orderBy: 'startTime',
    });

    const events = response.data.items || [];
    if (events.length === 0) {
      return {
        content: [{ type: 'text', text: '未找到未来的事件' }],
      };
    }

    const eventList = events
      .map((event) => {
        const start = event.start.dateTime || event.start.date;
        return `- ${event.summary} (${start})`;
      })
      .join('\n');

    return {
      content: [{ type: 'text', text: `📅 未来事件:\n${eventList}` }],
    };
  }

  async run() {
    const transport = new StdioServerTransport();
    await this.server.connect(transport);
    console.error('Google Calendar MCP server running on stdio');
  }
}

const server = new GoogleCalendarServer();
server.run().catch(console.error);
EOF

    cd ~/.mcp-servers/google-calendar-server
    npm install
    cd ~/.mcp-servers
    
    MCP_COMMAND="node ~/.mcp-servers/google-calendar-server/index.js"
    echo -e "${GREEN}✅ 已创建本地 MCP 服务器${NC}"
fi

echo ""

# 检测编辑器配置位置
echo "🔍 检测编辑器配置..."

# VS Code Insiders
VSCODE_INSIDERS_CONFIG="$HOME/Library/Application Support/Code - Insiders/User/globalStorage/github.copilot-chat"
# Claude Desktop
CLAUDE_CONFIG_DIR="$HOME/Library/Application Support/Claude"

if [ -d "$VSCODE_INSIDERS_CONFIG" ]; then
    echo -e "${GREEN}✅ 找到 VS Code Insiders${NC}"
    
    mkdir -p "$VSCODE_INSIDERS_CONFIG"
    MCP_CONFIG="$VSCODE_INSIDERS_CONFIG/mcp_settings.json"
    
    cat > "$MCP_CONFIG" << EOF
{
  "mcpServers": {
    "google-calendar": {
      "command": "node",
      "args": [
        "$HOME/.mcp-servers/google-calendar-server/index.js"
      ],
      "env": {
        "GOOGLE_CALENDAR_CREDENTIALS": "$HOME/.mcp-servers/google-calendar-credentials.json",
        "GOOGLE_CALENDAR_TOKEN": "$HOME/.mcp-servers/google-calendar-token.json"
      }
    }
  }
}
EOF
    
    echo -e "${GREEN}✅ 已创建 VS Code Insiders MCP 配置${NC}"
    EDITOR_CONFIGURED="vscode"
fi

if [ -d "$CLAUDE_CONFIG_DIR" ]; then
    echo -e "${GREEN}✅ 找到 Claude Desktop${NC}"
    
    CLAUDE_CONFIG="$CLAUDE_CONFIG_DIR/claude_desktop_config.json"
    
    cat > "$CLAUDE_CONFIG" << EOF
{
  "mcpServers": {
    "google-calendar": {
      "command": "node",
      "args": [
        "$HOME/.mcp-servers/google-calendar-server/index.js"
      ],
      "env": {
        "GOOGLE_CALENDAR_CREDENTIALS": "$HOME/.mcp-servers/google-calendar-credentials.json",
        "GOOGLE_CALENDAR_TOKEN": "$HOME/.mcp-servers/google-calendar-token.json"
      }
    }
  }
}
EOF
    
    echo -e "${GREEN}✅ 已创建 Claude Desktop MCP 配置${NC}"
    EDITOR_CONFIGURED="claude"
fi

if [ -z "$EDITOR_CONFIGURED" ]; then
    echo -e "${YELLOW}⚠️  未找到支持的编辑器配置目录${NC}"
    echo "请手动配置 MCP 设置"
fi

echo ""
echo -e "${YELLOW}📝 下一步操作:${NC}"
echo ""
echo "1️⃣  获取 Google Calendar API 凭据:"
echo "   - 访问: https://console.cloud.google.com/"
echo "   - 创建项目并启用 Calendar API"
echo "   - 创建 OAuth 2.0 客户端 ID（桌面应用）"
echo "   - 下载 JSON 文件保存为:"
echo "     ~/.mcp-servers/google-calendar-credentials.json"
echo ""
echo "2️⃣  运行 OAuth 认证:"
echo "   bash setup-google-calendar-auth.sh"
echo ""
echo "3️⃣  重启编辑器"
echo ""
echo "4️⃣  测试工具调用:"
echo '   "创建一个日历事件：收成康普茶，2026-03-18 10:00-10:30"'
echo ""
echo -e "${GREEN}✨ 配置脚本完成！${NC}"
