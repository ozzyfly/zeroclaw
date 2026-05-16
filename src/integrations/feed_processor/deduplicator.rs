use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use super::types::ProcessingStatus;

/// Compute the deduplication key: `SHA-256(feed_url || "\0" || item_guid)`.
pub fn dedup_id(feed_url: &str, item_guid: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(feed_url.as_bytes());
    hasher.update(b"\0");
    hasher.update(item_guid.as_bytes());
    hex::encode(hasher.finalize())
}

/// Returns `true` if this item has already been processed (any status).
pub fn has_processed(conn: &Connection, feed_url: &str, item_guid: &str) -> Result<bool> {
    let id = dedup_id(feed_url, item_guid);
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM feed_items_processed WHERE id = ?1",
            [&id],
            |row| row.get(0),
        )
        .context("dedup lookup failed")?;
    Ok(count > 0)
}

/// Mark an item as processed with the given status.
pub fn mark_processed(
    conn: &Connection,
    feed_url: &str,
    feed_id: &str,
    item_guid: &str,
    item_title: &str,
    pub_date: Option<&str>,
    status: ProcessingStatus,
    notes: Option<&str>,
) -> Result<()> {
    let id = dedup_id(feed_url, item_guid);
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT OR REPLACE INTO feed_items_processed
            (id, feed_id, item_title, pub_date, processed_at, status, notes)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            feed_id,
            item_title,
            pub_date,
            now,
            status.as_str(),
            notes
        ],
    )
    .context("failed to mark item processed")?;
    Ok(())
}

/// Update the status of an already-tracked item.
pub fn update_status(
    conn: &Connection,
    feed_url: &str,
    item_guid: &str,
    status: ProcessingStatus,
) -> Result<()> {
    let id = dedup_id(feed_url, item_guid);
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE feed_items_processed SET status = ?1, processed_at = ?2 WHERE id = ?3",
        rusqlite::params![status.as_str(), now, id],
    )
    .context("failed to update item status")?;
    Ok(())
}

/// Prune processed items older than `days` to prevent unbounded growth.
pub fn prune_old_items(conn: &Connection, days: u32) -> Result<usize> {
    let cutoff = Utc::now() - chrono::Duration::days(i64::from(days));
    let deleted = conn
        .execute(
            "DELETE FROM feed_items_processed WHERE processed_at < ?1",
            [cutoff.to_rfc3339()],
        )
        .context("failed to prune old items")?;
    Ok(deleted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integrations::feed_processor::schema;
    use tempfile::TempDir;

    #[test]
    fn dedup_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let conn = schema::open_db(tmp.path()).unwrap();

        // Insert a subscription first (FK constraint).
        conn.execute(
            "INSERT INTO feed_subscriptions (id, feed_url, source_type, created_at)
             VALUES ('sub1', 'https://example.com/rss', 'rss', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let feed_url = "https://example.com/rss";
        let guid = "ep-42";

        assert!(!has_processed(&conn, feed_url, guid).unwrap());

        mark_processed(
            &conn,
            feed_url,
            "sub1",
            guid,
            "Episode 42",
            None,
            ProcessingStatus::New,
            None,
        )
        .unwrap();

        assert!(has_processed(&conn, feed_url, guid).unwrap());

        update_status(&conn, feed_url, guid, ProcessingStatus::Delivered).unwrap();

        let status: String = conn
            .query_row(
                "SELECT status FROM feed_items_processed WHERE id = ?1",
                [dedup_id(feed_url, guid)],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "delivered");
    }
}
