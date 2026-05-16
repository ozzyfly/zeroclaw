use std::collections::HashMap;

use super::schemas::{AggregatedSymbolView, AnalysisResult, SourceView};

/// Aggregate multiple single-content analyses into per-ticker views.
///
/// - Groups `SymbolAnalysis` entries by `ticker`.
/// - Merges bull/bear points and risk factors (deduplicated).
/// - Computes a weighted overall sentiment and confidence.
/// - Also collects all `macro_view` data across sources.
pub fn aggregate_analyses(
    analyses: &[AnalysisResult],
) -> (Vec<AggregatedSymbolView>, AggregatedMacro) {
    let mut ticker_map: HashMap<String, TickerAccumulator> = HashMap::new();

    let mut macro_trends = Vec::new();
    let mut macro_themes = Vec::new();
    let mut macro_risks = Vec::new();

    for analysis in analyses {
        // Collect macro views.
        if let Some(mv) = &analysis.macro_view {
            if let Some(t) = &mv.market_trend {
                if !t.is_empty() {
                    macro_trends.push(t.clone());
                }
            }
            for theme in &mv.key_themes {
                push_unique(&mut macro_themes, theme);
            }
            for risk in &mv.risks {
                push_unique(&mut macro_risks, risk);
            }
        }

        // Collect per-ticker data.
        for sym in &analysis.symbols {
            // Skip symbols with empty ticker (LLM returned null)
            if sym.ticker.is_empty() {
                continue;
            }
            let acc = ticker_map
                .entry(sym.ticker.to_uppercase())
                .or_insert_with(|| TickerAccumulator {
                    ticker: sym.ticker.to_uppercase(),
                    company_name: sym.company_name.clone(),
                    sources: Vec::new(),
                    bull_points: Vec::new(),
                    bear_points: Vec::new(),
                    risks: Vec::new(),
                    sentiment_scores: Vec::new(),
                });

            acc.sources.push(SourceView {
                source_type: analysis.source_type.clone(),
                source_name: analysis.source_name.clone(),
                title: analysis.title.clone(),
                url: analysis.url.clone(),
                publish_time: analysis.publish_time.clone(),
                thesis_summary: sym.thesis_summary.clone(),
                sentiment: sym.sentiment.clone(),
                confidence: sym.confidence,
            });

            for p in &sym.bull_points {
                push_unique(&mut acc.bull_points, p);
            }
            for p in &sym.bear_points {
                push_unique(&mut acc.bear_points, p);
            }
            for r in &sym.risk_factors {
                push_unique(&mut acc.risks, r);
            }

            let score = sentiment_to_score(&sym.sentiment);
            acc.sentiment_scores.push((score, sym.confidence));
        }
    }

    // Build aggregated views.
    let mut aggregated: Vec<AggregatedSymbolView> = ticker_map
        .into_values()
        .map(|acc| {
            let (overall_sentiment, overall_confidence) =
                compute_weighted_sentiment(&acc.sentiment_scores);
            AggregatedSymbolView {
                ticker: acc.ticker,
                company_name: acc.company_name,
                sources: acc.sources,
                overall_sentiment,
                overall_confidence,
                combined_bull_points: acc.bull_points,
                combined_bear_points: acc.bear_points,
                combined_risks: acc.risks,
            }
        })
        .collect();

    // Sort by number of sources (most discussed first), then by confidence.
    aggregated.sort_by(|a, b| {
        b.sources.len().cmp(&a.sources.len()).then(
            b.overall_confidence
                .partial_cmp(&a.overall_confidence)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });

    let agg_macro = AggregatedMacro {
        trends: macro_trends,
        themes: macro_themes,
        risks: macro_risks,
    };

    (aggregated, agg_macro)
}

/// Collected macro-level views aggregated across all sources.
#[derive(Debug, Clone)]
pub struct AggregatedMacro {
    pub trends: Vec<String>,
    pub themes: Vec<String>,
    pub risks: Vec<String>,
}

// ── Helpers ─────────────────────────────────────────────────────────────

struct TickerAccumulator {
    ticker: String,
    company_name: String,
    sources: Vec<SourceView>,
    bull_points: Vec<String>,
    bear_points: Vec<String>,
    risks: Vec<String>,
    sentiment_scores: Vec<(f64, f64)>, // (score, confidence)
}

fn sentiment_to_score(sentiment: &str) -> f64 {
    match sentiment.to_lowercase().as_str() {
        "bullish" => 1.0,
        "bearish" => -1.0,
        _ => 0.0,
    }
}

/// Compute confidence-weighted average sentiment.
/// Returns (sentiment_label, average_confidence).
fn compute_weighted_sentiment(scores: &[(f64, f64)]) -> (String, f64) {
    if scores.is_empty() {
        return ("neutral".to_string(), 0.0);
    }

    let total_weight: f64 = scores.iter().map(|(_, c)| c).sum();
    if total_weight == 0.0 {
        return ("neutral".to_string(), 0.0);
    }

    let weighted_score: f64 = scores.iter().map(|(s, c)| s * c).sum::<f64>() / total_weight;
    let avg_confidence: f64 = total_weight / scores.len() as f64;

    let label = if weighted_score > 0.3 {
        "bullish"
    } else if weighted_score < -0.3 {
        "bearish"
    } else {
        "neutral"
    };

    (label.to_string(), (avg_confidence * 100.0).round() / 100.0)
}

/// Push to vec only if not already present (case-insensitive dedup).
fn push_unique(vec: &mut Vec<String>, item: &str) {
    let lower = item.to_lowercase();
    if !vec.iter().any(|existing| existing.to_lowercase() == lower) {
        vec.push(item.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::investment_reporter::schemas::{
        AnalysisResult, MacroView, SymbolAnalysis,
    };

    fn make_analysis(ticker: &str, sentiment: &str, confidence: f64) -> AnalysisResult {
        AnalysisResult {
            source_type: "podcast".to_string(),
            source_name: "Test Show".to_string(),
            title: format!("Episode about {ticker}"),
            url: "https://example.com".to_string(),
            publish_time: Some("2026-03-08T00:00:00Z".to_string()),
            language: Some("zh-TW".to_string()),
            symbols: vec![SymbolAnalysis {
                ticker: ticker.to_string(),
                company_name: format!("{ticker} Corp"),
                sector: None,
                thesis_summary: "Test thesis".to_string(),
                bull_points: vec!["Growth".to_string()],
                bear_points: vec!["Valuation".to_string()],
                time_horizon: "mid".to_string(),
                sentiment: sentiment.to_string(),
                confidence,
                risk_factors: vec!["Macro".to_string()],
                key_numbers: vec![],
                actionable_signal: true,
            }],
            macro_view: Some(MacroView {
                market_trend: Some("Risk-on".to_string()),
                key_themes: vec!["AI".to_string()],
                risks: vec!["Rates".to_string()],
            }),
        }
    }

    #[test]
    fn aggregate_groups_by_ticker() {
        let analyses = vec![
            make_analysis("AAPL", "bullish", 0.8),
            make_analysis("AAPL", "bullish", 0.9),
            make_analysis("NVDA", "bearish", 0.7),
        ];

        let (agg, macro_view) = aggregate_analyses(&analyses);

        assert_eq!(agg.len(), 2);

        let aapl = agg.iter().find(|a| a.ticker == "AAPL").unwrap();
        assert_eq!(aapl.sources.len(), 2);
        assert_eq!(aapl.overall_sentiment, "bullish");

        let nvda = agg.iter().find(|a| a.ticker == "NVDA").unwrap();
        assert_eq!(nvda.sources.len(), 1);
        assert_eq!(nvda.overall_sentiment, "bearish");

        assert!(!macro_view.themes.is_empty());
        assert!(!macro_view.risks.is_empty());
    }

    #[test]
    fn aggregate_empty_input() {
        let (agg, _) = aggregate_analyses(&[]);
        assert!(agg.is_empty());
    }

    #[test]
    fn dedup_points() {
        let mut vec = Vec::new();
        push_unique(&mut vec, "Growth");
        push_unique(&mut vec, "growth"); // same, different case
        push_unique(&mut vec, "New point");
        assert_eq!(vec.len(), 2);
    }
}
