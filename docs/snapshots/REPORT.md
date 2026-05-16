# ZeroClaw 專案總報告

版本：v1.0
日期：2026-05-11
報告範圍：workspace 根目錄 `zero/`（主要程式在 `zeroclaw/`）

## 1. 執行摘要

ZeroClaw 是一個 Rust-first 的自治代理執行平台，核心價值在於：

- 低資源占用（適合邊緣設備與低成本 VPS）
- 安全優先（策略、隔離、金鑰與憑證治理）
- 高可擴充（Provider/Channel/Tool/Memory/Runtime 模組化）
- 可部署性高（CLI、Docker、VPS、反向代理與 systemd）

專案已具備完整工程骨架（格式化、靜態檢查、測試、CI 腳本、Spec-Driven 流程），可支援持續擴張。當前主要風險集中於高變更量下的回歸穩定性與多功能組合測試矩陣管理。

## 2. 專案定位與目標

ZeroClaw 以「代理執行基礎設施」為定位，將 AI 代理所需核心能力做成可替換元件：

- 模型供應商可替換（OpenAI-compatible 與多家後端）
- 通道可替換（Telegram/WhatsApp/Matrix 等）
- 工具可替換（shell、檔案、HTTP、browser、memory）
- 執行隔離可替換（native / landlock / bubblewrap）

此設計能降低單一供應商鎖定風險，並提升在不同部署環境的可移植性。

## 3. 技術架構總覽

### 3.1 架構模式

採用 Trait + Factory 模式：

- 新能力多數透過「實作 trait + 在 factory 註冊」完成
- 避免跨模組大改，維持系統穩定邊界

### 3.2 主要模組

- CLI/進入點：`src/main.rs`
- Agent orchestration：`src/agent/loop_.rs`
- 設定與遷移：`src/config/`
- 安全子系統：`src/security/`
- Gateway：`src/gateway/`
- Web 儀表板：`web/`（嵌入二進位）

### 3.3 能力開關（Features）

重要可選能力含：

- `channel-matrix`, `channel-lark`
- `memory-postgres`
- `observability-otel`
- `browser-native`
- `sandbox-landlock`, `sandbox-bubblewrap`
- `peripheral-rpi`, `probe`, `rag-pdf`, `whatsapp-web`

## 4. 工程與品質體系

### 4.1 標準品質流程

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test --locked`
- `./dev/ci.sh all`（Docker）
- 原生質量腳本：`scripts/ci/*`

### 4.2 開發流程治理

專案採 Spectra 規格驅動流程：

- 規格：`openspec/specs/`
- 變更提案：`openspec/changes/`
- 流程：discuss → propose → apply/ingest → archive

## 5. 部署與運維能力

### 5.1 已建立部署資產

- `deploy/docker-compose.prod.yml`
- `deploy/nginx.conf`
- `deploy/zeroclaw.service`
- `scripts/deploy-vps.sh`
- `scripts/hetzner-deploy.sh`
- 文件：`docs/vps-deployment-guide.md`, `docs/hetzner-deployment-guide.md`

### 5.2 規格化部署要求（openspec）

- VPS 規格包含：容器化、TLS、服務託管、冪等部署
- Hetzner 規格包含：雲主機建置、防火牆、遠端自動部署

## 6. 量化快照（本次盤點）

- `zeroclaw/src` 檔案數：214
- `zeroclaw/tests` 檔案數：19
- `zeroclaw/docs` 檔案數：146
- `openspec/specs/*/spec.md`：2 份
- Requirement 條目：7 條

解讀：文件成熟度高；測試存在但仍可針對高風險路徑提升密度。

## 7. 風險盤點

### 7.1 高風險路徑

- `src/security/**`
- `src/runtime/**`
- `src/gateway/**`
- `src/tools/**`
- `.github/workflows/**`

### 7.2 主要風險

1. 對外介面風險：Gateway + Tool 執行能力若未嚴格限制，可能擴大攻擊面。
2. 組合回歸風險：feature flags 與多 provider/channel 組合使測試矩陣膨脹。
3. 部署漂移風險：VPS、Nginx、TLS、systemd、compose 鏈路長，配置漂移會造成中斷。
4. 發佈治理風險：當前工作樹變更量大，需強化切片與回退策略。

## 8. 建議與執行優先順序

1. 建立最小關鍵路徑 smoke tests（security/gateway/deploy）。
2. 建立 feature 組合測試策略（常用組合 + 高風險組合）。
3. 補齊 openspec 中 Purpose 為 TBD 的規格敘述。
4. 發佈前執行變更切片（主題化 PR）與風險標記。

## 9. 結論

ZeroClaw 具備強架構與良好可擴充性，已達到可持續演進的工程基礎。下一階段關鍵在於：

- 用更系統化測試治理變動風險
- 用更嚴謹發佈流程提升穩定交付
- 持續維持安全優先的預設策略

---

## 附件

- 管理層一頁版：`REPORT_EXECUTIVE_1PAGE.md`
- 技術審查版：`REPORT_TECH_REVIEW.md`
