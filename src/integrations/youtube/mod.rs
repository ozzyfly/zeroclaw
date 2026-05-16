pub mod transcript;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const YOUTUBE_API_BASE: &str = "https://www.googleapis.com/youtube/v3";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// A YouTube video returned from search + details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YouTubeVideo {
    pub video_id: String,
    pub title: String,
    pub channel_title: String,
    pub published_at: DateTime<Utc>,
    pub description: String,
    pub duration_iso: Option<String>,
}

// ── YouTube Data API response shapes ────────────────────────────────────

#[derive(Deserialize)]
struct SearchResponse {
    items: Option<Vec<SearchItem>>,
}

#[derive(Deserialize)]
struct SearchItem {
    id: SearchItemId,
    snippet: Option<SearchSnippet>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchItemId {
    video_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchSnippet {
    title: Option<String>,
    channel_title: Option<String>,
    published_at: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct VideosResponse {
    items: Option<Vec<VideoItem>>,
}

#[derive(Deserialize)]
struct VideoItem {
    id: String,
    snippet: Option<VideoSnippet>,
    #[serde(rename = "contentDetails")]
    content_details: Option<ContentDetails>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VideoSnippet {
    title: Option<String>,
    channel_title: Option<String>,
    published_at: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize)]
struct ContentDetails {
    duration: Option<String>,
}

// ── Public API ──────────────────────────────────────────────────────────

/// Search a YouTube channel for recent videos published after `since`.
///
/// `channel_id` can be either a channel ID (`UCxxx`) or a free-text search query.
pub async fn search_recent_videos(
    api_key: &str,
    channel_id: &str,
    since: Option<DateTime<Utc>>,
    max_results: u32,
) -> Result<Vec<YouTubeVideo>> {
    let client = build_client()?;

    let mut url = format!(
        "{YOUTUBE_API_BASE}/search?part=snippet&type=video&order=date&maxResults={max_results}&key={api_key}"
    );

    // Determine if it looks like a channel ID or a search query.
    if channel_id.starts_with("UC") && channel_id.len() == 24 {
        url.push_str(&format!("&channelId={channel_id}"));
    } else {
        url.push_str(&format!("&q={}", urlencoding::encode(channel_id)));
    }

    if let Some(dt) = since {
        // YouTube API requires RFC 3339 with "Z" suffix; the default to_rfc3339()
        // produces "+00:00" which breaks when embedded in a URL without encoding.
        url.push_str(&format!(
            "&publishedAfter={}",
            dt.format("%Y-%m-%dT%H:%M:%SZ")
        ));
    }

    let resp: SearchResponse = client
        .get(&url)
        .send()
        .await
        .context("YouTube search request failed")?
        .error_for_status()
        .context("YouTube search returned error status")?
        .json()
        .await
        .context("failed to parse YouTube search response")?;

    let video_ids: Vec<String> = resp
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| item.id.video_id)
        .collect();

    if video_ids.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch full details (title, duration, etc.) for found videos.
    fetch_video_details(api_key, &video_ids).await
}

/// Fetch detailed metadata for a list of video IDs.
pub async fn fetch_video_details(api_key: &str, video_ids: &[String]) -> Result<Vec<YouTubeVideo>> {
    if video_ids.is_empty() {
        return Ok(Vec::new());
    }

    let client = build_client()?;
    let ids_param = video_ids.join(",");
    let url = format!(
        "{YOUTUBE_API_BASE}/videos?part=snippet,contentDetails&id={ids_param}&key={api_key}"
    );

    let resp: VideosResponse = client
        .get(&url)
        .send()
        .await
        .context("YouTube videos request failed")?
        .error_for_status()
        .context("YouTube videos returned error status")?
        .json()
        .await
        .context("failed to parse YouTube videos response")?;

    let videos: Vec<YouTubeVideo> = resp
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| {
            let snippet = v.snippet?;
            let published_at = snippet
                .published_at
                .as_deref()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))?;

            Some(YouTubeVideo {
                video_id: v.id,
                title: snippet.title.unwrap_or_default(),
                channel_title: snippet.channel_title.unwrap_or_default(),
                published_at,
                description: snippet.description.unwrap_or_default(),
                duration_iso: v.content_details.and_then(|cd| cd.duration),
            })
        })
        .collect();

    Ok(videos)
}

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(Duration::from_secs(10))
        .build()
        .context("failed to build HTTP client")
}
