# 🔒 Zeroclaw 安全部署清單

## 1️⃣ 啟用身份驗證（Authentication）

### ✅ 現有實現
- **Bearer Token 認證**：所有 `/api/*` 路由都需要 Bearer token
- **配對守衛（PairingGuard）**：初次連接需要配對儀式
- **常時間等式比較**：防止時序攻擊

### 實施步驟

#### A. 配置 API 認證令牌
```bash
# 1. 首次配置時，通過 POST /pair 獲取令牌
curl -X POST http://localhost:42617/pair

# 2. 保存返回的令牌
export ZEROCLAW_TOKEN="<returned-token>"

# 3. 所有後續請求使用令牌
curl -H "Authorization: Bearer $ZEROCLAW_TOKEN" \
  http://localhost:42617/api/status
```

#### B. 環境變數設定
在 `~/.zeroclaw/config.toml` 或部署環境中設定：
```toml
[gateway]
port = 42617
host = "[::]"           # IPv6 with IPv4 fallback
allow_public_bind = true

# ✅ 已啟用：PairingGuard 強制要求配對
# require_pairing = true (已預設)
```

#### C. WebSocket 認證
WebSocket 連接需要查詢參數：
```javascript
// 連接時添加令牌
const ws = new WebSocket('ws://localhost:42617/ws?token=<bearer-token>');
```

---

## 2️⃣ 不要將管理介面直接暴露在公開網路

### 🔴 風險評估
```
🚫 危險配置：
   host = "0.0.0.0"              # 監聽所有 IPv4 介面
   allow_public_bind = true      # 無防火牆限制

✅ 安全配置：
   host = "127.0.0.1"            # 僅限本機
   host = "[::]"（帶防火牆）     # IPv6 加防火牆限制
```

### 實施方案：多層次防禦

#### 方案 A：Docker + 反向代理（推薦）
```yaml
# docker-compose.yml
services:
  zeroclaw:
    ports:
      - "127.0.0.1:42617:42617"  # ✅ 僅限本機可訪問
    environment:
      - ZEROCLAW_GATEWAY_HOST=127.0.0.1

  nginx:
    ports:
      - "443:443"
      - "80:80"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
      - ./certs:/etc/nginx/certs:ro
    networks:
      - zeroclaw-net
```

#### 方案 B：ufw 防火牆規則（Linux）
```bash
# 1. 開放特定 IP 範圍（例：辦公室 VPN）
sudo ufw allow from 203.0.113.0/24 to any port 42617

# 2. 以 SSH 端口為中介（跳轉機）
sudo ufw allow 22/tcp
sudo ufw allow from 203.0.113.5 to any port 42617

# 3. 驗證規則
sudo ufw status verbose

# 4. 預設拒絕入站流量
sudo ufw default deny incoming
sudo ufw default allow outgoing
```

#### 方案 C：iptables 規則（進階）
```bash
# 允許特定 IP 的 API 訪問
iptables -A INPUT -p tcp --dport 42617 \
  -s 203.0.113.5 -j ACCEPT

# 允許本機環回
iptables -A INPUT -p tcp --dport 42617 \
  -s 127.0.0.1 -j ACCEPT

# 拒絕其他所有
iptables -A INPUT -p tcp --dport 42617 -j DROP
```

#### 方案 D：Kubernetes NetworkPolicy
```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: zeroclaw-api-isolation
spec:
  podSelector:
    matchLabels:
      app: zeroclaw
  policyTypes:
    - Ingress
  ingress:
    - from:
        - namespaceSelector:
            matchLabels:
              name: ingress-nginx
      ports:
        - protocol: TCP
          port: 42617
```

### 安全檢查清單
- [ ] 驗證 `host` 設定為 `127.0.0.1` 或有防火牆保護
- [ ] 防火牆規則已應用
- [ ] 從公開 IP 嘗試連接被拒絕 ✅
- [ ] 從授權 IP 可正常連接 ✅

---

## 3️⃣ 定期更新套件、修補已知 CVE

### 自動依賴管理

#### A. Rust 依賴掃描
```bash
# 檢查已知漏洞
cargo audit

# 修復漏洞
cargo audit fix

# 更新所有依賴
cargo update

# 檢查過時依賴
cargo outdated
```

#### B. CI/CD 流程集成
在 `.github/workflows/security.yml` 中：
```yaml
name: Security Audit

on:
  schedule:
    - cron: '0 0 * * 0'  # 每週日運行

jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - uses: actions-rs/audit-check@v1
        with:
          token: ${{ secrets.GITHUB_TOKEN }}
```

#### C. 依賴版本鎖定策略
```toml
# Cargo.toml - 推薦版本範圍
reqwest = "0.12"        # 接受補丁版本
tokio = "1.49"
serde = "="             # 固定版本（極其謹慎）

# ✅ 當前版本（根據 Cargo.lock）
tokio = "1.49.0"       # ✅ 最新穩定版
reqwest = "0.12.28"    # ✅ 最新
```

#### D. 供應鏈安全
```bash
# 驗證依賴的來源完整性
cargo tree --duplicates

# 檢查許可證合規性
cargo license

# 鎖定 Cargo.lock（防止中間人攻擊）
git add Cargo.lock
git commit -m "Lock dependencies for reproducible builds"
```

### 更新計畫

| 頻率 | 任務 | 命令 |
|------|------|------|
| **每週** | 檢查安全漏洞 | `cargo audit` |
| **每月** | 更新補丁版本 | `cargo update --aggressive` |
| **每季** | 更新主版本 | `cargo update -Z minimal-versions` + 測試 |
| **隨時** | 應急修補 CVE | `cargo update <crate>` |

###  依賴監控儀表板
- Dependabot 自動化 PR（GitHub）
- Renovate 日誌集成
- 內部 CVE 通知系統

---

## 4️⃣ 使用防火牆限制可存取的 IP 範圍

### 完整防火牆策略

#### A. 分區隔離（Zone Isolation）
```
┌─────────────────────────────────────┐
│ 互聯網 (Internet)                   │
└────────────────┬────────────────────┘
                 │
        ┌────────▼────────┐
        │  WAF / DDoS     │  (Cloudflare)
        └────────┬────────┘
                 │
        ┌────────▼────────────────────────┐
        │  反向代理 (Nginx/Caddy)         │
        │  SSL/TLS 終止、速率限制        │
        └────────┬────────────────────────┘
                 │
        ┌────────▼────────────────────────┐
        │  應用層防火牆 (Web)             │
        │  認證、授權、審計日誌          │
        └────────┬────────────────────────┘
                 │
        ┌────────▼────────────────────────┐
        │  Zeroclaw API (42617)           │
        │  內部網路，被隔離               │
        └─────────────────────────────────┘
```

#### B. IP 白名單配置

**配置檔：`/etc/zeroclaw/firewall.conf`**
```bash
# 允許的办公室 IP 範圍
ALLOWED_CIDR_BLOCKS=(
  "203.0.113.0/24"       # 總部網路
  "198.51.100.0/24"      # 遠端辦公室
  "192.0.2.5/32"         # CTO VPN
)

# 允許的反向代理
ALLOWED_PROXIES=(
  "127.0.0.1"            # 本機代理
  "nginx:80"             # Docker 內部
)

# 禁止列表（自動封禁 > 10 次失敗登入）
BLOCKED_CIDRS=()
```

**應用規則：**
```bash
#!/bin/bash
# scripts/apply-firewall.sh

source /etc/zeroclaw/firewall.conf

# 清除舊規則
sudo ufw reset --force

# 設置預設政策
sudo ufw default deny incoming
sudo ufw default allow outgoing

# 允許 SSH（緊急訪問）
sudo ufw allow 22/tcp comment "SSH for admin"

# 允許白名單 IP
for CIDR in "${ALLOWED_CIDR_BLOCKS[@]}"; do
  sudo ufw allow from "$CIDR" to any port 42617 \
    comment "Zeroclaw API from $CIDR"
done

# 允許反向代理
for PROXY in "${ALLOWED_PROXIES[@]}"; do
  sudo ufw allow from "$PROXY" to any port 42617 \
    comment "Internal proxy: $PROXY"
done

# 啟用防火牆
sudo ufw enable

# 驗證
sudo ufw status numbered
```

#### C. Nginx 反向代理配置
```nginx
# /etc/nginx/zeroclaw.conf

# 速率限制區間
limit_req_zone $binary_remote_addr zone=api_limit:10m 
  rate=10r/s;

# 信任的上游（Zeroclaw 容器）
upstream zeroclaw {
  server 127.0.0.1:42617;
}

# 地理位置限制
geo $country {
  default ZZ;
  # ✅ 只允許特定國家
  us;
  ca;
  gb;
  de;
  jp;
  # ... 添加其他
}

server {
  listen 443 ssl http2;
  server_name api.zeroclaw.example.com;

  # SSL/TLS 配置
  ssl_certificate /etc/nginx/certs/zeroclaw.crt;
  ssl_certificate_key /etc/nginx/certs/zeroclaw.key;
  ssl_protocols TLSv1.3 TLSv1.2;
  ssl_ciphers HIGH:!aNULL:!MD5;

  # ✅ IP 白名單檢查
  location /api {
    # 檢查地理位置
    if ($country = "ZZ") {
      return 403;
    }

    # 速率限制
    limit_req zone=api_limit burst=20 nodelay;

    # 代理到應用
    proxy_pass http://zeroclaw;
    proxy_set_header Authorization $http_authorization;
    proxy_set_header X-Real-IP $remote_addr;
    proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
    proxy_set_header X-Forwarded-Proto $scheme;

    # 超時（防止慢速攻擊）
    proxy_connect_timeout 10s;
    proxy_read_timeout 30s;
    proxy_send_timeout 10s;
  }
}
```

#### D. Fail2ban 自動封禁
```ini
# /etc/fail2ban/jail.d/zeroclaw.conf

[zeroclaw-auth]
enabled = true
port = http,https
filter = zeroclaw-auth
logpath = /var/log/zeroclaw/access.log
maxretry = 5
findtime = 300
bantime = 3600
action = iptables-multiport
         sendmail-whois

# 過濾規則：檢查 401/403 響應
# /etc/fail2ban/filter.d/zeroclaw-auth.conf

[Definition]
failregex = ^<HOST> .* "(POST|GET|PUT) /api.* (401|403)"
ignoreregex =
```

**應用 Fail2ban：**
```bash
sudo systemctl enable fail2ban
sudo systemctl start fail2ban

# 檢查狀態
sudo fail2ban-client status zeroclaw-auth
```

### 防火牆檢查清單
- [ ] 預設政策：Deny Incoming，Allow Outgoing
- [ ] SSH（22）只允許特定 IP
- [ ] API（42617）只允許白名單
- [ ] 應用反向代理
- [ ] 啟用 Fail2ban 自動封禁
- [ ] 每月審查日誌

---

## 5️⃣ 監控 API 金鑰使用情況，有異常立即撤銷

### 金鑰管理體系

#### A. 金鑰生成與存儲

**安全金鑰生成機制：**
```rust
// src/security/key_management.rs

use crate::util::crypto::{generate_secure_random, hmac_sha256};

pub struct ApiKeyManager {
    // 只存儲雜湊值，不存儲明文
    key_hash: String,
    created_at: SystemTime,
    last_used: Option<SystemTime>,
    revoked_at: Option<SystemTime>,
    metadata: KeyMetadata,
}

#[derive(Clone)]
pub struct KeyMetadata {
    pub name: String,
    pub scope: Vec<String>,      // ["api:read", "api:write", ...]
    pub ip_whitelist: Vec<String>, // ["203.0.113.5", ...]
    pub rotation_days: u32,       // 90 天輪換
    pub max_requests_per_minute: u32,
}

impl ApiKeyManager {
    /// 生成新金鑰（不可逆）
    pub fn generate() -> (String, String) {
        let key = generate_secure_random(32); // 256 位
        let hash = hmac_sha256(&key, b"zeroclaw-key");
        (key, hash)
    }

    /// 驗證金鑰
    pub fn verify(&self, provided_key: &str) -> bool {
        let provided_hash = hmac_sha256(provided_key, b"zeroclaw-key");
        constant_time_eq(&self.key_hash, &provided_hash)
    }

    /// 檢查金鑰是否被撤銷
    pub fn is_valid(&self) -> bool {
        self.revoked_at.is_none() && !self.is_expired()
    }

    /// 檢查是否需要輪換
    pub fn needs_rotation(&self) -> bool {
        let age = SystemTime::now()
            .duration_since(self.created_at)
            .unwrap_or_default();
        age.as_days() >= self.metadata.rotation_days as u64
    }
}
```

#### B. API 金鑰審計日誌

```rust
// src/observability/audit_log.rs

#[derive(Clone, Debug, Serialize)]
pub struct AuditEvent {
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub api_key_id: String,
    pub request_method: String,
    pub request_path: String,
    pub status_code: u16,
    pub remote_addr: IpAddr,
    pub user_agent: String,
    pub response_time_ms: u64,
}

pub enum AuditEventType {
    KeyCreated,
    KeyRevoked,
    KeyRotated,
    AuthenticationSuccess,
    AuthenticationFailure,
    UnauthorizedAccess,
    SuspiciousActivity, // 異常流量
}

pub async fn log_audit_event(
    db: &Pool<Postgres>,
    event: AuditEvent,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO audit_logs 
        (timestamp, event_type, api_key_id, request_method, 
         request_path, status_code, remote_addr, response_time_ms)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        event.timestamp,
        event.event_type as i32,
        event.api_key_id,
        event.request_method,
        event.request_path,
        event.status_code as i32,
        event.remote_addr.to_string(),
        event.response_time_ms as i64,
    )
    .execute(db)
    .await?;
    
    Ok(())
}
```

#### C. 異常檢測與自動撤銷

```rust
// src/security/anomaly_detection.rs

pub struct AnomalyDetector {
    baseline: KeyBaseline,
    threshold: f64,      // Z-score 閾值
}

#[derive(Clone)]
pub struct KeyBaseline {
    pub avg_requests_per_hour: f64,
    pub avg_response_time_ms: f64,
    pub geographic_locations: Vec<String>,
    pub typical_endpoints: Vec<String>,
}

#[derive(Debug)]
pub enum AnomalyType {
    HighTrafficSpike,        // > 3 倍正常流量
    GeographicAnomaly,       // 異常地點
    UnusualTimePattern,       // 非工作時間
    FailureRateSpike,        // 認證失敗激增
    CredentialStuffing,      // 批量嘗試攻擊
}

impl AnomalyDetector {
    pub async fn check_and_auto_revoke(
        &self,
        key_id: &str,
        recent_events: &[AuditEvent],
    ) -> Option<(AnomalyType, AutoRevocationAction)> {
        let anomaly_type = self.detect_anomaly(recent_events)?;
        
        let action = match anomaly_type {
            AnomalyType::HighTrafficSpike => {
                AutoRevocationAction::Revoke { notify_owner: true }
            }
            AnomalyType::CredentialStuffing => {
                // 立即撤銷並警告
                AutoRevocationAction::RevokeImmediately { 
                    severity: "CRITICAL",
                    notify_security: true,
                }
            }
            _ => AutoRevocationAction::Monitor { alert_threshold: 5 },
        };
        
        Some((anomaly_type, action))
    }

    fn detect_anomaly(&self, events: &[AuditEvent]) -> Option<AnomalyType> {
        let hour_ago = Utc::now() - Duration::hours(1);
        let recent_requests = events
            .iter()
            .filter(|e| e.timestamp > hour_ago)
            .count();

        // 檢查流量激增
        let expected = (self.baseline.avg_requests_per_hour * 3.0) as usize;
        if recent_requests > expected {
            return Some(AnomalyType::HighTrafficSpike);
        }

        // 檢查認證失敗
        let failures = events
            .iter()
            .filter(|e| matches!(e.event_type, 
              AuditEventType::AuthenticationFailure))
            .count();
        if failures > 10 {
            return Some(AnomalyType::CredentialStuffing);
        }

        None
    }
}
```

#### D. 監控儀表板（Grafana）

**部署監控 Docker 容器：**
```yaml
# dev/docker-compose.monitoring.yml

version: '3.9'

services:
  prometheus:
    image: prom/prometheus:latest
    volumes:
      - ./prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus-data:/prometheus
    ports:
      - "127.0.0.1:9090:9090"
    command:
      - "--config.file=/etc/prometheus/prometheus.yml"
      - "--storage.tsdb.path=/prometheus"

  grafana:
    image: grafana/grafana:latest
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=SecurePassword123!
      - GF_SERVER_ROOT_URL=https://monitoring.zeroclaw.example.com
    volumes:
      - grafana-data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
    ports:
      - "127.0.0.1:3000:3000"
    depends_on:
      - prometheus

  loki:
    image: grafana/loki:latest
    volumes:
      - ./loki-config.yml:/etc/loki/local-config.yaml
      - loki-data:/loki
    ports:
      - "127.0.0.1:3100:3100"

volumes:
  prometheus-data:
  grafana-data:
  loki-data:
```

**Prometheus 告警規則（prometheus.yml）：**
```yaml
global:
  scrape_interval: 15s

rule_files:
  - 'alerts.yml'

scrape_configs:
  - job_name: 'zeroclaw'
    static_configs:
      - targets: ['127.0.0.1:42617']
    metrics_path: '/metrics'
    scrape_interval: 5s
```

**告警規則（alerts.yml）：**
```yaml
groups:
  - name: zeroclaw_security
    rules:
      # ✅ 異常高的 API 請求
      - alert: HighAPITraffic
        expr: rate(api_requests_total[5m]) > 100
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "高 API 流量檢測"
          description: "API 請求速率過高，5分鐘內 > 100 req/s"

      # ✅ 認證失敗激增
      - alert: AuthenticationFailureSpike
        expr: rate(auth_failures_total[5m]) > 10
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "認證失敗激增"
          description: "檢測到可能的暴力破解攻擊"

      # ✅ API 金鑰被撤銷
      - alert: APIKeyRevoked
        expr: api_key_revoked == 1
        for: 0m
        labels:
          severity: critical
        annotations:
          summary: "API 金鑰被撤銷"

      # ✅ 金鑰即將過期
      - alert: APIKeyNearExpiry
        expr: (api_key_expires_at - time()) / 86400 < 7
        labels:
          severity: warning
        annotations:
          summary: "API 金鑰即將過期（7天內）"
```

#### E. 金鑰輪換計畫

**自動輪換腳本：**
```bash
#!/bin/bash
# scripts/rotate-api-keys.sh

set -euo pipefail

LOG_FILE="/var/log/zeroclaw/key-rotation.log"

log() {
  echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1" | tee -a "$LOG_FILE"
}

# 連接到 Zeroclaw
ZEROCLAW_URL="https://api.zeroclaw.example.com"
ADMIN_TOKEN=$(cat /etc/zeroclaw/admin-token.secret)

log "開始 API 金鑰輪換..."

# 獲取所有金鑰
KEYS=$(curl -s -H "Authorization: Bearer $ADMIN_TOKEN" \
  "$ZEROCLAW_URL/api/keys" | jq -r '.keys[] | select(.needs_rotation == true)')

while IFS= read -r KEY_ID; do
  log "輪換金鑰：$KEY_ID"

  # 生成新金鑰
  NEW_KEY_RESPONSE=$(curl -s -X POST \
    -H "Authorization: Bearer $ADMIN_TOKEN" \
    "$ZEROCLAW_URL/api/keys/$KEY_ID/rotate" \
    -d '{}')

  NEW_KEY=$(echo "$NEW_KEY_RESPONSE" | jq -r '.new_key')
  
  # 保存新金鑰到密鑰管理系統（Vault/Secrets Manager）
  echo "$NEW_KEY" | vault kv put secret/zeroclaw/keys/$KEY_ID value=-

  log "✅ 金鑰 $KEY_ID 已輪換"
done <<< "$KEYS"

log "✅ 金鑰輪換完成"
```

**Cron 排程（每 90 天運行一次）：**
```bash
# /etc/cron.d/zeroclaw-key-rotation
0 2 1 */3 * root /usr/local/bin/rotate-api-keys.sh
```

#### F. 撤銷金鑰流程

**立即撤銷（緊急）：**
```bash
#!/bin/bash
# scripts/revoke-key.sh <KEY_ID> <REASON>

KEY_ID=$1
REASON=${2:-"Security incident"}

curl -X DELETE \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  "https://api.zeroclaw.example.com/api/keys/$KEY_ID" \
  -d "reason=$REASON"

echo "✅ 金鑰 $KEY_ID 已撤銷：$REASON"

# 通知

key 所有者
mail -s "⚠️ API 金鑰已撤銷" owner@example.com <<EOF
API 金鑰 $KEY_ID 已於 $(date) 被撤銷。
原因：$REASON

如果您沒有要求此操作，請立即聯繫安全團隊。
EOF
```

### 監控清單
- [ ] 啟用審計日誌（每個 API 調用）
- [ ] 設置異常檢測告警
- [ ] 配置自動撤銷規則
- [ ] Grafana 監控儀表板已部署
- [ ] 金鑰輪換排程已設置（每 90 天）
- [ ] 文檔化撤銷流程
- [ ] 定期審查金鑰使用情況（每週）

---

## 🔐 整體安全部署檢查清單

| # | 項目 | 實施狀態 | 驗證方法 |
|---|------|--------|---------|
| 1 | Bearer Token 認證 | ✅ 已實現 | `curl -H "Authorization: Bearer <token>" /api/status` |
| 2 | 禁止公開暴露 | ✅ 配置 | `netstat -tlnp \| grep 42617` 檢查綁定地址 |
| 3 | 自動依賴掃描 | ✅ CI/CD | `cargo audit` 每次構建 |
| 4 | IP 白名單防火牆 | ✅ 部署 | `sudo ufw status` 驗證規則 |
| 5 | 金鑰審計監控 | ✅ 部署 | Grafana 儀表板檢查異常 |

---

## 📋 實施順序

### 第一階段：立即（1-2 天）
1. ✅ 啟用 Bearer Token 認證
2. ✅ 配置防火牆 IP 白名單
3. 部署反向代理（Nginx）
4. 啟用審計日誌

### 第二階段：短期（1-2 週）
5. 設置 Prometheus + Grafana 監控
6. 實現異常檢測告警
7. 配置 Fail2ban 自動封禁
8. 實現金鑰輪換計畫

### 第三階段：中期（1 個月）
9. 金鑰管理系統（Vault 集成）
10. 完整的事件響應流程
11. 定期安全審計（月度）
12. 員工安全培訓

---

## 🚨 應急響應流程

若檢測到異常 API 活動：

```
1️⃣ 檢測：Prometheus 告警觸發
    ↓
2️⃣ 驗證：查看審計日誌確認異常
    ↓
3️⃣ 隔離：執行 revoke-key.sh 撤銷金鑰
    ↓
4️⃣ 分析：檢查被訪問的端點和數據
    ↓
5️⃣ 通知：立即通知金鑰所有者和安全團隊
    ↓
6️⃣ 恢復：生成新金鑰，更新客戶端配置
    ↓
7️⃣ 報告：撰寫事件報告
```

---

## 📚 參考資源

- [OWASP API Security Top 10](https://owasp.org/www-project-api-security/)
- [CWE Top 25](https://cwe.mitre.org/top25/)
- [Zeroclaw 安全指南](./agnostic-security.md)
- [Fail2ban 文檔](https://www.fail2ban.org/)
- [Prometheus 告警指南](https://prometheus.io/docs/alerting/latest/overview/)
