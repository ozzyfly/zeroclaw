use anyhow::{Context, Result};
use tracing::debug;

use crate::providers::traits::Provider;

use super::schemas::AnalysisResult;

/// Maximum transcript length (in characters) sent to the LLM.
/// Longer texts are truncated to stay within token budgets.
const MAX_TRANSCRIPT_CHARS: usize = 12_000;

const SYSTEM_PROMPT: &str = r#"你是一位專業的投資分析師。你的任務是分析以下內容，找出其中提到的投資標的（股票/ETF/加密貨幣）、投資觀點、以及宏觀市場看法。

你必須以 JSON 格式回答，嚴格按照以下 schema：

{
  "source_type": "podcast 或 youtube",
  "source_name": "來源名稱",
  "title": "標題",
  "url": "原始連結",
  "publish_time": "ISO8601 時間字串或 null",
  "language": "內容語言",
  "symbols": [
    {
      "ticker": "股票代碼",
      "company_name": "公司名稱",
      "sector": "產業（可為 null）",
      "thesis_summary": "投資論點摘要",
      "bull_points": ["看多理由"],
      "bear_points": ["看空理由"],
      "time_horizon": "short | mid | long",
      "sentiment": "bullish | bearish | neutral",
      "confidence": 0.0 到 1.0 的數值,
      "risk_factors": ["風險因素"],
      "key_numbers": ["關鍵數字，例如目標價、EPS等"],
      "actionable_signal": true 或 false
    }
  ],
  "macro_view": {
    "market_trend": "市場趨勢描述",
    "key_themes": ["關鍵主題"],
    "risks": ["宏觀風險"]
  }
}

重要規則：
1. 只輸出 JSON，不要加任何其他文字或 markdown 標記。
2. 如果內容沒有提到任何投資標的，symbols 陣列留空 []。
3. confidence 是 0.0-1.0 的數值，代表你對該分析的信心程度。
4. 保持客觀，忠實反映內容觀點，不加入你自己的推測。"#;

/// Analyze a single piece of content (podcast or YouTube) for investment signals.
pub async fn analyze_single_content(
    provider: &dyn Provider,
    model: &str,
    temperature: f64,
    source_type: &str,
    source_name: &str,
    title: &str,
    url: &str,
    publish_time: Option<&str>,
    transcript: &str,
) -> Result<AnalysisResult> {
    let truncated = if transcript.len() > MAX_TRANSCRIPT_CHARS {
        &transcript[..MAX_TRANSCRIPT_CHARS]
    } else {
        transcript
    };

    let user_message = format!(
        "來源類型: {source_type}\n\
         來源名稱: {source_name}\n\
         標題: {title}\n\
         連結: {url}\n\
         發布時間: {}\n\n\
         ---\n\n\
         {truncated}",
        publish_time.unwrap_or("未知"),
    );

    debug!(
        source_name,
        title,
        transcript_len = transcript.len(),
        "analyzing single content"
    );

    let raw_response = provider
        .chat_with_system(Some(SYSTEM_PROMPT), &user_message, model, temperature)
        .await
        .with_context(|| format!("LLM analysis failed for: {title}"))?;

    // Strip markdown code fences if the LLM wrapped the JSON.
    let json_str = strip_code_fences(&raw_response);

    let result: AnalysisResult = serde_json::from_str(json_str).with_context(|| {
        format!(
            "failed to parse LLM JSON for: {title}\nRaw response (first 500 chars): {}",
            &raw_response[..raw_response.len().min(500)]
        )
    })?;

    debug!(title, symbols = result.symbols.len(), "analysis complete");

    Ok(result)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_code_fences_plain_json() {
        let input = r#"{"symbols": []}"#;
        assert_eq!(strip_code_fences(input), input);
    }

    #[test]
    fn strip_code_fences_wrapped() {
        let input = "```json\n{\"symbols\": []}\n```";
        assert_eq!(strip_code_fences(input), "{\"symbols\": []}");
    }

    #[test]
    fn strip_code_fences_no_lang() {
        let input = "```\n{\"symbols\": []}\n```";
        assert_eq!(strip_code_fences(input), "{\"symbols\": []}");
    }
}
