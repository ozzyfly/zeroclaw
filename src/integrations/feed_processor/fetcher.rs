use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use std::time::Duration;

use super::parser;
use super::types::{FeedItem, SourceType};

const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024; // 10 MB

/// Fetch a feed URL and return parsed items.
///
/// `since` filters out items older than the given timestamp (if available in
/// the feed).
pub async fn fetch_feed_items(
    url: &str,
    source_type: SourceType,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<FeedItem>> {
    let client = reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .context("failed to build HTTP client")?;

    let resp = client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (compatible; zeroclaw/0.1; +https://github.com/zeroclaw-labs/zeroclaw)",
        )
        .send()
        .await
        .with_context(|| format!("failed to fetch feed: {url}"))?;

    let status = resp.status();
    if !status.is_success() {
        anyhow::bail!("feed returned HTTP {status} for {url}");
    }

    let bytes = resp
        .bytes()
        .await
        .with_context(|| format!("failed to read feed body: {url}"))?;

    if bytes.len() > MAX_RESPONSE_BYTES {
        anyhow::bail!(
            "feed response too large ({} bytes, max {MAX_RESPONSE_BYTES}): {url}",
            bytes.len()
        );
    }

    let items = parser::parse_feed(&bytes, source_type)?;

    // Filter by publish date if requested.
    let items = match since {
        Some(cutoff) => items
            .into_iter()
            .filter(|item| item.pub_date.map_or(true, |d| d > cutoff))
            .collect(),
        None => items,
    };

    Ok(items)
}
