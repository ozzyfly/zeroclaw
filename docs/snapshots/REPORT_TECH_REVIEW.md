# ZeroClaw 技術審查報告（含風險矩陣與驗證清單）

日期：2026-05-11

## 1. 審查範圍

- 倉庫層級：`zero/`
- 主要程式：`zeroclaw/`
- 規格治理：`openspec/`

## 2. 架構判讀

### 2.1 核心設計

專案以 Trait + Factory 為核心，主體擴充點為 Provider/Channel/Tool/Memory/Runtime/Observer。此模式可在新增能力時限制影響面，有利穩定維運。

### 2.2 模組責任清晰度

- `src/main.rs`：CLI 命令入口
- `src/agent/loop_.rs`：代理執行主循環
- `src/config/`：設定載入、解析與遷移
- `src/security/`：策略、秘密管理、配對授權
- `src/gateway/`：對外 HTTP 入口

整體責任分界清楚，符合中大型 Rust 專案可維護性原則。

## 3. 可建置性與品質門檻

### 3.1 工具鏈與版本

- Rust edition：2021
- MSRV：1.87
- workspace members：主 crate + `crates/robot-kit`

### 3.2 現有品質流程

- fmt / clippy / test
- `dev/ci.sh` 多目標執行
- `scripts/ci/*` 原生質量腳本

整體具備合格的工程化基礎。

## 4. 風險矩陣

| 風險項 | 影響度 | 發生機率 | 風險等級 | 說明 | 建議控制措施 |
|---|---|---|---|---|---|
| Gateway 對外暴露與代理路由錯配 | 高 | 中 | 高 | 涉及 HTTP/WS/TLS，多層轉發容易配置錯誤 | 建立 e2e health + ws 探針測試；加上 preflight 配置檢查 |
| Tool 執行權限過寬 | 高 | 中 | 高 | 可能造成命令/檔案能力過度暴露 | 預設最小權限策略、允許清單、審計日誌必開 |
| Feature 組合回歸 | 中 | 高 | 高 | `whatsapp-web` 等選配能力多，組合爆炸 | 設計分層矩陣：核心組合每日跑、長尾組合每週跑 |
| VPS 部署配置漂移 | 中 | 中 | 中 | compose/nginx/systemd/certbot 鏈路長 | 建立部署後自動核對腳本與定期巡檢 |
| 規格文件 Purpose 未補齊 | 中 | 中 | 中 | 影響治理追溯與決策一致性 | 補齊 openspec Purpose 並納入 PR 檢核 |

## 5. 驗證清單（建議作為 release gate）

### 5.1 安全與接口

- [ ] Gateway 健康檢查與 WebSocket 連線測試通過
- [ ] 安全策略路徑（security/runtime/tools）關鍵測試通過
- [ ] 祕密管理與憑證處理流程驗證完成

### 5.2 品質與相容

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test --locked`
- [ ] 常用 feature 組合 smoke tests 通過

### 5.3 部署與運維

- [ ] `docker compose -f deploy/docker-compose.prod.yml config` 無誤
- [ ] Nginx 配置語法與 TLS 續約流程驗證完成
- [ ] `systemd` 服務重啟/開機啟動驗證完成
- [ ] VPS 部署腳本重跑（冪等）驗證通過

### 5.4 規格與文件

- [ ] openspec 變更與實作一致
- [ ] Purpose 為 TBD 的 spec 已補齊
- [ ] 文件連結與操作步驟可重現

## 6. 30 天技術執行計畫（建議）

1. 週 1：完成高風險路徑 smoke tests 與 pipeline 強制化。
2. 週 2：定義並導入 feature 組合矩陣（核心每日、長尾每週）。
3. 週 3：部署核對腳本 + 故障注入演練（TLS 到期、容器重啟、反代故障）。
4. 週 4：規格與文件補齊，整理 release gate，進行一次完整發布演練。

## 7. 技術結論

專案架構成熟且方向正確，下一階段重點不在大改，而在「驗證能力產品化」：把測試、部署核對、風險控制流程變成固定門檻，才能在高迭代下維持穩定交付。
