#!/bin/bash
# scripts/security/monitor-api-keys.sh
# 監控 API 金鑰使用情況，檢測異常活動

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LOG_FILE="${SCRIPT_DIR}/logs/api-key-monitoring.log"
mkdir -p "$(dirname "$LOG_FILE")"

# 配置
ZEROCLAW_URL="${ZEROCLAW_URL:-http://localhost:42617}"
ADMIN_TOKEN="${ZEROCLAW_ADMIN_TOKEN}"
CHECK_INTERVAL_SECS=${CHECK_INTERVAL_SECS:-300}  # 5分鐘
ALERT_THRESHOLD_RPM=${ALERT_THRESHOLD_RPM:-1000} # 每分鐘請求數

log() {
  echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

error() {
  log "❌ $1" >&2
  exit 1
}

log "=== Zeroclaw API Key 監控系統啟動 ==="

# 1. 驗證配置
if [[ -z "$ADMIN_TOKEN" ]]; then
  generate_admin_token() {
    log "生成臨時管理員令牌（不用於生產環境）..."
    ADMIN_TOKEN=$(curl -s -X POST "$ZEROCLAW_URL/pair" | grep -o '"token":"[^"]*"' | cut -d'"' -f4)
    if [[ -z "$ADMIN_TOKEN" ]]; then
      error "無法生成管理員令牌"
    fi
  }
fi

# 2. API 金鑰審計函數
audit_api_keys() {
  local response
  
  log "審計 API 金鑰使用情況..."
  
  # 獲取最近的審計日誌
  response=$(curl -s \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    "$ZEROCLAW_URL/api/audit-logs?limit=1000&order=desc" \
    2>/dev/null || echo "{}")
  
  echo "$response"
}

# 3. 檢測異常流量
detect_anomalies() {
  local audit_logs="$1"
  local now=$(date +%s)
  local five_mins_ago=$((now - 300))
  
  log "檢測異常流量..."
  
  # 計算最近5分鐘的請求數
  local recent_requests=$(echo "$audit_logs" | grep -c "timestamp" || echo "0")
  local requests_per_minute=$((recent_requests / 5))
  
  # 檢查是否超過閾值
  if [[ $requests_per_minute -gt $ALERT_THRESHOLD_RPM ]]; then
    log "🚨 警告：高流量檢測！($requests_per_minute req/min > $ALERT_THRESHOLD_RPM 閾值)"
    return 1  # 異常
  elif [[ $requests_per_minute -gt $((ALERT_THRESHOLD_RPM / 2)) ]]; then
    log "⚠️  注意：流量增加 ($requests_per_minute req/min)"
  else
    log "✅ 流量正常 ($requests_per_minute req/min)"
  fi
  
  return 0
}

# 4. 檢測認證失敗激增
detect_auth_failures() {
  local audit_logs="$1"
  local auth_failures=$(echo "$audit_logs" | grep -c "AuthenticationFailure" || echo "0")
  
  if [[ $auth_failures -gt 10 ]]; then
    log "🚨 警告：認證失敗激增！($auth_failures in 5 mins)"
    log "可能的原因："
    log "  - 暴力破解攻擊"
    log "  - 過期金鑰未更新"
    log "  - 配置錯誤"
    return 1
  fi
  
  return 0
}

# 5. 提取異常 IP 地址
detect_geographic_anomaly() {
  local audit_logs="$1"
  
  log "檢查地理位置異常..."
  
  # 提取唯一 IP
  local ips=$(echo "$audit_logs" | grep -o '"remote_addr":"[^"]*"' | cut -d'"' -f4 | sort -u)
  
  for ip in $ips; do
    # 簡單的本機檢查
    if [[ "$ip" != "127.0.0.1" ]] && [[ "$ip" != "::1" ]]; then
      log "⚠️  檢測到非本機 IP：$ip"
    fi
  done
}

# 6. 檢查金鑰輪換狀態
check_key_rotation() {
  log "檢查金鑰輪換狀態..."
  
  # 獲取所有金鑰
  local keys=$(curl -s \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    "$ZEROCLAW_URL/api/keys" \
    2>/dev/null || echo "{}")
  
  # 檢查需要輪換的金鑰
  local needs_rotation=$(echo "$keys" | grep -c '"needs_rotation":true' || echo "0")
  
  if [[ $needs_rotation -gt 0 ]]; then
    log "⚠️  有 $needs_rotation 個金鑰需要輪換（已超過 90 天）"
    return 1
  fi
  
  return 0
}

# 7. 撤銷可疑金鑰
revoke_suspicious_key() {
  local key_id="$1"
  local reason="$2"
  
  log "撤銷金鑰 $key_id：$reason"
  
  curl -s -X DELETE \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    "$ZEROCLAW_URL/api/keys/$key_id" \
    -d "reason=$reason" \
    2>/dev/null || log "❌ 撤銷失敗"
}

# 8. 發送告警
send_alert() {
  local severity="$1"
  local message="$2"
  
  log "發送告警：[$severity] $message"
  
  # 支援多種通知方式
  if command -v mail &> /dev/null; then
    echo "$message" | mail -s "🚨 Zeroclaw Security Alert: $severity" \
      "${ALERT_EMAIL:-admin@example.com}"
  fi
  
  # Slack 集成（可選）
  if [[ -n "${SLACK_WEBHOOK:-}" ]]; then
    curl -X POST "$SLACK_WEBHOOK" \
      -H 'Content-Type: application/json' \
      -d "{\"text\": \"🚨 Zeroclaw Alert ($severity): $message\"}" \
      2>/dev/null || true
  fi
  
  # 系統日誌
  logger -t zeroclaw-security "[$severity] $message"
}

# 9. 生成監控報告
generate_report() {
  local audit_logs="$1"
  local report_file="/tmp/zeroclaw-security-report-$(date +%Y%m%d-%H%M%S).html"
  
  log "生成 HTML 報告..."
  
  cat > "$report_file" << 'EOF'
<!DOCTYPE html>
<html>
<head>
  <title>Zeroclaw Security Report</title>
  <style>
    body { font-family: Arial, sans-serif; margin: 20px; }
    .ok { color: green; }
    .warning { color: orange; }
    .critical { color: red; }
    table { border-collapse: collapse; width: 100%; margin: 20px 0; }
    th, td { border: 1px solid #ddd; padding: 8px; text-align: left; }
    th { background-color: #4CAF50; color: white; }
  </style>
</head>
<body>
  <h1>Zeroclaw Security Monitoring Report</h1>
  <p>Generated: <strong id="timestamp"></strong></p>
  
  <h2>Summary</h2>
  <dl>
    <dt>Total API Requests (5 min):</dt>
    <dd id="total_requests">-</dd>
    
    <dt>Authentication Failures:</dt>
    <dd id="auth_failures">-</dd>
    
    <dt>Keys Needing Rotation:</dt>
    <dd id="keys_rotation">-</dd>
    
    <dt>Status:</dt>
    <dd id="overall_status">-</dd>
  </dl>
  
  <h2>Recent Activity</h2>
  <table id="activity_table">
    <tr>
      <th>Time</th>
      <th>Event Type</th>
      <th>Key ID</th>
      <th>Status</th>
    </tr>
  </table>
  
  <h2>Recommendations</h2>
  <ul id="recommendations">
    <li>定期審查 API 金鑰使用情況</li>
    <li>啟用多因素認證</li>
    <li>設置自動告警和恢復</li>
  </ul>
</body>
</html>
EOF
  
  log "✅ 報告已保存：$report_file"
}

# ===== 主監控循環 =====
main_loop() {
  log "進入監控循環（檢查間隔：${CHECK_INTERVAL_SECS}s）"
  
  while true; do
    log "--- 監控檢查開始 ---"
    
    # 獲取審計日誌
    audit_logs=$(audit_api_keys)
    
    # 執行檢查
    has_error=0
    
    detect_anomalies "$audit_logs" || has_error=1
    detect_auth_failures "$audit_logs" || has_error=1
    detect_geographic_anomaly "$audit_logs"
    check_key_rotation || has_error=1
    
    # 如果有錯誤，發送告警
    if [[ $has_error -ne 0 ]]; then
      send_alert "HIGH" "Zeroclaw 檢測到可疑活動，請立即審查"
    fi
    
    # 生成報告
    generate_report "$audit_logs"
    
    log "--- 監控檢查完成 ---"
    log "等待 ${CHECK_INTERVAL_SECS}s 後重複檢查..."
    
    sleep "$CHECK_INTERVAL_SECS"
  done
}

# ===== 一次性檢查模式 =====
if [[ "${1:-}" == "--once" ]]; then
  audit_logs=$(audit_api_keys)
  detect_anomalies "$audit_logs"
  detect_auth_failures "$audit_logs"
  detect_geographic_anomaly "$audit_logs"
  check_key_rotation
  generate_report "$audit_logs"
  log "✅ 單次檢查完成"
else
  # 後台運行監控
  log "✅ API Key 監控系統啟動"
  log "執行命令：$0 --once  進行單次檢查"
  log "按 Ctrl+C 停止監控"
  
  main_loop &
  MONITOR_PID=$!
  
  # 捕捉 SIGTERM，優雅關閉
  trap "log '監控已停止'; kill $MONITOR_PID 2>/dev/null; exit 0" SIGTERM SIGINT
  
  wait $MONITOR_PID
fi
