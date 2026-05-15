use serde::{Deserialize, Deserializer, Serialize};

fn null_as_empty<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Option::<String>::deserialize(d).map(|o| o.unwrap_or_default())
}

// ── Single-content analysis result ──────────────────────────────────────

/// Investment analysis of a single piece of content (podcast episode or YT video).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub source_type: String,
    pub source_name: String,
    pub title: String,
    pub url: String,
    pub publish_time: Option<String>,
    pub language: Option<String>,
    pub symbols: Vec<SymbolAnalysis>,
    pub macro_view: Option<MacroView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolAnalysis {
    #[serde(default, deserialize_with = "null_as_empty")]
    pub ticker: String,
    #[serde(default, deserialize_with = "null_as_empty")]
    pub company_name: String,
    #[serde(default)]
    pub sector: Option<String>,
    pub thesis_summary: String,
    #[serde(default)]
    pub bull_points: Vec<String>,
    #[serde(default)]
    pub bear_points: Vec<String>,
    #[serde(default = "default_horizon")]
    pub time_horizon: String,
    #[serde(default = "default_sentiment")]
    pub sentiment: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub risk_factors: Vec<String>,
    #[serde(default)]
    pub key_numbers: Vec<String>,
    #[serde(default)]
    pub actionable_signal: bool,
}

fn default_horizon() -> String {
    "mid".to_string()
}
fn default_sentiment() -> String {
    "neutral".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroView {
    pub market_trend: Option<String>,
    #[serde(default)]
    pub key_themes: Vec<String>,
    #[serde(default)]
    pub risks: Vec<String>,
}

// ── Aggregated multi-source view ────────────────────────────────────────

/// A single ticker aggregated across multiple content sources.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedSymbolView {
    pub ticker: String,
    pub company_name: String,
    pub sources: Vec<SourceView>,
    pub overall_sentiment: String,
    pub overall_confidence: f64,
    pub combined_bull_points: Vec<String>,
    pub combined_bear_points: Vec<String>,
    pub combined_risks: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceView {
    pub source_type: String,
    pub source_name: String,
    pub title: String,
    pub url: String,
    pub publish_time: Option<String>,
    pub thesis_summary: String,
    pub sentiment: String,
    pub confidence: f64,
}

// ── Final report ────────────────────────────────────────────────────────

/// The complete investment report sent to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvestmentReport {
    pub report_date: String,
    pub market_overview: String,
    #[serde(default)]
    pub key_themes: Vec<String>,
    pub symbols: Vec<ReportSymbol>,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSymbol {
    pub ticker: String,
    pub company_name: String,
    pub summary: String,
    pub overall_sentiment: String,
    pub overall_confidence: f64,
    #[serde(default = "default_horizon")]
    pub time_horizon: String,
    #[serde(default)]
    pub upside_drivers: Vec<String>,
    #[serde(default)]
    pub downside_risks: Vec<String>,
    #[serde(default)]
    pub positioning_suggestion: Option<String>,
}

/// Output of the final report generation step.
#[derive(Debug, Clone)]
pub struct ReportOutput {
    /// Structured JSON report.
    pub report: InvestmentReport,
    /// WhatsApp-friendly plain-text summary (Chinese, ≤ 4096 chars).
    pub whatsapp_summary: String,
    /// YYYY-MM-DD date the report covers in reporter timezone.
    pub report_date: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_result_roundtrip() {
        let json = r#"{
            "source_type": "podcast",
            "source_name": "Test Show",
            "title": "Episode 1",
            "url": "https://example.com/ep1",
            "publish_time": "2026-01-01T00:00:00Z",
            "language": "zh-TW",
            "symbols": [{
                "ticker": "AAPL",
                "company_name": "Apple Inc.",
                "thesis_summary": "Strong iPhone cycle",
                "bull_points": ["Services growth"],
                "bear_points": ["China risk"],
                "sentiment": "bullish",
                "confidence": 0.8,
                "actionable_signal": true
            }],
            "macro_view": {
                "market_trend": "Risk-on",
                "key_themes": ["AI capex"],
                "risks": ["Tariffs"]
            }
        }"#;

        let result: AnalysisResult = serde_json::from_str(json).unwrap();
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].ticker, "AAPL");
        assert_eq!(result.symbols[0].sentiment, "bullish");

        // Roundtrip
        let serialized = serde_json::to_string(&result).unwrap();
        let _: AnalysisResult = serde_json::from_str(&serialized).unwrap();
    }

    #[test]
    fn report_roundtrip() {
        let report = InvestmentReport {
            report_date: "2026-03-08".to_string(),
            market_overview: "Markets up".to_string(),
            key_themes: vec!["AI".to_string()],
            symbols: vec![ReportSymbol {
                ticker: "NVDA".to_string(),
                company_name: "NVIDIA".to_string(),
                summary: "Strong".to_string(),
                overall_sentiment: "bullish".to_string(),
                overall_confidence: 0.9,
                time_horizon: "mid".to_string(),
                upside_drivers: vec!["Data center".to_string()],
                downside_risks: vec!["Valuation".to_string()],
                positioning_suggestion: Some("Buy dips".to_string()),
            }],
            disclaimer: "Not financial advice.".to_string(),
        };

        let json = serde_json::to_string_pretty(&report).unwrap();
        let deser: InvestmentReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deser.symbols[0].ticker, "NVDA");
    }
}
