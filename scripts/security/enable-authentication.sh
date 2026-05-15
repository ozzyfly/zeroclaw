#!/bin/bash
# scripts/security/enable-authentication.sh
# 啟用 Zeroclaw Bearer Token 認證

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_FILE="${SCRIPT_DIR}/logs/auth-setup.log"
mkdir -p "$(dirname "$LOG_FILE")"

log() {
  echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

log "=== 啟動 Bearer Token 認證設置 ==="

# 1. 驗證 Zeroclaw 配置
log "檢查 Zeroclaw 配置..."

CONFIG_FILE="${ZEROCLAW_CONFIG:-~/.zeroclaw/config.toml}"
if [[ ! -f "$CONFIG_FILE" ]]; then
  log "❌ 錯誤：找不到配置檔 $CONFIG_FILE"
  exit 1
fi

# 檢查是否啟用了配對守衛
if ! grep -q "require_pairing" "$CONFIG_FILE"; then
  log "⚠️  警告：配置中未找到 require_pairing，添加預設值..."
  cat >> "$CONFIG_FILE" << 'EOF'

[pairing]
# ✅ 強制配對認證
require_pairing = true
EOF
  log "✅ 已添加 require_pairing = true"
fi

# 2. 生成首個配對令牌
log "生成首個配對令牌..."

if command -v curl &> /dev/null; then
  ZEROCLAW_PORT=$(grep "^port = " "$CONFIG_FILE" | head -1 | awk '{print $3}' || echo "42617")
  ZEROCLAW_HOST=$(grep "^host = " "$CONFIG_FILE" | head -1 | awk '{print $3}' | tr -d '"' || echo "127.0.0.1")
  
  log "嘗試連接 http://$ZEROCLAW_HOST:$ZEROCLAW_PORT/pair"
  
  PAIR_RESPONSE=$(curl -s -X POST "http://$ZEROCLAW_HOST:$ZEROCLAW_PORT/pair" 2>&1 || echo "{}")
  
  if echo "$PAIR_RESPONSE" | grep -q "token"; then
    AUTH_TOKEN=$(echo "$PAIR_RESPONSE" | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
    log "✅ 已生成認證令牌"
    
    # 保存到安全位置
    TOKEN_FILE="${HOME}/.zeroclaw/.auth-token.secret"
    mkdir -p "$(dirname "$TOKEN_FILE")"
    echo "$AUTH_TOKEN" > "$TOKEN_FILE"
    chmod 600 "$TOKEN_FILE"
    log "✅ 令牌已保存到 $TOKEN_FILE （權限 600）"
  else
    log "⚠️  無法自動生成令牌，請手動執行："
    log "  curl -X POST http://$ZEROCLAW_HOST:$ZEROCLAW_PORT/pair"
  fi
else
  log "⚠️  curl 未安裝，跳過自動令牌生成"
fi

# 3. 驗證認證
log "驗證 Bearer Token 認證..."

if [[ -f "$TOKEN_FILE" ]]; then
  AUTH_TOKEN=$(cat "$TOKEN_FILE")
  
  # 測試認證端點
  STATUS=$(curl -s -H "Authorization: Bearer $AUTH_TOKEN" \
    "http://$ZEROCLAW_HOST:$ZEROCLAW_PORT/api/status" \
    -w "\n%{http_code}" | tail -1)
  
  if [[ "$STATUS" == "200" ]]; then
    log "✅ 認證驗證成功 (HTTP 200)"
  else
    log "❌ 認證失敗 (HTTP $STATUS)"
  fi
fi

# 4. 設置環境變數
log "設置環境變數..."

ENV_LINE="export ZEROCLAW_API_TOKEN=$(cat "$TOKEN_FILE" 2>/dev/null || echo '<YOUR_TOKEN>')"

if grep -q "ZEROCLAW_API_TOKEN" ~/.bashrc ~/.zshrc 2>/dev/null; then
  log "✅ ZEROCLAW_API_TOKEN 已存在於 shell 配置"
else
  for rc_file in ~/.bashrc ~/.zshrc; do
    if [[ -f "$rc_file" ]]; then
      echo "$ENV_LINE" >> "$rc_file"
      log "✅ 已添加到 $rc_file"
    fi
  done
fi

log ""
log "=== ✅ 認證設置完成 ==="
log ""
log "後續步驟："
log "1. 重新加載 shell 配置："
log "   source ~/.bashrc  # 或 source ~/.zshrc"
log ""
log "2. 驗證令牌："
log "   curl -H \"Authorization: Bearer \$ZEROCLAW_API_TOKEN\" \\"
log "     http://localhost:42617/api/status"
log ""
