use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Fetch the transcript (subtitles) of a YouTube video via Apify.
///
/// Uses the `pintostudio~youtube-transcript-scraper` actor in synchronous mode
/// and returns the full text (segments joined).
pub async fn fetch_transcript(apify_token: &str, video_id: &str) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .context("failed to build HTTP client")?;

    let url = format!(
        "https://api.apify.com/v2/acts/pintostudio~youtube-transcript-scraper/run-sync-get-dataset-items?token={apify_token}"
    );

    let body = serde_json::json!({
        "urls": [format!("https://www.youtube.com/watch?v={video_id}")],
        "outputFormat": "singleStringText",
    });

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("Apify transcript request failed for {video_id}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        anyhow::bail!("Apify returned HTTP {status} for video {video_id}: {text}");
    }

    let items: Vec<TranscriptItem> = resp
        .json()
        .await
        .with_context(|| format!("failed to parse Apify response for {video_id}"))?;

    // Concatenate all transcript text segments.
    let full_text: String = items
        .into_iter()
        .filter_map(|item| item.text.or(item.transcript_text))
        .collect::<Vec<_>>()
        .join(" ");

    if full_text.trim().is_empty() {
        tracing::warn!(video_id, "Apify returned empty transcript");
    }

    Ok(full_text)
}

/// Apify response item — the actor may return different field names depending
/// on version, so we accept both `text` and `transcriptText`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptItem {
    text: Option<String>,
    transcript_text: Option<String>,
}
