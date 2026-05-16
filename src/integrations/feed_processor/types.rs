use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Type of content source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Rss,
    Podcast,
    Youtube,
}

impl SourceType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rss => "rss",
            Self::Podcast => "podcast",
            Self::Youtube => "youtube",
        }
    }
}

impl std::fmt::Display for SourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A feed subscription entry stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedSubscription {
    pub id: String,
    pub feed_url: String,
    pub source_type: SourceType,
    pub source_name: String,
    pub last_fetch: Option<DateTime<Utc>>,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
}

/// A single item parsed from a feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedItem {
    /// Unique identifier within the feed (RSS guid / YouTube videoId).
    pub guid: String,
    pub title: String,
    pub link: String,
    pub pub_date: Option<DateTime<Utc>>,
    pub description: Option<String>,
    /// Audio enclosure URL for podcasts.
    pub enclosure_url: Option<String>,
    /// Duration in seconds (from itunes:duration or similar).
    pub duration_secs: Option<u64>,
}

/// Processing status for a feed item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessingStatus {
    New,
    Transcribed,
    Analyzed,
    Delivered,
    Failed,
}

impl ProcessingStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Transcribed => "transcribed",
            Self::Analyzed => "analyzed",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for ProcessingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Record of a processed feed item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessedItem {
    /// SHA-256 hash of `feed_url + guid`.
    pub id: String,
    pub feed_id: String,
    pub item_title: String,
    pub pub_date: Option<DateTime<Utc>>,
    pub processed_at: DateTime<Utc>,
    pub status: ProcessingStatus,
    pub notes: Option<String>,
}

/// Configuration for the investment reporter scheduler.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InvestReporterConfig {
    pub enabled: bool,
    /// Cron expression, e.g. `"0 9 * * *"`.
    pub schedule: String,
    /// IANA timezone, e.g. `"Asia/Taipei"`.
    pub timezone: String,
    /// RSS / Podcast feed URLs.
    pub feeds: Vec<FeedSourceConfig>,
    /// YouTube channel IDs or search queries.
    pub youtube_channels: Vec<String>,
    /// AssemblyAI transcription timeout in seconds.
    pub assemblyai_timeout_secs: u64,
    /// Apify transcript timeout in seconds.
    pub apify_timeout_secs: u64,
    /// LLM analysis timeout in seconds.
    pub llm_timeout_secs: u64,
}

impl Default for InvestReporterConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            schedule: "0 9 * * *".to_string(),
            timezone: "Asia/Taipei".to_string(),
            feeds: Vec::new(),
            youtube_channels: Vec::new(),
            assemblyai_timeout_secs: 600,
            apify_timeout_secs: 60,
            llm_timeout_secs: 120,
        }
    }
}

/// Per-feed source configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FeedSourceConfig {
    pub url: String,
    pub name: String,
    pub source_type: SourceType,
}
