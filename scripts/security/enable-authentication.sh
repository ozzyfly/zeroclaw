#!/bin/bash
# scripts/security/enable-authentication.sh
# 啟用 Zeroclaw Bearer Token 認證

set -euo pipefail

WRITE_RC=0
for arg in "$@"; do
  case "$arg" in
    --write-rc) WRITE_RC=1 ;;
    -h|--help)
      cat <<'USAGE'
Usage: enable-authentication.sh [--write-rc]

Pairs with the local Zeroclaw daemon and stores the bearer token at
~/.zeroclaw/.auth-token.secret (chmod 600).

Options:
  --write-rc   Also append `export ZEROCLAW_API_TOKEN=...` to ~/.bashrc and
               ~/.zshrc. By default the script only prints the export line
               so the operator can add it manually.
USAGE
      exit 0 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_FILE="${SCRIPT_DIR}/logs/auth-setup.log"
mkdir -p "$(dirname "$LOG_FILE")"

log() {
  echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

log "=== 啟動 Bearer Token 認證設置 ==="

# 1. 驗證 Zeroclaw 配置
log "檢查 Zeroclaw 配置..."

CONFIG_FILE="${ZEROCLAW_CONFIG:-$HOME/.zeroclaw/config.toml}"
if [[ ! -f "$CONFIG_FILE" ]]; then
  log "❌ 錯誤：找不到配置檔 $CONFIG_FILE"
  exit 1
fi

# 檢查是否啟用了配對守衛
# 安全做法：不自動改寫 TOML（避免重複 section header / 結構破壞）。
# 由操作者手動確認/添加。
if ! grep -qE '^[[:space:]]*require_pairing[[:space:]]*=' "$CONFIG_FILE"; then
  log "⚠️  配置中未啟用 require_pairing。請手動在 $CONFIG_FILE 的 [pairing] section 加入："
  log "    [pairing]"
  log "    require_pairing = true"
  log "    （若 [pairing] section 已存在，只需新增 require_pairing = true 一行）"
fi

# 2. 生成首個配對令牌
log "生成首個配對令牌..."

ZEROCLAW_PORT=$(grep "^port = " "$CONFIG_FILE" | head -1 | awk '{print $3}' || echo "42617")
ZEROCLAW_HOST=$(grep "^host = " "$CONFIG_FILE" | head -1 | awk '{print $3}' | tr -d '"' || echo "127.0.0.1")
TOKEN_FILE="${HOME}/.zeroclaw/.auth-token.secret"

if command -v curl &> /dev/null; then
  log "嘗試連接 http://$ZEROCLAW_HOST:$ZEROCLAW_PORT/pair"

  PAIR_RESPONSE=$(curl -s -X POST "http://$ZEROCLAW_HOST:$ZEROCLAW_PORT/pair" 2>&1 || echo "{}")

  if echo "$PAIR_RESPONSE" | grep -q "token"; then
    AUTH_TOKEN=$(echo "$PAIR_RESPONSE" | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
    log "✅ 已生成認證令牌"

    # 保存到安全位置
    mkdir -p "$(dirname "$TOKEN_FILE")"
    umask 077
    printf '%s\n' "$AUTH_TOKEN" > "$TOKEN_FILE"
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

  STATUS=$(curl -s -H "Authorization: Bearer $AUTH_TOKEN" \
    "http://$ZEROCLAW_HOST:$ZEROCLAW_PORT/api/status" \
    -w "\n%{http_code}" | tail -1)

  if [[ "$STATUS" == "200" ]]; then
    log "✅ 認證驗證成功 (HTTP 200)"
  else
    log "❌ 認證失敗 (HTTP $STATUS)"
  fi
fi

# 4. 環境變數設置
ENV_LINE="export ZEROCLAW_API_TOKEN=\"\$(cat $TOKEN_FILE 2>/dev/null)\""

if [[ "$WRITE_RC" -eq 1 ]]; then
  log "寫入 shell rc 文件..."
  for rc_file in "$HOME/.bashrc" "$HOME/.zshrc"; do
    if [[ -f "$rc_file" ]]; then
      if grep -q 'ZEROCLAW_API_TOKEN' "$rc_file"; then
        log "✅ $rc_file 已包含 ZEROCLAW_API_TOKEN，跳過"
      else
        printf '\n# Zeroclaw bearer token\n%s\n' "$ENV_LINE" >> "$rc_file"
        log "✅ 已附加到 $rc_file"
      fi
    fi
  done
else
  log ""
  log "如需將令牌匯出為環境變數，將以下行加入 ~/.bashrc 或 ~/.zshrc："
  log "  $ENV_LINE"
  log "或重新執行此腳本帶 --write-rc 旗標。"
fi

log ""
log "=== ✅ 認證設置完成 ==="
log ""
log "後續步驟："
log "1. 確認 [pairing] / require_pairing = true 已在 $CONFIG_FILE"
log "2. 驗證令牌："
log "   curl -H \"Authorization: Bearer \$(cat $TOKEN_FILE)\" \\"
log "     http://$ZEROCLAW_HOST:$ZEROCLAW_PORT/api/status"
log ""
