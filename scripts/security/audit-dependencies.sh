#!/bin/bash
# scripts/security/audit-dependencies.sh
# 掃描已知 CVE 漏洞並自動修補

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
log() {
  echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1"
}

error() {
  log "❌ $1"
  exit 1
}

log "=== Zeroclaw 依賴安全審計 ==="

# 1. 檢查 cargo-audit 工具
log "檢查 cargo-audit..."

if ! command -v cargo-audit &> /dev/null; then
  log "安裝 cargo-audit..."
  cargo install cargo-audit
fi

# 2. 執行安全審計
log "掃描已知漏洞..."

AUDIT_REPORT=$(mktemp)
VULN_COUNT=0

set +e
cargo audit --json 2>&1 | tee "$AUDIT_REPORT" > /dev/null
AUDIT_EXIT=$?
set -e

if grep -q '"vulnerabilities"' "$AUDIT_REPORT" 2>/dev/null; then
  VULN_COUNT=$(grep -c '"kind":"vulnerability"' "$AUDIT_REPORT" 2>/dev/null || echo "0")
fi

if [[ $AUDIT_EXIT -ne 0 ]] && [[ $VULN_COUNT -gt 0 ]]; then
  log "⚠️  發現 $VULN_COUNT 個漏洞"
  log ""
  cargo audit
  log ""
  
  # 3. 自動修複
  log "嘗試自動修複漏洞..."
  if cargo audit fix; then
    log "✅ 漏洞已修複"
  else
    log "❌ 自動修複失敗，請手動檢查"
  fi
else
  log "✅ 未發現已知漏洞"
fi

# 4. 檢查過時依賴
log ""
log "檢查過時依賴..."

if command -v cargo-outdated &> /dev/null; then
  cargo outdated --exit-code=0 || true
else
  log "安裝 cargo-outdated..."
  cargo install cargo-outdated
  cargo outdated --exit-code=0 || true
fi

# 5. 檢查重複依賴
log ""
log "檢查重複依賴..."

DUPLICATES=$(cargo tree --duplicates | grep -c "dupl" || echo "0")
if [[ $DUPLICATES -gt 0 ]]; then
  log "⚠️  發現 $DUPLICATES 個重複依賴："
  cargo tree --duplicates
  log "建議："
  log "  - 更新所有依賴：cargo update"
  log "  - 檢查不兼容版本要求"
else
  log "✅ 無重複依賴"
fi

# 6. 生成依賴樹
log ""
log "生成依賴樹報告..."

DEPS_REPORT="/tmp/zeroclaw-dependencies.txt"
cargo tree > "$DEPS_REPORT"
log "✅ 依賴報告已保存到 $DEPS_REPORT"

# 7. 許可證檢查
log ""
log "檢查許可證合規性..."

if command -v cargo-license &> /dev/null; then
  cargo license
else
  log "安裝 cargo-license..."
  cargo install cargo-license
  cargo license
fi

# 8. 更新 Cargo.lock
log ""
log "更新 Cargo.lock..."

git diff Cargo.lock > /dev/null && {
  log "⚠️  Cargo.lock 已修改（包含安全更新）"
  git add Cargo.lock
  git commit -m "security: update dependencies to patch CVE" || true
} || {
  log "✅ Cargo.lock 無需更新"
}

# 9. 執行測試
log ""
log "執行安全測試..."

if cargo test --lib 2>&1 | grep -q "test result: ok"; then
  log "✅ 所有測試通過"
else
  error "測試失敗 - 修復依賴時可能引入了不兼容性"
fi

rm -f "$AUDIT_REPORT"

log ""
log "=== ✅ 依賴審計完成 ==="
log ""
log "建議："
log "1. 定期運行此腳本（每週或 CI/CD 流程中）"
log "2. 訂閱 cargo 的安全公告"
log "3. 在更新主版本依賴後充分測試"
log ""
