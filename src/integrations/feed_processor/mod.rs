pub mod deduplicator;
pub mod fetcher;
pub mod parser;
pub mod schema;
pub mod types;

use anyhow::{Context, Result};
use chrono::Utc;
use tracing::info;

use crate::config::Config;
use types::{FeedItem, FeedSourceConfig, FeedSubscription, ProcessingStatus, SourceType};

/// Register a feed subscription in the database.
pub fn subscribe(config: &Config, src: &FeedSourceConfig) -> Result<FeedSubscription> {
    let conn = schema::open_db(&config.workspace_dir)?;

    // Check if already subscribed to this URL.
    let existing: Option<(String, String, Option<String>)> = conn
        .query_row(
            "SELECT id, source_name, last_fetch FROM feed_subscriptions WHERE feed_url = ?1",
            rusqlite::params![src.url],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .ok();

    let (id, last_fetch_str) = if let Some((existing_id, _, lf)) = existing {
        (existing_id, lf)
    } else {
        let new_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO feed_subscriptions
                (id, feed_url, source_type, source_name, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            rusqlite::params![new_id, src.url, src.source_type.as_str(), src.name, now],
        )
        .context("failed to insert subscription")?;
        (new_id, None)
    };

    let last_fetch = last_fetch_str
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&Utc));

    Ok(FeedSubscription {
        id,
        feed_url: src.url.clone(),
        source_type: src.source_type,
        source_name: src.name.clone(),
        last_fetch,
        enabled: true,
        created_at: Utc::now(),
    })
}

/// List all enabled subscriptions.
pub fn list_subscriptions(config: &Config) -> Result<Vec<FeedSubscription>> {
    let conn = schema::open_db(&config.workspace_dir)?;
    let mut stmt = conn.prepare(
        "SELECT id, feed_url, source_type, source_name, last_fetch, enabled, created_at
         FROM feed_subscriptions WHERE enabled = 1",
    )?;

    let subs = stmt
        .query_map([], |row| {
            let st_str: String = row.get(2)?;
            let source_type = match st_str.as_str() {
                "podcast" => SourceType::Podcast,
                "youtube" => SourceType::Youtube,
                _ => SourceType::Rss,
            };
            Ok(FeedSubscription {
                id: row.get(0)?,
                feed_url: row.get(1)?,
                source_type,
                source_name: row.get(3)?,
                last_fetch: row
                    .get::<_, Option<String>>(4)?
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                enabled: row.get::<_, i32>(5)? != 0,
                created_at: row
                    .get::<_, String>(6)
                    .ok()
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(subs)
}

/// Fetch new items for a single subscription, filtering out already-processed
/// ones.  Returns only the new `FeedItem`s.
pub async fn fetch_new_items(config: &Config, sub: &FeedSubscription) -> Result<Vec<FeedItem>> {
    let items = fetcher::fetch_feed_items(&sub.feed_url, sub.source_type, sub.last_fetch).await?;

    let conn = schema::open_db(&config.workspace_dir)?;
    let total_count = items.len();
    let new_items: Vec<FeedItem> = items
        .into_iter()
        .filter(|item| {
            !deduplicator::has_processed(&conn, &sub.feed_url, &item.guid).unwrap_or(true)
        })
        .collect();

    // Update last_fetch timestamp.
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE feed_subscriptions SET last_fetch = ?1 WHERE id = ?2",
        rusqlite::params![now, sub.id],
    )?;

    info!(
        feed = %sub.source_name,
        total = total_count,
        new = new_items.len(),
        "feed fetch complete"
    );

    Ok(new_items)
}

/// Mark a single feed item as processed.  Call this only after the item has
/// been successfully transcribed and added to analysis — not before.
pub fn mark_item_processed(config: &Config, sub: &FeedSubscription, item: &FeedItem) -> Result<()> {
    let conn = schema::open_db(&config.workspace_dir)?;
    deduplicator::mark_processed(
        &conn,
        &sub.feed_url,
        &sub.id,
        &item.guid,
        &item.title,
        item.pub_date.as_ref().map(|d| d.to_rfc3339()).as_deref(),
        ProcessingStatus::New,
        None,
    )
}
