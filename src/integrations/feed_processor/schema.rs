use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::Path;

/// Open (or create) the feed processor database and ensure schema is up to date.
pub fn open_db(workspace_dir: &Path) -> Result<Connection> {
    let db_dir = workspace_dir.join("feeds");
    std::fs::create_dir_all(&db_dir)
        .with_context(|| format!("Failed to create feeds directory: {}", db_dir.display()))?;

    let db_path = db_dir.join("subscriptions.db");
    let conn = Connection::open(&db_path)
        .with_context(|| format!("Failed to open feed DB: {}", db_path.display()))?;

    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous  = NORMAL;
         PRAGMA foreign_keys = ON;

         CREATE TABLE IF NOT EXISTS feed_subscriptions (
            id          TEXT PRIMARY KEY,
            feed_url    TEXT UNIQUE NOT NULL,
            source_type TEXT NOT NULL,
            source_name TEXT NOT NULL DEFAULT '',
            last_fetch  TEXT,
            enabled     INTEGER NOT NULL DEFAULT 1,
            created_at  TEXT NOT NULL
         );

         CREATE TABLE IF NOT EXISTS feed_items_processed (
            id           TEXT PRIMARY KEY,
            feed_id      TEXT NOT NULL,
            item_title   TEXT NOT NULL DEFAULT '',
            pub_date     TEXT,
            processed_at TEXT NOT NULL,
            status       TEXT NOT NULL DEFAULT 'new',
            notes        TEXT,
            FOREIGN KEY (feed_id) REFERENCES feed_subscriptions(id) ON DELETE CASCADE
         );
         CREATE INDEX IF NOT EXISTS idx_fip_feed_id
            ON feed_items_processed(feed_id);
         CREATE INDEX IF NOT EXISTS idx_fip_status
            ON feed_items_processed(status);
        ",
    )
    .context("Failed to initialize feed processor schema")?;

    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn schema_is_idempotent() {
        let tmp = TempDir::new().unwrap();
        let conn1 = open_db(tmp.path()).unwrap();
        drop(conn1);
        // Opening again should not fail (CREATE IF NOT EXISTS).
        let _conn2 = open_db(tmp.path()).unwrap();
    }
}
