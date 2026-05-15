use anyhow::{Context, Result};
use tracing::debug;

use crate::providers::traits::Provider;

use super::aggregator::AggregatedMacro;
use super::schemas::{AggregatedSymbolView, InvestmentReport, ReportOutput};

/// Maximum length for the WhatsApp summary (WhatsApp Cloud API limit is 4096).
const MAX_WHATSAPP_CHARS: usize = 4000;

const REPORT_SYSTEM_PROMPT: &str = r#"你是一位資深投資策略分析師。根據以下多來源彙整的投資標的分析數據，請產出一份完整的每日投資報告。

你必須以 JSON 格式回答，嚴格按照以下 schema：

{
  "report_date": "YYYY-MM-DD",
  "market_overview": "今日市場概覽（2-3句）",
  "key_themes": ["關鍵投資主題"],
  "symbols": [
    {
      "ticker": "股票代碼",
      "company_name": "公司名稱",
      "summary": "綜合分析摘要",
      "overall_sentiment": "bullish | bearish | neutral",
      "overall_confidence": 0.0 到 1.0,
      "time_horizon": "short | mid | long",
      "upside_drivers": ["上漲驅動因素"],
      "downside_risks": ["下跌風險"],
      "positioning_suggestion": "操作建議"
    }
  ],
  "disclaimer": "投資免責聲明"
}

規則：
1. 只輸出 JSON，不加任何 markdown 標記。
2. 依信心程度/討論度排序 symbols。
3. disclaimer 固定寫「本報告僅供參考，不構成投資建議。投資有風險，入市需謹慎。」
4. 用繁體中文。"#;

const SUMMARY_SYSTEM_PROMPT: &str = r#"你是一位投資報告編輯。請根據以下投資報告 JSON，產出一份適合 WhatsApp 閱讀的中文摘要。

格式要求（嚴格遵守）：

標題區：
📊 *投資訊號日報*
YYYY-MM-DD ｜ 分析 N 個來源

溫度計：
🌡 *市場溫度計*
看漲 X 🟩🟩🟩🟩🟥🟥 看空 Y
（用方塊比例表示多空比例，總共 10 格）

看漲區（sentiment=bullish 的標的，依 confidence 降序）：
📈 *看漲訊號* (X)

每個標的格式：
▎*TICKER* ・COMPANY_NAME
  HORIZON標籤 ｜ 信心 CONFIDENCE%
  💬 SUMMARY（一句話）
  📌 POSITIONING_SUGGESTION

看空區（sentiment=bearish 的標的）：
📉 *看空訊號* (Y)
（同上格式）

中性區（如有）：
⚪ *觀望訊號* (Z)
（同上格式）

市場總覽：
🌐 *市場總覽*
MARKET_OVERVIEW

尾部：
⚠️ 本報告僅供參考，不構成投資建議。

規則：
1. 使用 WhatsApp 格式：*粗體* 用星號包圍
2. 不要用 Markdown 的 # 或 **
3. HORIZON 標籤用：短線🏃 中線⏳ 長線🏔️
4. 只輸出摘要文字，不要加任何解釋
5. 總字數控制在 3000 字以內"#;

/// Generate the final investment report (JSON) and WhatsApp summary from
/// aggregated per-ticker views plus macro data.
pub async fn generate_report(
    provider: &dyn Provider,
    model: &str,
    temperature: f64,
    aggregated_symbols: &[AggregatedSymbolView],
    macro_data: &AggregatedMacro,
    report_date: &str,
    source_count: usize,
) -> Result<ReportOutput> {
    let date_str = report_date.trim().to_string();

    // ── Step 1: Generate structured JSON report ──
    let input_payload = serde_json::json!({
        "date": date_str,
        "aggregated_symbols": aggregated_symbols,
        "macro": {
            "trends": macro_data.trends,
            "themes": macro_data.themes,
            "risks": macro_data.risks,
        }
    });

    let report_user_msg = format!(
        "今天日期: {date_str}\n\n以下是彙整後的投資分析數據：\n\n{}",
        serde_json::to_string_pretty(&input_payload)
            .unwrap_or_else(|_| "(serialization error)".to_string())
    );

    debug!("generating structured report via LLM");

    let raw_report = provider
        .chat_with_system(
            Some(REPORT_SYSTEM_PROMPT),
            &report_user_msg,
            model,
            temperature,
        )
        .await
        .context("LLM report generation failed")?;

    let report_json = strip_code_fences(&raw_report);
    let report: InvestmentReport = serde_json::from_str(report_json).with_context(|| {
        format!(
            "failed to parse report JSON\nRaw (first 500): {}",
            &raw_report[..raw_report.len().min(500)]
        )
    })?;

    // ── Step 2: Generate WhatsApp summary ──
    debug!("generating WhatsApp summary via LLM");

    let summary_user_msg =
        format!("本次共分析了 {source_count} 個來源。以下是今日投資報告 JSON：\n\n{report_json}");

    let whatsapp_summary = provider
        .chat_with_system(
            Some(SUMMARY_SYSTEM_PROMPT),
            &summary_user_msg,
            model,
            temperature,
        )
        .await
        .context("LLM WhatsApp summary generation failed")?;

    // Truncate if exceeds WhatsApp limit.
    let whatsapp_summary = if whatsapp_summary.len() > MAX_WHATSAPP_CHARS {
        let mut truncated = whatsapp_summary[..MAX_WHATSAPP_CHARS].to_string();
        truncated.push_str("\n\n⚠️ (報告已截斷)");
        truncated
    } else {
        whatsapp_summary
    };

    Ok(ReportOutput {
        report,
        whatsapp_summary,
        report_date: date_str,
    })
}

/// Strip ```json ... ``` fences that LLMs sometimes add.
fn strip_code_fences(s: &str) -> &str {
    let trimmed = s.trim();
    let without_start = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let without_end = without_start
        .trim()
        .strip_suffix("```")
        .unwrap_or(without_start);
    without_end.trim()
}
