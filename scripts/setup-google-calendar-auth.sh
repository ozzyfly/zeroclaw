#!/bin/bash
# Google Calendar OAuth 认证脚本

set -e

echo "🔐 开始 Google Calendar OAuth 认证..."
echo ""

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

CREDENTIALS_PATH="$HOME/.mcp-servers/google-calendar-credentials.json"
TOKEN_PATH="$HOME/.mcp-servers/google-calendar-token.json"

# 检查凭据文件
if [ ! -f "$CREDENTIALS_PATH" ]; then
    echo -e "${RED}❌ 未找到凭据文件${NC}"
    echo ""
    echo "请先完成以下步骤:"
    echo ""
    echo "1. 访问 Google Cloud Console:"
    echo "   https://console.cloud.google.com/"
    echo ""
    echo "2. 创建新项目或选择现有项目"
    echo ""
    echo "3. 启用 Google Calendar API:"
    echo "   https://console.cloud.google.com/apis/library/calendar-json.googleapis.com"
    echo ""
    echo "4. 创建 OAuth 2.0 凭据:"
    echo "   - 转到 '凭据' 页面"
    echo "   - 点击 '创建凭据' → 'OAuth 客户端 ID'"
    echo "   - 应用类型选择 '桌面应用'"
    echo "   - 下载 JSON 文件"
    echo ""
    echo "5. 将下载的文件保存为:"
    echo "   $CREDENTIALS_PATH"
    echo ""
    exit 1
fi

echo -e "${GREEN}✅ 找到凭据文件${NC}"
echo ""

# 创建临时 Node.js 认证脚本（在 google-calendar-server 目录，使用 .mjs 扩展名）
AUTH_SCRIPT="$HOME/.mcp-servers/google-calendar-server/oauth-helper.mjs"

cat > "$AUTH_SCRIPT" << 'EOF'
import { google } from 'googleapis';
import fs from 'fs/promises';
import http from 'http';
import { URL } from 'url';
import open from 'open';

const SCOPES = ['https://www.googleapis.com/auth/calendar'];
const CREDENTIALS_PATH = process.env.HOME + '/.mcp-servers/google-calendar-credentials.json';
const TOKEN_PATH = process.env.HOME + '/.mcp-servers/google-calendar-token.json';

async function authenticate() {
  try {
    const content = await fs.readFile(CREDENTIALS_PATH, 'utf-8');
    const credentials = JSON.parse(content);
    const { client_secret, client_id, redirect_uris } = credentials.installed || credentials.web;

    const oAuth2Client = new google.auth.OAuth2(
      client_id,
      client_secret,
      redirect_uris[0] || 'http://localhost:3000/oauth2callback'
    );

    // 启动本地服务器接收回调
    const server = http.createServer(async (req, res) => {
      try {
        const url = new URL(req.url, 'http://localhost:3000');
        if (url.pathname === '/oauth2callback') {
          const code = url.searchParams.get('code');
          
          res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
          res.end(`
            <!DOCTYPE html>
            <html>
            <head>
              <meta charset="utf-8">
              <title>认证成功</title>
              <style>
                body { font-family: system-ui; text-align: center; padding: 50px; }
                .success { color: #22c55e; font-size: 24px; }
              </style>
            </head>
            <body>
              <h1 class="success">✅ 认证成功！</h1>
              <p>您可以关闭此窗口并返回终端。</p>
            </body>
            </html>
          `);

          const { tokens } = await oAuth2Client.getToken(code);
          oAuth2Client.setCredentials(tokens);
          await fs.writeFile(TOKEN_PATH, JSON.stringify(tokens, null, 2));
          
          console.log('\n✅ 认证成功！Token 已保存到:', TOKEN_PATH);
          
          server.close();
          process.exit(0);
        }
      } catch (error) {
        console.error('认证错误:', error.message);
        res.writeHead(500);
        res.end('认证失败');
        server.close();
        process.exit(1);
      }
    });

    server.listen(3000, () => {
      const authUrl = oAuth2Client.generateAuthUrl({
        access_type: 'offline',
        scope: SCOPES,
      });

      console.log('🌐 正在打开浏览器进行认证...');
      console.log('');
      console.log('如果浏览器没有自动打开，请手动访问以下链接:');
      console.log(authUrl);
      console.log('');

      open(authUrl);
    });

  } catch (error) {
    console.error('错误:', error.message);
    process.exit(1);
  }
}

authenticate();
EOF

# 检查并安装依赖
cd ~/.mcp-servers/google-calendar-server 2>/dev/null || {
    echo -e "${RED}❌ 请先运行 setup-google-calendar-mcp.sh${NC}"
    exit 1
}

# 确保安装了 open 包
if ! npm list open &> /dev/null; then
    echo "📦 安装额外依赖..."
    npm install open
fi

echo -e "${YELLOW}🔐 启动 OAuth 认证流程...${NC}"
echo ""
echo "浏览器将自动打开，请："
echo "1. 登录您的 Google 账号"
echo "2. 授权访问日历权限"
echo "3. 完成后返回终端"
echo ""

# 切换到 google-calendar-server 目录（那里有 node_modules）
cd ~/.mcp-servers/google-calendar-server || {
    echo -e "${RED}❌ 请先运行 setup-google-calendar-mcp.sh${NC}"
    exit 1
}

# 运行认证脚本
node "$AUTH_SCRIPT"

# 清理临时脚本
rm -f "$AUTH_SCRIPT"

echo ""
echo -e "${GREEN}✨ OAuth 认证完成！${NC}"
echo ""
echo "下一步："
echo "1. 重启您的编辑器（VS Code Insiders 或 Claude Desktop）"
echo "2. 在 Copilot Chat 中测试："
echo '   "创建日历事件：收成康普茶，2026-03-18 10:00-10:30"'
echo ""
