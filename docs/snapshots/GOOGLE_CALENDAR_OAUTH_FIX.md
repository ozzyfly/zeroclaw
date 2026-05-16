# 🔧 Google Calendar OAuth 修復指南

## 問題
你的 Google Calendar API 認證失敗，原因是：
- **OAuth 客戶端已被刪除**: `401: deleted_client` 錯誤
- 需要在 Google Cloud Console 重新建立 OAuth 2.0 客戶端 ID

## 修復步驟（5 分鐘）

### 第 1 步：刪除舊憑證
```bash
rm -f ~/.mcp-servers/google-calendar-credentials.json
rm -f ~/.mcp-servers/google-calendar-token.json
```

### 第 2 步：在 Google Cloud Console 建立新 OAuth 客戶端

#### 2.1 訪問 Google Cloud Console
- 前往：https://console.cloud.google.com/
- 如果還沒登入，請用你的 Google 帳號登入

#### 2.2 建立新專案（或選擇現有）
1. 點擊頂部的**"專案選擇器"**（顯示當前專案名稱的地方）
2. 點擊**"新建專案"**
3. 輸入名稱：`ZeroClaw Calendar Fix`
4. 點擊**"建立"**，等待 1-2 分鐘

#### 2.3 啟用 Google Calendar API
1. 導航到：https://console.cloud.google.com/apis/library/calendar-json.googleapis.com
2. 點擊**"啟用"**按鈕

#### 2.4 建立 OAuth 2.0 客戶端 ID
1. 轉到"APIs & Services" → **"認證"**
2. 點擊**"建立認證"** → **"OAuth 客戶端 ID"**
3. 如果提示**"設定 OAuth 同意屏幕"**，點擊"設定"：
   - 選擇**"外部"** 使用者類型
   - 填寫必要信息（應用名稱、使用者支持郵箱、開發者聯絡資訊）
   - 點擊**"儲存並繼續"** → 跳過範圍 → **"儲存並繼續"** → **"返回儀表板"**
4. 再次點擊**"建立認證"** → **"OAuth 客戶端 ID"**
5. 選擇應用類型：**"桌面應用"**
6. 名稱：`ZeroClaw Calendar`
7. 點擊**"建立"**

#### 2.5 下載 JSON 憑證
- 點擊下載按鈕（⬇️ 圖標）
- 將文件保存為（根據下載位置調整）：
```bash
mv ~/Downloads/client_secret_*.configs.googleusercontent.com.json \
   ~/.mcp-servers/google-calendar-credentials.json
```

或手動複製文件內容後儲存。

### 第 3 步：運行認證腳本
```bash
bash zeroclaw/scripts/setup-google-calendar-auth.sh
```

這將：
- 🌐 在瀏覽器中打開 Google 登入頁面
- 🔑 引導你授權 ZeroClaw 訪問日曆
- ✅ 自動保存新的 OAuth token

### 第 4 步：驗證配置
```bash
# 確認文件存在
ls -la ~/.mcp-servers/google-calendar-*.json

# 檢查 token 是否有效（應顯示未來日期）
node --input-type=module -e "import fs from 'fs';const t=JSON.parse(fs.readFileSync(process.env.HOME+'/.mcp-servers/google-calendar-token.json','utf8'));const d=new Date(t.expiry_date);console.log('Token 過期時間:', d.toISOString());"
```

### 第 5 步：重啟 VS Code 並測試
1. 完全退出 VS Code（或 `⌘Q` 退出 VS Code Insiders）
2. 重新打開 VS Code
3. 在 GitHub Copilot Chat 中輸入：
   ```
   創建一個日曆事件：
   - 標題：測試事件
   - 時間：2026-03-18 10:00 到 10:30
   - 描述：這是 OAuth 修復後的第一次測試
   ```

## 常見問題

### Q: 仍然得到 `401: deleted_client` 錯誤
**A**: 確認 `~/.mcp-servers/google-calendar-credentials.json` 中的 `client_id`：
```bash
node --input-type=module -e "import fs from 'fs';const c=JSON.parse(fs.readFileSync(process.env.HOME+'/.mcp-servers/google-calendar-credentials.json'));console.log('Client ID:', (c.installed||c.web).client_id);"
```
在 Google Cloud 中檢查該 client_id 是否確實存在且未刪除。

### Q: "需要 OAuth 認證"錯誤
**A**: Token 可能已過期，重新運行：
```bash
bash zeroclaw/scripts/setup-google-calendar-auth.sh
```

### Q: 瀏覽器沒有自動打開
**A**: 手動複製終端輸出的 URL 到瀏覽器，完成認證後會顯示成功頁面。

### Q: Token 檔何時需要更新
**A**: 通常 Google 的 OAuth token 有效期為 1 小時。zeroct aw 會自動用 refresh token 更新。如果失敗，重新運行 `setup-google-calendar-auth.sh`。

## 快速修復腳本（自動化）

如果上述步驟有問題，保存此腳本並執行：

```bash
#!/bin/bash
set -e

echo "🔧 Google Calendar OAuth 修復..."
echo ""

CREDENTIALS_FILE="$HOME/.mcp-servers/google-calendar-credentials.json"

if [ ! -f "$CREDENTIALS_FILE" ]; then
    echo "❌ 找不到 $CREDENTIALS_FILE"
    echo ""
    echo "請先按照上述步驟 1-2 建立新 OAuth 客戶端並下載 JSON 文件"
    echo "然後保存為: $CREDENTIALS_FILE"
    exit 1
fi

echo "✅ 找到憑證文件"
echo ""

# 驗證客戶端 ID
CLIENT_ID=$(node --input-type=module -e "import fs from 'fs';const c=JSON.parse(fs.readFileSync('$CREDENTIALS_FILE'));process.stdout.write((c.installed||c.web).client_id);" 2>/dev/null || echo "INVALID")

if [ "$CLIENT_ID" = "INVALID" ]; then
    echo "❌ 憑證文件格式無效"
    exit 1
fi

echo "✅ OAuth Client ID: ${CLIENT_ID:0:30}..."
echo ""

# 運行認證
echo "🔐 啟動 OAuth 認證流程..."
cd "$HOME/.mcp-servers/google-calendar-server"
npm install --silent 2>/dev/null || true

bash zeroclaw/scripts/setup-google-calendar-auth.sh

echo ""
echo "✅ 修復完成！請重啟 VS Code 並再試一次。"
```

## 測試 API
```bash
node --input-type=module << 'EOF'
import { google } from 'googleapis';
import fs from 'fs/promises';

const creds = JSON.parse(await fs.readFile(process.env.HOME + '/.mcp-servers/google-calendar-credentials.json', 'utf8'));
const token = JSON.parse(await fs.readFile(process.env.HOME + '/.mcp-servers/google-calendar-token.json', 'utf8'));
const { client_id, client_secret, redirect_uris } = creds.installed || creds.web;

const auth = new google.auth.OAuth2(client_id, client_secret, redirect_uris?.[0]);
auth.setCredentials(token);

const calendar = google.calendar({ version: 'v3', auth });
const result = await calendar.events.list({
  calendarId: 'primary',
  maxResults: 5,
  singleEvents: true,
  orderBy: 'startTime',
  timeMin: new Date().toISOString(),
});

console.log('接下來 5 個事件:');
(result.data.items || []).forEach(e => {
  const start = e.start.dateTime || e.start.date;
  console.log(`  - ${e.summary} (${start})`);
});
EOF
```

---

**有問題？** 提供以下資訊以便診斷：
1. `~/.mcp-servers/google-calendar-credentials.json` 檔案大小和修改時間
2. 執行 `setup-google-calendar-auth.sh` 時的完整錯誤訊息
3. VS Code 或 Claude Desktop MCP 日誌（如適用）
