#!/bin/bash
# scripts/security/setup-firewall.sh
# 配置防火牆 IP 白名單和端口限制

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_FILE="${SCRIPT_DIR}/logs/firewall-setup.log"
mkdir -p "$(dirname "$LOG_FILE")"

log() {
  echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

error() {
  log "❌ $1"
  exit 1
}

# 檢查是否為 root
if [[ $EUID -ne 0 ]]; then
  error "此腳本必須以 root 權限運行：sudo $0"
fi

log "=== 啟動防火牆配置 ==="

# 1. 檢測 OS 和防火牆系統
log "檢測操作系統..."

OS_TYPE="unknown"
if command -v ufw &> /dev/null; then
  OS_TYPE="ubuntu"
  log "✅ 檢測到 ufw（Ubuntu/Debian）"
elif command -v firewall-cmd &> /dev/null; then
  OS_TYPE="rhel"
  log "✅ 檢測到 firewalld（CentOS/RHEL）"
elif command -v iptables &> /dev/null; then
  OS_TYPE="generic"
  log "✅ 檢測到 iptables（通用 Linux）"
else
  error "找不到支持的防火牆系統 (ufw, firewalld, iptables)"
fi

# 2. 讀取配置文件
CONFIG_FILE="/etc/zeroclaw/firewall.conf"
if [[ ! -f "$CONFIG_FILE" ]]; then
  log "⚠️  配置檔不存在，創建預設值..."
  mkdir -p "$(dirname "$CONFIG_FILE")"
  cat > "$CONFIG_FILE" << 'EOF'
# Zeroclaw 防火牆配置

# API 端口
ZEROCLAW_PORT=42617

# SSH 端口（緊急訪問）
SSH_PORT=22

# 允許的 IP 範圍（CIDR）
# 修改此處添加您的辦公室網路
ALLOWED_CIDR_BLOCKS=(
  "127.0.0.1/32"         # 本機
  "203.0.113.0/24"       # 總部示例
  "198.51.100.0/24"      # 遠端辦公室示例
)

# 允許的反向代理 IP（用於 Docker/Kubernetes）
ALLOWED_PROXIES=(
  "127.0.0.1"            # 本機代理
)

# 啟用 Fail2ban
ENABLE_FAIL2BAN=true

# 啟用 ping（診斷）
ENABLE_PING=false
EOF
  log "✅ 已創建預設配置檔：$CONFIG_FILE"
  log "⚠️  請編輯此檔案添加您的可信 IP 範圍"
fi

source "$CONFIG_FILE"

# 3. ufw 設置（Ubuntu/Debian）
setup_ufw() {
  log "配置 ufw..."
  
  # 重置為預設
  log "重置防火牆規則..."
  ufw --force reset > /dev/null 2>&1 || true
  
  # 設置預設政策
  ufw default deny incoming
  ufw default allow outgoing
  log "✅ 預設政策：Deny Incoming, Allow Outgoing"
  
  # 開放 SSH（緊急訪問）
  log "開放 SSH (${SSH_PORT})..."
  ufw allow "$SSH_PORT/tcp" comment "SSH admin access"
  
  # 開放 Zeroclaw API（僅限白名單）
  log "配置 Zeroclaw API (${ZEROCLAW_PORT})..."
  for CIDR in "${ALLOWED_CIDR_BLOCKS[@]}"; do
    log "  + 允許來自 $CIDR"
    ufw allow from "$CIDR" to any port "$ZEROCLAW_PORT" \
      comment "Zeroclaw from $CIDR"
  done
  
  # 允許 Ping（可選）
  if [[ "$ENABLE_PING" == "true" ]]; then
    ufw allow in icmp comment "Allow ICMP ping"
  fi
  
  # 啟用防火牆
  log "啟用防火牆..."
  ufw enable
  
  # 顯示規則
  log "✅ 防火牆規則："
  ufw status numbered
}

# 4. firewalld 設置（CentOS/RHEL）
setup_firewalld() {
  log "配置 firewalld..."
  
  # 啟動服務
  systemctl enable firewalld
  systemctl start firewalld
  
  # 設置預設 zone
  firewall-cmd --set-default-zone=drop
  log "✅ 預設 zone: drop (拒絕所有入站)"
  
  # 開放 SSH
  log "開放 SSH (${SSH_PORT})..."
  firewall-cmd --permanent --add-port="$SSH_PORT/tcp"
  firewall-cmd --reload
  
  # 開放 Zeroclaw API
  log "配置 Zeroclaw API (${ZEROCLAW_PORT})..."
  for CIDR in "${ALLOWED_CIDR_BLOCKS[@]}"; do
    log "  + 允許來自 $CIDR"
    firewall-cmd --permanent --add-rich-rule=\
"rule family='ipv4' source address='$CIDR' port protocol='tcp' port='$ZEROCLAW_PORT' accept"
  done
  
  firewall-cmd --reload
  log "✅ 規則已應用"
}

# 5. iptables 設置（通用）
setup_iptables() {
  log "配置 iptables..."
  
  # 設置預設政策
  iptables -P INPUT DROP
  iptables -P FORWARD DROP
  iptables -P OUTPUT ACCEPT
  log "✅ 預設政策已設置"
  
  # 允許環回
  iptables -A INPUT -i lo -j ACCEPT
  
  # 允許已建立的連接
  iptables -A INPUT -m state --state ESTABLISHED,RELATED -j ACCEPT
  
  # 允許 SSH
  iptables -A INPUT -p tcp --dport "$SSH_PORT" -j ACCEPT
  log "✅ SSH 開放"
  
  # 允許 Zeroclaw
  for CIDR in "${ALLOWED_CIDR_BLOCKS[@]}"; do
    log "  + 允許來自 $CIDR"
    iptables -A INPUT -p tcp -s "$CIDR" --dport "$ZEROCLAW_PORT" -j ACCEPT
  done
  
  # 保存規則
  if command -v iptables-save &> /dev/null; then
    mkdir -p /etc/iptables
    iptables-save > /etc/iptables/rules.v4
    log "✅ iptables 規則已保存到 /etc/iptables/rules.v4"
  fi
}

# 6. Fail2ban 配置（可選）
setup_fail2ban() {
  if [[ "$ENABLE_FAIL2BAN" != "true" ]]; then
    log "⚠️  Fail2ban 未啟用（ENABLE_FAIL2BAN=false）"
    return
  fi
  
  if ! command -v fail2ban-client &> /dev/null; then
    log "安裝 Fail2ban..."
    if command -v apt-get &> /dev/null; then
      apt-get update && apt-get install -y fail2ban
    elif command -v yum &> /dev/null; then
      yum install -y fail2ban
    else
      log "⚠️  無法自動安裝 Fail2ban，請手動安裝"
      return
    fi
  fi
  
  # 創建本地配置
  mkdir -p /etc/fail2ban/jail.d
  cat > /etc/fail2ban/jail.d/zeroclaw.conf << 'EOF'
[zeroclaw-auth]
enabled = true
port = http,https
filter = zeroclaw-auth
logpath = /var/log/zeroclaw/access.log
maxretry = 5
findtime = 300
bantime = 3600
EOF
  
  # 創建過濾器
  mkdir -p /etc/fail2ban/filter.d
  cat > /etc/fail2ban/filter.d/zeroclaw-auth.conf << 'EOF'
[Definition]
failregex = ^<HOST> .* "(POST|GET|PUT) /api.* (401|403)"
ignoreregex =
EOF
  
  # 重新加載
  systemctl enable fail2ban
  systemctl restart fail2ban
  log "✅ Fail2ban 已配置"
}

# 7. 驗證配置
verify_firewall() {
  log ""
  log "驗證防火牆配置..."
  
  case "$OS_TYPE" in
    ubuntu)
      ufw status verbose
      ;;
    rhel)
      firewall-cmd --list-all
      ;;
    generic)
      iptables -L -n
      ;;
  esac
}

# 8. 測試連接
test_connectivity() {
  log ""
  log "測試連接..."
  
  # 嘗試 SSH
  if nc -zv 127.0.0.1 "$SSH_PORT" 2>&1 | grep -q succeeded; then
    log "✅ SSH 端口 ($SSH_PORT) 可訪問"
  else
    log "⚠️  SSH 端口 ($SSH_PORT) 無法訪問（預期，如果未運行 sshd）"
  fi
  
  # 嘗試 API
  if nc -zv 127.0.0.1 "$ZEROCLAW_PORT" 2>&1 | grep -q succeeded; then
    log "✅ Zeroclaw API 端口 ($ZEROCLAW_PORT) 可訪問"
  else
    log "⚠️  Zeroclaw API 端口 ($ZEROCLAW_PORT) 未監聽（預期，如果 Zeroclaw 未運行）"
  fi
}

# ===== 主程序 =====

case "$OS_TYPE" in
  ubuntu)
    setup_ufw
    ;;
  rhel)
    setup_firewalld
    ;;
  generic)
    setup_iptables
    ;;
esac

setup_fail2ban
verify_firewall
test_connectivity

log ""
log "=== ✅ 防火牆配置完成 ==="
log ""
log "後續步驟："
log "1. 編輯配置檔添加更多可信 IP："
log "   sudo nano $CONFIG_FILE"
log ""
log "2. 重新應用規則："
log "   sudo $0"
log ""
log "3. 監控日誌："
log "   sudo tail -f /var/log/ufw.log"
log ""
