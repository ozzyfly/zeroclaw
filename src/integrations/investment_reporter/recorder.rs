use anyhow::{Context, Result};
use chrono::Utc;
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use tracing::debug;

/// Open (or create) the investment reports database and ensure schema is up to date.
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

         CREATE TABLE IF NOT EXISTS investment_reports (
            id                      TEXT PRIMARY KEY,
            report_date             TEXT NOT NULL,
            json_report             TEXT NOT NULL,
            whatsapp_summary        TEXT NOT NULL,
            whatsapp_sent_at        TEXT,
            whatsapp_delivery_status TEXT NOT NULL DEFAULT 'pending',
            sources_count           INTEGER NOT NULL DEFAULT 0,
            symbols_count           INTEGER NOT NULL DEFAULT 0,
            error_log               TEXT,
            created_at              TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_ir_report_date
            ON investment_reports(report_date);
        ",
    )
    .context("Failed to initialize investment_reports schema")?;

    Ok(conn)
}

/// Delivery status for a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStatus {
    Pending,
    Delivered,
    Failed,
}

impl DeliveryStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
        }
    }
}

/// Save a generated report to the database.
pub fn save_report(
    conn: &Connection,
    report_date: &str,
    json_report: &str,
    whatsapp_summary: &str,
    sources_count: usize,
    symbols_count: usize,
) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO investment_reports
            (id, report_date, json_report, whatsapp_summary, sources_count, symbols_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![id, report_date, json_report, whatsapp_summary, sources_count, symbols_count, now],
    )
    .context("failed to insert investment report")?;

    debug!("Saved investment report {id} for date {report_date}");
    Ok(id)
}

/// Update delivery status after WhatsApp send attempt.
pub fn update_delivery_status(
    conn: &Connection,
    report_id: &str,
    status: DeliveryStatus,
    error: Option<&str>,
) -> Result<()> {
    let now = if status == DeliveryStatus::Delivered || status == DeliveryStatus::Failed {
        Some(Utc::now().to_rfc3339())
    } else {
        None
    };

    conn.execute(
        "UPDATE investment_reports
         SET whatsapp_delivery_status = ?1,
             whatsapp_sent_at = COALESCE(?2, whatsapp_sent_at),
             error_log = ?3
         WHERE id = ?4",
        rusqlite::params![status.as_str(), now, error, report_id],
    )
    .context("failed to update report delivery status")?;

    Ok(())
}

/// A stored report summary (for list display).
#[derive(Debug, Clone)]
pub struct ReportSummary {
    pub id: String,
    pub report_date: String,
    pub sources_count: i64,
    pub symbols_count: i64,
    pub delivery_status: String,
    pub created_at: String,
}

/// A stored report with delivery-ready summary content.
#[derive(Debug, Clone)]
pub struct StoredReport {
    pub id: String,
    pub report_date: String,
    pub json_report: String,
    pub whatsapp_summary: String,
    pub delivery_status: String,
    pub created_at: String,
}

/// List recent reports.
pub fn list_reports(conn: &Connection, limit: usize) -> Result<Vec<ReportSummary>> {
    let mut stmt = conn.prepare(
        "SELECT id, report_date, sources_count, symbols_count, whatsapp_delivery_status, created_at
         FROM investment_reports
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;

    let rows = stmt
        .query_map(rusqlite::params![limit], |row| {
            Ok(ReportSummary {
                id: row.get(0)?,
                report_date: row.get(1)?,
                sources_count: row.get(2)?,
                symbols_count: row.get(3)?,
                delivery_status: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    Ok(rows)
}

/// Get the full JSON report by id.
pub fn get_report_json(conn: &Connection, report_id: &str) -> Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT json_report FROM investment_reports WHERE id = ?1")?;

    let result = stmt
        .query_row(rusqlite::params![report_id], |row| row.get(0))
        .optional()
        .context("failed to query report")?;

    Ok(result)
}

fn query_stored_report(
    conn: &Connection,
    sql: &str,
    params: impl rusqlite::Params,
) -> Result<Option<StoredReport>> {
    let mut stmt = conn.prepare(sql)?;
    let report = stmt
        .query_row(params, |row| {
            Ok(StoredReport {
                id: row.get(0)?,
                report_date: row.get(1)?,
                json_report: row.get(2)?,
                whatsapp_summary: row.get(3)?,
                delivery_status: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .optional()
        .context("failed to query stored report")?;

    Ok(report)
}

/// Get the latest stored report for a specific date.
pub fn get_report_by_date(conn: &Connection, report_date: &str) -> Result<Option<StoredReport>> {
    query_stored_report(
        conn,
        "SELECT id, report_date, json_report, whatsapp_summary, whatsapp_delivery_status, created_at
         FROM investment_reports
         WHERE report_date = ?1
         ORDER BY created_at DESC
         LIMIT 1",
        rusqlite::params![report_date],
    )
}

/// Get the most recently created stored report.
pub fn get_latest_report(conn: &Connection) -> Result<Option<StoredReport>> {
    query_stored_report(
        conn,
        "SELECT id, report_date, json_report, whatsapp_summary, whatsapp_delivery_status, created_at
         FROM investment_reports
         ORDER BY created_at DESC
         LIMIT 1",
        [],
    )
}

/// Save the full JSON report to a file in the workspace.
pub fn save_report_file(
    workspace_dir: &Path,
    report_date: &str,
    json_report: &str,
) -> Result<std::path::PathBuf> {
    let reports_dir = workspace_dir.join("reports");
    std::fs::create_dir_all(&reports_dir).with_context(|| {
        format!(
            "Failed to create reports directory: {}",
            reports_dir.display()
        )
    })?;

    let filename = format!("投資_{report_date}.json");
    let file_path = reports_dir.join(&filename);
    std::fs::write(&file_path, json_report)
        .with_context(|| format!("Failed to write report file: {}", file_path.display()))?;

    debug!("Saved report file: {}", file_path.display());
    Ok(file_path)
}

/// Check if a report already exists for the given date.
pub fn has_report_for_date(conn: &Connection, report_date: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM investment_reports WHERE report_date = ?1",
        rusqlite::params![report_date],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn report_save_and_list() {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();

        let id = save_report(
            &conn,
            "2026-03-08",
            r#"{"report_date":"2026-03-08"}"#,
            "📊 Test summary",
            5,
            3,
        )
        .unwrap();

        assert!(!id.is_empty());
        assert!(has_report_for_date(&conn, "2026-03-08").unwrap());
        assert!(!has_report_for_date(&conn, "2026-03-09").unwrap());

        let reports = list_reports(&conn, 10).unwrap();
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0].report_date, "2026-03-08");
        assert_eq!(reports[0].sources_count, 5);
        assert_eq!(reports[0].symbols_count, 3);
        assert_eq!(reports[0].delivery_status, "pending");

        // Update delivery status
        update_delivery_status(&conn, &id, DeliveryStatus::Delivered, None).unwrap();
        let reports = list_reports(&conn, 10).unwrap();
        assert_eq!(reports[0].delivery_status, "delivered");

        // Get JSON
        let json = get_report_json(&conn, &id).unwrap();
        assert_eq!(json.unwrap(), r#"{"report_date":"2026-03-08"}"#);
    }

    #[test]
    fn report_file_save() {
        let tmp = TempDir::new().unwrap();
        let path = save_report_file(tmp.path(), "2026-03-08", "{}").unwrap();
        assert!(path.exists());
        assert!(path.to_string_lossy().contains("投資_2026-03-08.json"));
    }

    #[test]
    fn delivery_status_failed_with_error() {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();

        let id = save_report(&conn, "2026-03-08", "{}", "summary", 1, 1).unwrap();
        update_delivery_status(&conn, &id, DeliveryStatus::Failed, Some("timeout")).unwrap();

        let reports = list_reports(&conn, 10).unwrap();
        assert_eq!(reports[0].delivery_status, "failed");
    }

    #[test]
    fn get_report_by_date_returns_latest_row() {
        let tmp = TempDir::new().unwrap();
        let conn = open_db(tmp.path()).unwrap();

        let first_id = save_report(&conn, "2026-03-08", "{\"v\":1}", "summary-1", 1, 1).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let second_id = save_report(&conn, "2026-03-08", "{\"v\":2}", "summary-2", 2, 2).unwrap();

        let stored = get_report_by_date(&conn, "2026-03-08").unwrap().unwrap();
        assert_eq!(stored.id, second_id);
        assert_ne!(stored.id, first_id);
        assert_eq!(stored.whatsapp_summary, "summary-2");
        assert_eq!(stored.json_report, "{\"v\":2}");
    }
}
