#!/bin/bash
# scripts/security/README.md
# Zeroclaw 安全部署快速參考

## 🚀 快速開始（5 分鐘部署）

```bash
cd zeroclaw/scripts/security

# 1️⃣ 啟用身份驗證（3 分鐘）
sudo chmod +x enable-authentication.sh
./enable-authentication.sh

# 2️⃣ 配置防火牆（2 分鐘）
sudo chmod +x setup-firewall.sh
sudo ./setup-firewall.sh

# 驗證
curl -H "Authorization: Bearer $(cat ~/.zeroclaw/.auth-token.secret)" \
  http://localhost:42617/api/status
```

---

## 📋 完整部署過程

### 階段 1：基礎安全（第 1 天）

#### A. 身份驗證 ✅
```bash
# 已啟用：Bearer Token 認證
# 驗證：
curl -H "Authorization: Bearer <TOKEN>" http://localhost:42617/api/status
```

#### B. 防火牆配置 ✅
```bash
# 編輯配置
sudo vim /etc/zeroclaw/firewall.conf

# 添加您的可信 IP：
ALLOWED_CIDR_BLOCKS=(
  "127.0.0.1/32"
  "YOUR_OFFICE_IP/24"
  "YOUR_VPN_IP/32"
)

# 應用規則
sudo scripts/security/setup-firewall.sh
```

#### C. 反向代理部署 ✅
```bash
# Nginx 配置已準備
sudo cp dev/nginx/zeroclaw.conf /etc/nginx/sites-available/
sudo ln -s /etc/nginx/sites-available/zeroclaw.conf \
           /etc/nginx/sites-enabled/

# 驗證配置
sudo nginx -t

# 重啟
sudo systemctl restart nginx

# 驗證可訪問性
curl https://api.zeroclaw.example.com/api/status
```

---

### 階段 2：依賴安全（第 2 天）

#### A. CVE 掃描 ✅
```bash
# 一次性掃描
chmod +x scripts/security/audit-dependencies.sh
./scripts/security/audit-dependencies.sh

# 檢查結果
cargo audit

# 自動修補
cargo audit fix
```

#### B. CI/CD 集成 ✅
在 `.github/workflows/security.yml` 中添加：
```yaml
- name: Audit Dependencies
  run: cargo audit
  
- name: Check Outdated
  run: cargo outdated
```

---

### 階段 3：監控和告警（第 3 天）

#### A. API Key 監控 ✅
```bash
# 部署監控服務
chmod +x scripts/security/monitor-api-keys.sh

# 一次性檢查
export ZEROCLAW_ADMIN_TOKEN="<your-admin-token>"
./scripts/security/monitor-api-keys.sh --once

# 啟動後台監控
nohup ./scripts/security/monitor-api-keys.sh > /var/log/zeroclaw-monitor.log 2>&1 &
```

#### B. Prometheus + Grafana（可選）
```bash
# 啟動監控容器
docker-compose -f dev/docker-compose.monitoring.yml up -d

# 訪問 Grafana
# http://localhost:3000 (admin / password)
```

---

## 🔒 安全檢查清單

### 日常檢查
- [ ] 檢查審計日誌有無異常
  ```bash
  tail -f /var/log/zeroclaw/audit.log | grep -E "CRITICAL|ERROR"
  ```

- [ ] 監控 API 金鑰使用
  ```bash
  curl -H "Authorization: Bearer $TOKEN" \
    http://localhost:42617/api/keys | jq '.keys[] | select(.needs_rotation == true)'
  ```

- [ ] 驗證防火牆規則
  ```bash
  sudo ufw status verbose | grep 42617
  ```

### 週度檢查
- [ ] 掃描新漏洞
  ```bash
  cargo audit
  ```

- [ ] 檢查過時依賴
  ```bash
  cargo outdated --format list | grep "^"
  ```

- [ ] 審查審計日誌
  ```bash
  # 查看最近 7 天的登入
  grep "AuthenticationSuccess\|AuthenticationFailure" /var/log/zeroclaw/audit.log | tail -100
  ```

### 月度檢查
- [ ] 輪換 API 金鑰
  ```bash
  ./scripts/security/rotate-api-keys.sh
  ```

- [ ] 更新依賴主版本
  ```bash
  cargo update
  cargo test --all
  ```

- [ ] 安全審計報告
  ```bash
  ./scripts/security/audit-dependencies.sh > /tmp/monthly-audit.txt
  cat /tmp/monthly-audit.txt
  ```

---

## 🚨 應急響應

### 若檢測到異常 API 活動

**立即動作（1 分鐘內）：**
```bash
# 1. 確認告警
tail -20f /var/log/zeroclaw/audit.log

# 2. 識別受害金鑰
grep "key_id" /var/log/zeroclaw/audit.log | sort | uniq -c | sort -rn | head -5

# 3. 撤銷金鑰
curl -X DELETE \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  "http://localhost:42617/api/keys/<COMPROMISED_KEY_ID>" \
  -d "reason=Security incident - anomalous activity detected"
```

**調查（10 分鐘內）：**
```bash
# 4. 檢查訪問的端點
grep "key_id: <KEY_ID>" /var/log/zeroclaw/audit.log |\
  awk '{print $NF}' | sort | uniq -c | sort -rn

# 5. 提取 IP 地址
grep "key_id: <KEY_ID>" /var/log/zeroclaw/audit.log | \
  grep -o 'remote_addr: [^ ]*' | sort | uniq -c

# 6. 生成事件報告
generateIncidentReport "<KEY_ID>" "/tmp/incident-report.txt"
```

**恢復（30 分鐘內）：**
```bash
# 7. 生成新金鑰
NEW_KEY=$(curl -s -X POST \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  "http://localhost:42617/api/keys/create" | jq -r '.key')

# 8. 通知客戶端
# 更新應用配置以使用新金鑰

# 9. 分析根本原因
# 檢查代碼變更、配置、依賴
```

---

## 📊 監控命令快速參考

```bash
# 監控 API 流量
watch -n5 'curl -s -H "Authorization: Bearer $TOKEN" \
  http://localhost:42617/api/metrics | jq ".api_requests_total"'

# 監控金鑰使用
watch -n5 'curl -s -H "Authorization: Bearer $TOKEN" \
  http://localhost:42617/api/keys | jq ".keys | length"'

# 監控系統健康
watch -n5 'curl -s -H "Authorization: Bearer $TOKEN" \
  http://localhost:42617/api/health'

# 查看防火牆日誌
sudo tail -f /var/log/ufw.log | grep 42617

# 查看 Nginx 訪問日誌
tail -f /var/log/nginx/zeroclaw-api-access.log

# 檢查失敗登入
grep "FAILED\|401\|403" /var/log/zeroclaw/audit.log | tail -50
```

---

## 🔧 常見故障排除

### 問題 1：Cannot connect to API
```bash
# 診斷
1. 檢查服務運行
   ps aux | grep zeroclaw

2. 檢查端口監聽
   netstat -tlnp | grep 42617

3. 檢查防火牆
   sudo ufw status | grep 42617

4. 測試本機訪問
   curl http://127.0.0.1:42617/api/status
```

### 問題 2：Authentication failed
```bash
# 診斷
1. 驗證令牌存在
   cat ~/.zeroclaw/.auth-token.secret

2. 檢查令牌有效性
   curl -H "Authorization: Bearer $(cat ~/.zeroclaw/.auth-token.secret)" \
     http://localhost:42617/api/status

3. 重新生成令牌
   curl -X POST http://localhost:42617/pair
```

### 問題 3：High API latency
```bash
# 診斷
1. 檢查上游服務
   curl -H "Authorization: Bearer $TOKEN" \
     http://localhost:42617/api/health

2. 監控資源
   top -p $(pgrep -f zeroclaw)

3. 檢查網路延遲
   ping api.zeroclaw.example.com
```

---

## 📚 相關文檔

- [完整安全部署檢查清單](./security-deployment-checklist.md)
- [Zeroclaw 配置參考](./config-reference.md)
- [安全性最佳實踐](./agnostic-security.md)
- [OWASP API 安全 Top 10](https://owasp.org/www-project-api-security/)

---

## 🤝 支援和反饋

若有安全問題或改進建議，請：
1. 不要公開發布（安全漏洞應私密報告）
2. 聯繫 security@zeroclaw.example.com
3. 參考 SECURITY.md

---

**最後更新**：2026-03-05
**維護者**：Zeroclaw Security Team
