// ABOUTME: DuckDB query layer for document CRUD and analytics
// ABOUTME: Provides upsert, cache, search, filter, and statistics operations

use chrono::{DateTime, Utc};
use duckdb::{params, Connection};

use crate::model::DocumentMetadata;
use crate::Result;

/// A row from the documents table for display purposes.
#[derive(Debug, Clone)]
pub struct DocumentRow {
    pub doc_id: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub duration_seconds: Option<i64>,
    pub filename: Option<String>,
}

/// Cached sync state for a document.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub filename: String,
    pub updated_at: DateTime<Utc>,
}

/// Aggregate statistics across all meetings.
#[derive(Debug, Clone)]
pub struct Stats {
    pub total_meetings: i64,
    pub total_duration_seconds: i64,
    pub unique_attendees: i64,
    pub meetings_per_week: f64,
}

/// Parse a timestamp string stored in the database back to DateTime<Utc>.
fn parse_ts(s: &str) -> DateTime<Utc> {
    s.parse::<DateTime<Utc>>()
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
}

/// Format a DateTime<Utc> as an RFC3339 string for storage.
fn fmt_ts(dt: &DateTime<Utc>) -> String {
    dt.to_rfc3339()
}

/// Parse a DocumentRow from a DuckDB row (columns: doc_id, title, created_at, duration_seconds, filename).
fn parse_document_row(row: &duckdb::Row<'_>) -> std::result::Result<DocumentRow, duckdb::Error> {
    let created_at_opt: Option<String> = row.get(2)?;
    Ok(DocumentRow {
        doc_id: row.get(0)?,
        title: row.get(1)?,
        created_at: created_at_opt
            .map(|s| parse_ts(&s))
            .unwrap_or(chrono::DateTime::UNIX_EPOCH),
        duration_seconds: row.get(3)?,
        filename: row.get(4)?,
    })
}

/// Upsert a document and its related attendees, labels, and participants.
/// Uses a transaction to keep everything consistent.
pub fn upsert_document(
    conn: &Connection,
    meta: &DocumentMetadata,
    doc_id: &str,
    filename: &str,
    notes: Option<&str>,
    summary_text: Option<&str>,
) -> Result<()> {
    conn.execute_batch("BEGIN TRANSACTION")?;

    let result = upsert_document_inner(conn, meta, doc_id, filename, notes, summary_text);

    if result.is_err() {
        let _ = conn.execute_batch("ROLLBACK");
        return result;
    }

    conn.execute_batch("COMMIT")?;
    Ok(())
}

/// Inner implementation for upsert_document, called within a transaction.
/// Separated so the caller can ROLLBACK on error.
fn upsert_document_inner(
    conn: &Connection,
    meta: &DocumentMetadata,
    doc_id: &str,
    filename: &str,
    notes: Option<&str>,
    summary_text: Option<&str>,
) -> Result<()> {
    let created_at = meta.created_at.as_ref().map(fmt_ts);
    let updated_at = meta.updated_at.as_ref().map(fmt_ts);
    let synced_at = fmt_ts(&Utc::now());

    // Delete child rows first so the document upsert doesn't hit FK violations
    // (DuckDB's ON CONFLICT may internally delete+insert, triggering FK checks)
    // Only delete attendees if the API provided them (None means absent, not empty)
    if meta.attendees.is_some() {
        conn.execute("DELETE FROM attendees WHERE doc_id = ?", params![doc_id])?;
    }
    conn.execute("DELETE FROM labels WHERE doc_id = ?", params![doc_id])?;
    conn.execute("DELETE FROM participants WHERE doc_id = ?", params![doc_id])?;

    // Upsert the document row
    conn.execute(
        "INSERT INTO documents (doc_id, title, created_at, updated_at, duration_seconds, filename, synced_at, notes, summary_text)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (doc_id) DO UPDATE SET
           title = excluded.title,
           updated_at = excluded.updated_at,
           duration_seconds = excluded.duration_seconds,
           filename = excluded.filename,
           synced_at = excluded.synced_at,
           notes = excluded.notes,
           summary_text = excluded.summary_text",
        params![
            doc_id,
            meta.title.as_deref(),
            created_at,
            updated_at,
            meta.duration_seconds.map(|d| d as i64),
            filename,
            synced_at,
            notes,
            summary_text,
        ],
    )?;
    if let Some(attendees) = &meta.attendees {
        for att in attendees.iter().filter(|a| a.is_person()) {
            let is_creator = meta
                .creator
                .as_ref()
                .and_then(|c| c.email.as_ref())
                .zip(att.email.as_ref())
                .map(|(ce, ae)| ce == ae)
                .unwrap_or(false);

            let (emp_title, company, linkedin) = att
                .details
                .as_ref()
                .map(|d| {
                    let emp = d
                        .person
                        .as_ref()
                        .and_then(|p| p.employment.as_ref())
                        .and_then(|e| e.title.clone());
                    let co = d.company.as_ref().and_then(|c| c.name.clone());
                    let li = d
                        .person
                        .as_ref()
                        .and_then(|p| p.linkedin.as_ref())
                        .and_then(|l| l.handle.clone());
                    (emp, co, li)
                })
                .unwrap_or((None, None, None));

            conn.execute(
                "INSERT INTO attendees (doc_id, name, email, employment_title, company_name, linkedin_handle, is_creator)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
                params![
                    doc_id,
                    att.display_name().as_deref(),
                    att.email.as_deref(),
                    emp_title.as_deref(),
                    company.as_deref(),
                    linkedin.as_deref(),
                    is_creator,
                ],
            )?;
        }
    }

    // Insert labels
    for label in &meta.labels {
        conn.execute(
            "INSERT INTO labels (doc_id, label) VALUES (?, ?)",
            params![doc_id, label],
        )?;
    }

    // Insert participants
    for name in &meta.participants {
        conn.execute(
            "INSERT INTO participants (doc_id, name) VALUES (?, ?)",
            params![doc_id, name],
        )?;
    }

    Ok(())
}

/// Check if a document exists in the DB but has no summary_text.
/// Returns true if the doc is missing or has NULL summary_text.
pub fn doc_missing_summary(conn: &Connection, doc_id: &str) -> Result<bool> {
    let mut stmt = conn.prepare("SELECT summary_text IS NULL FROM documents WHERE doc_id = ?")?;
    let mut rows = stmt.query(params![doc_id])?;
    if let Some(row) = rows.next()? {
        let is_null: bool = row.get(0)?;
        Ok(is_null)
    } else {
        // Doc not in DB at all — treat as missing
        Ok(true)
    }
}

/// Get a sync cache entry for a document.
pub fn get_cache_entry(conn: &Connection, doc_id: &str) -> Result<Option<CacheEntry>> {
    let mut stmt = conn.prepare("SELECT filename, updated_at FROM sync_cache WHERE doc_id = ?")?;
    let mut rows = stmt.query(params![doc_id])?;
    if let Some(row) = rows.next()? {
        let updated_at_str: String = row.get(1)?;
        Ok(Some(CacheEntry {
            filename: row.get(0)?,
            updated_at: parse_ts(&updated_at_str),
        }))
    } else {
        Ok(None)
    }
}

/// Upsert a sync cache entry.
pub fn upsert_cache_entry(
    conn: &Connection,
    doc_id: &str,
    filename: &str,
    updated_at: &DateTime<Utc>,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sync_cache (doc_id, filename, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT (doc_id) DO UPDATE SET
           filename = excluded.filename,
           updated_at = excluded.updated_at",
        params![doc_id, filename, fmt_ts(updated_at)],
    )?;
    Ok(())
}

/// List all documents, sorted by created_at descending.
pub fn list_documents(conn: &Connection) -> Result<Vec<DocumentRow>> {
    let mut stmt = conn.prepare(
        "SELECT doc_id, title, created_at, duration_seconds, filename
         FROM documents ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], parse_document_row)?;
    let mut docs = Vec::new();
    for row in rows {
        docs.push(row?);
    }
    Ok(docs)
}

/// Detail view of a single document, including summary and attendees.
#[derive(Debug, Clone)]
pub struct DocumentDetail {
    pub doc_id: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub duration_seconds: Option<i64>,
    pub filename: Option<String>,
    pub summary_text: Option<String>,
    pub notes: Option<String>,
    pub attendees: Vec<String>,
    pub labels: Vec<String>,
}

/// Get a single document by ID with summary, notes, attendees, and labels.
pub fn get_document(conn: &Connection, doc_id: &str) -> Result<Option<DocumentDetail>> {
    let mut stmt = conn.prepare(
        "SELECT doc_id, title, created_at, duration_seconds, filename, summary_text, notes
         FROM documents WHERE doc_id = ?",
    )?;
    let mut rows = stmt.query_map(params![doc_id], |row| {
        let created_at_opt: Option<String> = row.get(2)?;
        Ok(DocumentDetail {
            doc_id: row.get(0)?,
            title: row.get(1)?,
            created_at: created_at_opt
                .map(|s| parse_ts(&s))
                .unwrap_or(chrono::DateTime::UNIX_EPOCH),
            duration_seconds: row.get(3)?,
            filename: row.get(4)?,
            summary_text: row.get(5)?,
            notes: row.get(6)?,
            attendees: Vec::new(),
            labels: Vec::new(),
        })
    })?;

    let Some(doc) = rows.next() else {
        return Ok(None);
    };
    let mut doc = doc.map_err(|e| crate::Error::Database(e.to_string()))?;

    // Fetch attendees
    let mut att_stmt = conn.prepare(
        "SELECT name FROM attendees WHERE doc_id = ? AND name IS NOT NULL ORDER BY name",
    )?;
    let att_rows = att_stmt.query_map(params![doc_id], |row| row.get::<_, String>(0))?;
    for name in att_rows {
        doc.attendees.push(name.map_err(|e| crate::Error::Database(e.to_string()))?);
    }

    // Fetch labels
    let mut lbl_stmt = conn.prepare(
        "SELECT label FROM labels WHERE doc_id = ? ORDER BY label",
    )?;
    let lbl_rows = lbl_stmt.query_map(params![doc_id], |row| row.get::<_, String>(0))?;
    for label in lbl_rows {
        doc.labels.push(label.map_err(|e| crate::Error::Database(e.to_string()))?);
    }

    Ok(Some(doc))
}

/// Search documents by title (case-insensitive LIKE).
pub fn search_documents(conn: &Connection, query: &str, limit: usize) -> Result<Vec<DocumentRow>> {
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT doc_id, title, created_at, duration_seconds, filename
         FROM documents
         WHERE title ILIKE ?
         ORDER BY created_at DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(params![pattern, limit as i64], parse_document_row)?;
    let mut docs = Vec::new();
    for row in rows {
        docs.push(row?);
    }
    Ok(docs)
}

/// Filter documents by attendee name (case-insensitive).
pub fn filter_by_attendee(conn: &Connection, name: &str) -> Result<Vec<DocumentRow>> {
    let pattern = format!("%{}%", name);
    let mut stmt = conn.prepare(
        "SELECT DISTINCT d.doc_id, d.title, d.created_at, d.duration_seconds, d.filename
         FROM documents d
         JOIN attendees a ON d.doc_id = a.doc_id
         WHERE a.name ILIKE ?
         ORDER BY d.created_at DESC",
    )?;
    let rows = stmt.query_map(params![pattern], parse_document_row)?;
    let mut docs = Vec::new();
    for row in rows {
        docs.push(row?);
    }
    Ok(docs)
}

/// Filter documents by label (exact match).
pub fn filter_by_label(conn: &Connection, label: &str) -> Result<Vec<DocumentRow>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT d.doc_id, d.title, d.created_at, d.duration_seconds, d.filename
         FROM documents d
         JOIN labels l ON d.doc_id = l.doc_id
         WHERE l.label = ?
         ORDER BY d.created_at DESC",
    )?;
    let rows = stmt.query_map(params![label], parse_document_row)?;
    let mut docs = Vec::new();
    for row in rows {
        docs.push(row?);
    }
    Ok(docs)
}

/// Get aggregate statistics across all meetings.
pub fn get_stats(conn: &Connection) -> Result<Stats> {
    let total_meetings: i64 =
        conn.query_row("SELECT count(*) FROM documents", [], |row| row.get(0))?;

    let total_duration: i64 = conn.query_row(
        "SELECT coalesce(sum(duration_seconds), 0) FROM documents",
        [],
        |row| row.get(0),
    )?;

    let unique_attendees: i64 = conn.query_row(
        "SELECT count(DISTINCT name) FROM attendees WHERE name IS NOT NULL",
        [],
        |row| row.get(0),
    )?;

    let meetings_per_week: f64 = if total_meetings == 0 {
        0.0
    } else {
        // Count meetings in the last 12 weeks for a meaningful recent rate
        let recent_count: i64 = conn.query_row(
            "SELECT count(*) FROM documents WHERE CAST(created_at AS TIMESTAMP) >= current_date - INTERVAL 84 DAY",
            [],
            |row| row.get(0),
        )?;
        recent_count as f64 / 12.0
    };

    Ok(Stats {
        total_meetings,
        total_duration_seconds: total_duration,
        unique_attendees,
        meetings_per_week,
    })
}

/// List all doc_ids and filenames from the sync cache.
pub fn list_all_cached_entries(conn: &Connection) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT doc_id, filename FROM sync_cache")?;
    let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row?);
    }
    Ok(entries)
}

/// Delete a document and all related rows from the database.
/// DuckDB doesn't support CASCADE, so we delete child rows first.
pub fn delete_document(conn: &Connection, doc_id: &str) -> Result<()> {
    conn.execute("DELETE FROM attendees WHERE doc_id = ?", params![doc_id])?;
    conn.execute("DELETE FROM labels WHERE doc_id = ?", params![doc_id])?;
    conn.execute("DELETE FROM participants WHERE doc_id = ?", params![doc_id])?;
    conn.execute("DELETE FROM documents WHERE doc_id = ?", params![doc_id])?;
    conn.execute("DELETE FROM sync_cache WHERE doc_id = ?", params![doc_id])?;
    Ok(())
}

/// Migrate sync cache from the legacy JSON file format to DuckDB.
pub fn migrate_from_json_cache(conn: &Connection, cache_path: &std::path::Path) -> Result<()> {
    use std::collections::HashMap;

    #[derive(serde::Deserialize)]
    struct LegacyCacheEntry {
        filename: String,
        updated_at: DateTime<Utc>,
    }

    let content = std::fs::read_to_string(cache_path)?;
    let cache: HashMap<String, LegacyCacheEntry> = serde_json::from_str(&content)?;

    for (doc_id, entry) in &cache {
        upsert_cache_entry(conn, doc_id, &entry.filename, &entry.updated_at)?;
    }

    Ok(())
}

/// A row for weekly meeting counts.
#[derive(Debug, Clone)]
pub struct WeeklyCount {
    pub week: String,
    pub count: i64,
}

/// A row for daily meeting counts.
#[derive(Debug, Clone)]
pub struct DailyCount {
    pub day: String,
    pub count: i64,
}

/// A row for monthly meeting counts.
#[derive(Debug, Clone)]
pub struct MonthlyCount {
    pub month: String,
    pub count: i64,
}

/// An attendee with their meeting frequency.
#[derive(Debug, Clone)]
pub struct AttendeeFrequency {
    pub name: String,
    pub count: i64,
}

/// A label with its frequency.
#[derive(Debug, Clone)]
pub struct LabelFrequency {
    pub label: String,
    pub count: i64,
}

/// Meeting count by day of week (Monday=1 through Sunday=7).
#[derive(Debug, Clone)]
pub struct DayOfWeekCount {
    pub day_name: String,
    pub day_num: i64,
    pub count: i64,
}

/// Meeting count by hour of day (0-23).
#[derive(Debug, Clone)]
pub struct HourOfDayCount {
    pub hour: i64,
    pub count: i64,
}

/// Average attendee count per meeting, grouped by month.
#[derive(Debug, Clone)]
pub struct MeetingSizeByMonth {
    pub month: String,
    pub avg_attendees: f64,
}

/// An attendee with their recent meeting frequency.
#[derive(Debug, Clone)]
pub struct RecentCollaborator {
    pub name: String,
    pub count: i64,
}

/// A company with its meeting frequency.
#[derive(Debug, Clone)]
pub struct CompanyFrequency {
    pub company: String,
    pub count: i64,
}

/// A label count for a specific month, used for label trend analysis.
#[derive(Debug, Clone)]
pub struct LabelByMonth {
    pub label: String,
    pub month: String,
    pub count: i64,
}

/// Fun one-liner stats about meeting history.
#[derive(Debug, Clone, Default)]
pub struct Superlatives {
    /// Longest meeting: (title, date, duration_seconds)
    pub marathon: Option<(String, String, i64)>,
    /// Person who met with the most different co-attendees: (name, unique_count)
    pub social_butterfly: Option<(String, i64)>,
    /// Count of meetings with 5+ attendees and under 15 minutes
    pub email_meetings: i64,
    /// Longest consecutive-days-with-meetings streak
    pub streak_days: i64,
    /// Start date of the streak
    pub streak_start: Option<String>,
    /// End date of the streak
    pub streak_end: Option<String>,
    /// Meetings with 0-1 attendees
    pub solo_meetings: i64,
    /// Most repeated meeting title: (title, count)
    pub recurring_champ: Option<(String, i64)>,
}

/// Get meeting counts grouped by week for the last N weeks.
pub fn meetings_per_week(conn: &Connection, weeks: usize) -> Result<Vec<WeeklyCount>> {
    let mut stmt = conn.prepare(
        "SELECT strftime(CAST(created_at AS TIMESTAMP), '%Y-W%W') as week, count(*) as cnt
         FROM documents
         WHERE CAST(created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 7 DAY
         GROUP BY week
         ORDER BY week DESC",
    )?;
    let rows = stmt.query_map(params![weeks as i64], |row| {
        Ok(WeeklyCount {
            week: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get meeting counts grouped by day for the last N days.
pub fn meetings_per_day(conn: &Connection, days: usize) -> Result<Vec<DailyCount>> {
    let mut stmt = conn.prepare(
        "SELECT strftime(CAST(created_at AS TIMESTAMP), '%Y-%m-%d') as day, count(*) as cnt
         FROM documents
         WHERE CAST(created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 1 DAY
         GROUP BY day
         ORDER BY day DESC",
    )?;
    let rows = stmt.query_map(params![days as i64], |row| {
        Ok(DailyCount {
            day: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get meeting counts grouped by month for the last N months (approximated as N*30 days).
pub fn meetings_per_month(conn: &Connection, months: usize) -> Result<Vec<MonthlyCount>> {
    let mut stmt = conn.prepare(
        "SELECT strftime(CAST(created_at AS TIMESTAMP), '%Y-%m') as month, count(*) as cnt
         FROM documents
         WHERE CAST(created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 30 DAY
         GROUP BY month
         ORDER BY month DESC",
    )?;
    let rows = stmt.query_map(params![months as i64], |row| {
        Ok(MonthlyCount {
            month: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get meeting counts grouped by day of week (ISO: Monday=1, Sunday=7).
pub fn meetings_by_weekday(conn: &Connection) -> Result<Vec<DayOfWeekCount>> {
    let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let mut stmt = conn.prepare(
        "SELECT isodow(CAST(created_at AS TIMESTAMP)) as dow, count(*) as cnt
         FROM documents
         GROUP BY dow
         ORDER BY dow",
    )?;
    let rows = stmt.query_map([], |row| {
        let dow: i64 = row.get(0)?;
        Ok(DayOfWeekCount {
            day_name: day_names
                .get((dow - 1) as usize)
                .unwrap_or(&"?")
                .to_string(),
            day_num: dow,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get meeting counts grouped by hour of day (0-23).
pub fn meetings_by_hour(conn: &Connection) -> Result<Vec<HourOfDayCount>> {
    let mut stmt = conn.prepare(
        "SELECT hour(CAST(created_at AS TIMESTAMP)) as hr, count(*) as cnt
         FROM documents
         GROUP BY hr
         ORDER BY hr",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(HourOfDayCount {
            hour: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get average attendees per meeting, grouped by month.
pub fn avg_attendees_by_month(conn: &Connection, months: usize) -> Result<Vec<MeetingSizeByMonth>> {
    let mut stmt = conn.prepare(
        "SELECT strftime(CAST(d.created_at AS TIMESTAMP), '%Y-%m') as month,
                avg(COALESCE(att_count, 0)) as avg_att
         FROM documents d
         LEFT JOIN (
             SELECT doc_id, count(*) as att_count FROM attendees GROUP BY doc_id
         ) a ON d.doc_id = a.doc_id
         WHERE d.created_at IS NOT NULL
           AND CAST(d.created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 30 DAY
         GROUP BY month
         ORDER BY month DESC",
    )?;
    let rows = stmt.query_map(params![months as i64], |row| {
        Ok(MeetingSizeByMonth {
            month: row.get(0)?,
            avg_attendees: row.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get top attendees from the last N days.
pub fn top_collaborators_recent(
    conn: &Connection,
    days: usize,
    limit: usize,
) -> Result<Vec<RecentCollaborator>> {
    let mut stmt = conn.prepare(
        "SELECT a.name, count(*) as cnt
         FROM attendees a
         JOIN documents d ON a.doc_id = d.doc_id
         WHERE a.name IS NOT NULL
           AND CAST(d.created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 1 DAY
         GROUP BY a.name
         ORDER BY cnt DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(params![days as i64, limit as i64], |row| {
        Ok(RecentCollaborator {
            name: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get attendees who first appeared this month (never seen in prior months).
pub fn new_attendees_this_month(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT a.name
         FROM attendees a
         JOIN documents d ON a.doc_id = d.doc_id
         WHERE a.name IS NOT NULL
           AND strftime(CAST(d.created_at AS TIMESTAMP), '%Y-%m') = strftime(current_date, '%Y-%m')
           AND a.name NOT IN (
               SELECT DISTINCT a2.name
               FROM attendees a2
               JOIN documents d2 ON a2.doc_id = d2.doc_id
               WHERE a2.name IS NOT NULL
                 AND CAST(d2.created_at AS TIMESTAMP) < date_trunc('month', current_date)
           )
         GROUP BY a.name
         ORDER BY a.name",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get the top N companies by meeting frequency.
pub fn top_companies(conn: &Connection, limit: usize) -> Result<Vec<CompanyFrequency>> {
    let mut stmt = conn.prepare(
        "SELECT company_name, count(DISTINCT doc_id) as cnt
         FROM attendees
         WHERE company_name IS NOT NULL AND company_name != ''
         GROUP BY company_name
         ORDER BY cnt DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(CompanyFrequency {
            company: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get label counts by month for the last N months.
pub fn label_trends(conn: &Connection, months: usize) -> Result<Vec<LabelByMonth>> {
    let mut stmt = conn.prepare(
        "SELECT l.label,
                strftime(CAST(d.created_at AS TIMESTAMP), '%Y-%m') as month,
                count(*) as cnt
         FROM labels l
         JOIN documents d ON l.doc_id = d.doc_id
         WHERE CAST(d.created_at AS TIMESTAMP) >= current_date - CAST(? AS INTEGER) * INTERVAL 30 DAY
         GROUP BY l.label, month
         ORDER BY l.label, month",
    )?;
    let rows = stmt.query_map(params![months as i64], |row| {
        Ok(LabelByMonth {
            label: row.get(0)?,
            month: row.get(1)?,
            count: row.get(2)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Compute fun one-liner stats about meeting history.
#[allow(clippy::field_reassign_with_default)]
pub fn superlatives(conn: &Connection) -> Result<Superlatives> {
    let mut s = Superlatives::default();

    // Marathon: longest meeting by duration
    s.marathon = conn
        .query_row(
            "SELECT title, strftime(CAST(created_at AS TIMESTAMP), '%Y-%m-%d'), duration_seconds
             FROM documents
             WHERE duration_seconds IS NOT NULL AND created_at IS NOT NULL
             ORDER BY duration_seconds DESC
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?
                        .unwrap_or_else(|| "Untitled".to_string()),
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .ok();

    // Social butterfly: attendee who met with most distinct co-attendees
    s.social_butterfly = conn
        .query_row(
            "SELECT a1.name, count(DISTINCT a2.name) as co_count
             FROM attendees a1
             JOIN attendees a2 ON a1.doc_id = a2.doc_id AND a1.name != a2.name
             WHERE a1.name IS NOT NULL AND a2.name IS NOT NULL
             GROUP BY a1.name
             ORDER BY co_count DESC
             LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok();

    // Could've been an email: 5+ attendees, under 15 min
    s.email_meetings = conn
        .query_row(
            "SELECT count(*) FROM (
                 SELECT d.doc_id
                 FROM documents d
                 JOIN attendees a ON d.doc_id = a.doc_id
                 WHERE d.duration_seconds IS NOT NULL AND d.duration_seconds < 900
                 GROUP BY d.doc_id
                 HAVING count(a.id) >= 5
             )",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Meeting streak: longest consecutive days with at least one meeting
    s.streak_days = 0;
    if let Ok(mut stmt) = conn.prepare(
        "WITH meeting_days AS (
             SELECT DISTINCT CAST(CAST(created_at AS TIMESTAMP) AS DATE) as day
             FROM documents
             WHERE created_at IS NOT NULL
         ),
         gaps AS (
             SELECT day,
                    day - CAST(ROW_NUMBER() OVER (ORDER BY day) AS INTEGER) * INTERVAL 1 DAY as grp
             FROM meeting_days
         ),
         streaks AS (
             SELECT min(day) as streak_start,
                    max(day) as streak_end,
                    count(*) as streak_len
             FROM gaps
             GROUP BY grp
         )
         SELECT streak_start, streak_end, streak_len
         FROM streaks
         ORDER BY streak_len DESC
         LIMIT 1",
    ) {
        if let Ok(mut rows) = stmt.query([]) {
            if let Ok(Some(row)) = rows.next() {
                let start: String = row.get(0).unwrap_or_default();
                let end: String = row.get(1).unwrap_or_default();
                let len: i64 = row.get(2).unwrap_or(0);
                s.streak_days = len;
                s.streak_start = Some(start);
                s.streak_end = Some(end);
            }
        }
    }

    // Solo meetings: 0 or 1 attendees (just you)
    s.solo_meetings = conn
        .query_row(
            "SELECT count(*) FROM (
                 SELECT d.doc_id
                 FROM documents d
                 LEFT JOIN attendees a ON d.doc_id = a.doc_id
                 GROUP BY d.doc_id
                 HAVING count(a.id) <= 1
             )",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    // Recurring champion: most repeated meeting title
    s.recurring_champ = conn
        .query_row(
            "SELECT title, count(*) as cnt
             FROM documents
             WHERE title IS NOT NULL AND title != ''
             GROUP BY title
             ORDER BY cnt DESC
             LIMIT 1",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok();

    Ok(s)
}

/// Get the top N busiest weeks by meeting count.
pub fn busiest_weeks(conn: &Connection, limit: usize) -> Result<Vec<WeeklyCount>> {
    let mut stmt = conn.prepare(
        "SELECT strftime(CAST(created_at AS TIMESTAMP), '%Y-W%W') as week, count(*) as cnt
         FROM documents
         WHERE created_at IS NOT NULL
         GROUP BY week
         ORDER BY cnt DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(WeeklyCount {
            week: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get the top N attendees by meeting frequency.
pub fn top_attendees(conn: &Connection, limit: usize) -> Result<Vec<AttendeeFrequency>> {
    let mut stmt = conn.prepare(
        "SELECT name, count(*) as cnt
         FROM attendees
         WHERE name IS NOT NULL
         GROUP BY name
         ORDER BY cnt DESC
         LIMIT ?",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(AttendeeFrequency {
            name: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

/// Get the average meeting duration in minutes.
pub fn average_duration(conn: &Connection) -> Result<f64> {
    let avg: f64 = conn.query_row(
        "SELECT coalesce(avg(duration_seconds), 0) / 60.0 FROM documents WHERE duration_seconds IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    Ok(avg)
}

/// List all unique attendee names, sorted alphabetically.
pub fn list_all_attendees(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT DISTINCT name FROM attendees WHERE name IS NOT NULL ORDER BY name")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut names = Vec::new();
    for row in rows {
        names.push(row?);
    }
    Ok(names)
}

/// Get label frequency distribution.
pub fn label_distribution(conn: &Connection) -> Result<Vec<LabelFrequency>> {
    let mut stmt = conn.prepare(
        "SELECT label, count(*) as cnt
         FROM labels
         GROUP BY label
         ORDER BY cnt DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(LabelFrequency {
            label: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::connection::open_in_memory;
    use crate::model::{
        Attendee, CompanyInfo, DocumentMetadata, Employment, PersonDetails, PersonInfo, PersonName,
    };

    fn make_test_metadata(title: &str, participants: &[&str], labels: &[&str]) -> DocumentMetadata {
        DocumentMetadata {
            id: None,
            title: Some(title.to_string()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: Some("2025-10-29T01:00:00Z".parse().unwrap()),
            participants: participants.iter().map(|s| s.to_string()).collect(),
            duration_seconds: Some(3600),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            creator: None,
            attendees: None,
        }
    }

    fn make_metadata_with_attendees(
        title: &str,
        attendee_names: &[&str],
        labels: &[&str],
    ) -> DocumentMetadata {
        let attendees: Vec<Attendee> = attendee_names
            .iter()
            .map(|name| Attendee {
                name: Some(name.to_string()),
                email: Some(format!("{}@example.com", name.to_lowercase())),
                details: Some(PersonDetails {
                    person: Some(PersonInfo {
                        name: None,
                        employment: Some(Employment {
                            title: Some("Engineer".to_string()),
                        }),
                        linkedin: None,
                    }),
                    company: Some(CompanyInfo {
                        name: Some("Acme".to_string()),
                    }),
                }),
            })
            .collect();

        DocumentMetadata {
            id: None,
            title: Some(title.to_string()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: Some("2025-10-29T01:00:00Z".parse().unwrap()),
            participants: attendee_names.iter().map(|s| s.to_string()).collect(),
            duration_seconds: Some(3600),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            creator: None,
            attendees: Some(attendees),
        }
    }

    #[test]
    fn test_upsert_document_roundtrip() {
        let conn = open_in_memory().unwrap();
        let meta = make_metadata_with_attendees("Q4 Planning", &["Alice", "Bob"], &["Planning"]);
        upsert_document(&conn, &meta, "doc1", "2025-10-28_q4-planning", None, None).unwrap();

        let docs = list_documents(&conn).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].doc_id, "doc1");
        assert_eq!(docs[0].title.as_deref(), Some("Q4 Planning"));
        assert_eq!(docs[0].duration_seconds, Some(3600));

        // Verify attendees
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM attendees WHERE doc_id = 'doc1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);

        // Verify labels
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM labels WHERE doc_id = 'doc1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_upsert_is_idempotent() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Standup", &["Alice"], &[]);
        upsert_document(&conn, &meta, "doc1", "2025-10-28_standup", None, None).unwrap();
        upsert_document(&conn, &meta, "doc1", "2025-10-28_standup", None, None).unwrap();

        let docs = list_documents(&conn).unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[test]
    fn test_cache_entry_roundtrip() {
        let conn = open_in_memory().unwrap();
        let ts: DateTime<Utc> = "2025-10-28T15:04:05Z".parse().unwrap();

        assert!(get_cache_entry(&conn, "doc1").unwrap().is_none());

        upsert_cache_entry(&conn, "doc1", "2025-10-28_standup", &ts).unwrap();
        let entry = get_cache_entry(&conn, "doc1").unwrap().unwrap();
        assert_eq!(entry.filename, "2025-10-28_standup");
        assert_eq!(entry.updated_at, ts);

        // Update it
        let ts2: DateTime<Utc> = "2025-10-29T01:00:00Z".parse().unwrap();
        upsert_cache_entry(&conn, "doc1", "2025-10-29_standup-v2", &ts2).unwrap();
        let entry = get_cache_entry(&conn, "doc1").unwrap().unwrap();
        assert_eq!(entry.filename, "2025-10-29_standup-v2");
        assert_eq!(entry.updated_at, ts2);
    }

    #[test]
    fn test_doc_missing_summary() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting", &[], &[]);

        // Doc with no summary
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();
        assert!(doc_missing_summary(&conn, "doc1").unwrap());

        // Doc with summary
        upsert_document(&conn, &meta, "doc2", "b", None, Some("AI summary")).unwrap();
        assert!(!doc_missing_summary(&conn, "doc2").unwrap());

        // Unknown doc
        assert!(doc_missing_summary(&conn, "doc-unknown").unwrap());
    }

    #[test]
    fn test_search_documents() {
        let conn = open_in_memory().unwrap();
        let meta1 = make_test_metadata("Q4 Planning", &[], &[]);
        let meta2 = make_test_metadata("Weekly Standup", &[], &[]);
        let meta3 = make_test_metadata("Design Review", &[], &[]);
        upsert_document(&conn, &meta1, "doc1", "q4", None, None).unwrap();
        upsert_document(&conn, &meta2, "doc2", "standup", None, None).unwrap();
        upsert_document(&conn, &meta3, "doc3", "design", None, None).unwrap();

        let results = search_documents(&conn, "planning", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc1");

        let results = search_documents(&conn, "STANDUP", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc2");
    }

    #[test]
    fn test_filter_by_attendee() {
        let conn = open_in_memory().unwrap();
        let meta1 = make_metadata_with_attendees("Meeting A", &["Alice", "Bob"], &[]);
        let meta2 = make_metadata_with_attendees("Meeting B", &["Charlie"], &[]);
        upsert_document(&conn, &meta1, "doc1", "a", None, None).unwrap();
        upsert_document(&conn, &meta2, "doc2", "b", None, None).unwrap();

        let results = filter_by_attendee(&conn, "Alice").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc1");

        let results = filter_by_attendee(&conn, "charlie").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc2");
    }

    #[test]
    fn test_filter_by_label() {
        let conn = open_in_memory().unwrap();
        let meta1 = make_test_metadata("Meeting A", &[], &["Planning", "Q4"]);
        let meta2 = make_test_metadata("Meeting B", &[], &["Review"]);
        upsert_document(&conn, &meta1, "doc1", "a", None, None).unwrap();
        upsert_document(&conn, &meta2, "doc2", "b", None, None).unwrap();

        let results = filter_by_label(&conn, "Planning").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc1");

        let results = filter_by_label(&conn, "Q4").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc1");

        let results = filter_by_label(&conn, "Review").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].doc_id, "doc2");
    }

    #[test]
    fn test_stats() {
        let conn = open_in_memory().unwrap();
        let mut meta = make_metadata_with_attendees("Meeting", &["Alice", "Bob"], &[]);
        meta.created_at = Some(Utc::now());
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let stats = get_stats(&conn).unwrap();
        assert_eq!(stats.total_meetings, 1);
        assert_eq!(stats.total_duration_seconds, 3600);
        assert_eq!(stats.unique_attendees, 2);
        // 1 meeting in last 12 weeks → 1/12 ≈ 0.083
        assert!(stats.meetings_per_week > 0.0);
        assert!(stats.meetings_per_week < 1.0);
    }

    #[test]
    fn test_stats_empty() {
        let conn = open_in_memory().unwrap();
        let stats = get_stats(&conn).unwrap();
        assert_eq!(stats.total_meetings, 0);
        assert_eq!(stats.total_duration_seconds, 0);
        assert_eq!(stats.unique_attendees, 0);
        assert_eq!(stats.meetings_per_week, 0.0);
    }

    #[test]
    fn test_migrate_from_json_cache() {
        let conn = open_in_memory().unwrap();
        let temp = tempfile::TempDir::new().unwrap();
        let cache_path = temp.path().join(".sync_cache.json");

        let cache_json = r#"{
            "doc1": {"filename": "2025-10-28_standup", "updated_at": "2025-10-28T15:04:05Z"},
            "doc2": {"filename": "2025-10-29_planning", "updated_at": "2025-10-29T01:00:00Z"}
        }"#;
        std::fs::write(&cache_path, cache_json).unwrap();

        migrate_from_json_cache(&conn, &cache_path).unwrap();

        let entry1 = get_cache_entry(&conn, "doc1").unwrap().unwrap();
        assert_eq!(entry1.filename, "2025-10-28_standup");

        let entry2 = get_cache_entry(&conn, "doc2").unwrap().unwrap();
        assert_eq!(entry2.filename, "2025-10-29_planning");
    }

    #[test]
    fn test_delete_document_cleans_up_related() {
        let conn = open_in_memory().unwrap();
        let meta = make_metadata_with_attendees("Meeting", &["Alice"], &["Tag"]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();
        upsert_cache_entry(
            &conn,
            "doc1",
            "a",
            &"2025-10-28T15:04:05Z".parse().unwrap(),
        )
        .unwrap();

        // Verify data exists
        assert!(get_document(&conn, "doc1").unwrap().is_some());
        assert!(!list_all_cached_entries(&conn).unwrap().is_empty());

        // Delete via the public function
        delete_document(&conn, "doc1").unwrap();

        // Everything should be gone
        assert!(get_document(&conn, "doc1").unwrap().is_none());
        assert!(list_all_cached_entries(&conn).unwrap().is_empty());

        let att_count: i64 = conn
            .query_row("SELECT count(*) FROM attendees", [], |row| row.get(0))
            .unwrap();
        assert_eq!(att_count, 0);

        let label_count: i64 = conn
            .query_row("SELECT count(*) FROM labels", [], |row| row.get(0))
            .unwrap();
        assert_eq!(label_count, 0);
    }

    #[test]
    fn test_get_document_returns_full_detail() {
        let conn = open_in_memory().unwrap();
        let meta = make_metadata_with_attendees("Team Standup", &["Alice", "Bob"], &["daily"]);
        upsert_document(
            &conn,
            &meta,
            "doc1",
            "standup",
            Some("my notes"),
            Some("AI summary"),
        )
        .unwrap();

        let doc = get_document(&conn, "doc1").unwrap().unwrap();
        assert_eq!(doc.doc_id, "doc1");
        assert_eq!(doc.title.as_deref(), Some("Team Standup"));
        assert_eq!(doc.notes.as_deref(), Some("my notes"));
        assert_eq!(doc.summary_text.as_deref(), Some("AI summary"));
        assert_eq!(doc.attendees, vec!["Alice", "Bob"]);
        assert_eq!(doc.labels, vec!["daily"]);
        assert_eq!(doc.duration_seconds, Some(3600));
        // created_at should be parsed, not UNIX_EPOCH
        assert!(doc.created_at.timestamp() > 0);
    }

    #[test]
    fn test_get_document_returns_none_for_missing() {
        let conn = open_in_memory().unwrap();
        assert!(get_document(&conn, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_list_all_cached_entries_returns_inserted() {
        let conn = open_in_memory().unwrap();
        let ts: DateTime<Utc> = "2025-10-28T15:04:05Z".parse().unwrap();
        upsert_cache_entry(&conn, "doc1", "file_a", &ts).unwrap();
        upsert_cache_entry(&conn, "doc2", "file_b", &ts).unwrap();

        let entries = list_all_cached_entries(&conn).unwrap();
        assert_eq!(entries.len(), 2);
        let ids: Vec<&str> = entries.iter().map(|(id, _)| id.as_str()).collect();
        assert!(ids.contains(&"doc1"));
        assert!(ids.contains(&"doc2"));
    }

    #[test]
    fn test_upsert_preserves_attendees_when_none() {
        let conn = open_in_memory().unwrap();

        // First upsert with attendees
        let meta = make_metadata_with_attendees("Meeting", &["Alice", "Bob"], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let doc = get_document(&conn, "doc1").unwrap().unwrap();
        assert_eq!(doc.attendees.len(), 2);

        // Second upsert with attendees = None (API omitted them)
        let meta_no_att = make_test_metadata("Meeting Updated", &[], &[]);
        assert!(meta_no_att.attendees.is_none());
        upsert_document(&conn, &meta_no_att, "doc1", "a", None, None).unwrap();

        // Attendees should still be there
        let doc = get_document(&conn, "doc1").unwrap().unwrap();
        assert_eq!(doc.attendees.len(), 2);
        assert_eq!(doc.title.as_deref(), Some("Meeting Updated"));
    }

    #[test]
    fn test_top_attendees() {
        let conn = open_in_memory().unwrap();
        let meta1 = make_metadata_with_attendees("Meeting A", &["Alice", "Bob"], &[]);
        let meta2 = make_metadata_with_attendees("Meeting B", &["Alice", "Charlie"], &[]);
        upsert_document(&conn, &meta1, "doc1", "a", None, None).unwrap();
        upsert_document(&conn, &meta2, "doc2", "b", None, None).unwrap();

        let top = top_attendees(&conn, 10).unwrap();
        assert_eq!(top[0].name, "Alice");
        assert_eq!(top[0].count, 2);
        assert_eq!(top.len(), 3); // Alice, Bob, Charlie
    }

    #[test]
    fn test_average_duration() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting", &[], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let avg = average_duration(&conn).unwrap();
        assert_eq!(avg, 60.0); // 3600 seconds = 60 minutes
    }

    #[test]
    fn test_label_distribution() {
        let conn = open_in_memory().unwrap();
        let meta1 = make_test_metadata("Meeting A", &[], &["Planning", "Q4"]);
        let meta2 = make_test_metadata("Meeting B", &[], &["Planning"]);
        upsert_document(&conn, &meta1, "doc1", "a", None, None).unwrap();
        upsert_document(&conn, &meta2, "doc2", "b", None, None).unwrap();

        let dist = label_distribution(&conn).unwrap();
        assert_eq!(dist[0].label, "Planning");
        assert_eq!(dist[0].count, 2);
        assert_eq!(dist[1].label, "Q4");
        assert_eq!(dist[1].count, 1);
    }

    #[test]
    fn test_list_all_attendees() {
        let conn = open_in_memory().unwrap();
        let meta1 = make_metadata_with_attendees("Meeting A", &["Charlie", "Alice"], &[]);
        let meta2 = make_metadata_with_attendees("Meeting B", &["Bob", "Alice"], &[]);
        upsert_document(&conn, &meta1, "doc1", "a", None, None).unwrap();
        upsert_document(&conn, &meta2, "doc2", "b", None, None).unwrap();

        let attendees = list_all_attendees(&conn).unwrap();
        // Should be deduplicated and sorted alphabetically
        assert_eq!(attendees, vec!["Alice", "Bob", "Charlie"]);
    }

    #[test]
    fn test_list_all_attendees_empty() {
        let conn = open_in_memory().unwrap();
        let attendees = list_all_attendees(&conn).unwrap();
        assert!(attendees.is_empty());
    }

    #[test]
    fn test_upsert_document_with_notes_and_summary() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting with Notes", &["Alice"], &[]);

        let notes_md = "## Action Items\n\n- Fix the bug\n- Deploy";
        let summary = "Discussed bug fixes and deployment plan.";
        upsert_document(
            &conn,
            &meta,
            "doc1",
            "meeting-notes",
            Some(notes_md),
            Some(summary),
        )
        .unwrap();

        // Verify notes and summary_text columns are stored
        let (stored_notes, stored_summary): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT notes, summary_text FROM documents WHERE doc_id = 'doc1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(stored_notes.as_deref(), Some(notes_md));
        assert_eq!(stored_summary.as_deref(), Some(summary));
    }

    #[test]
    fn test_upsert_document_notes_null_when_none() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting without Notes", &[], &[]);

        upsert_document(&conn, &meta, "doc1", "no-notes", None, None).unwrap();

        let (stored_notes, stored_summary): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT notes, summary_text FROM documents WHERE doc_id = 'doc1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert!(stored_notes.is_none());
        assert!(stored_summary.is_none());
    }

    #[test]
    fn test_upsert_document_updates_notes_on_conflict() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting", &[], &[]);

        // First insert with notes
        upsert_document(
            &conn,
            &meta,
            "doc1",
            "a",
            Some("old notes"),
            Some("old summary"),
        )
        .unwrap();

        // Update with different notes
        upsert_document(
            &conn,
            &meta,
            "doc1",
            "a",
            Some("updated notes"),
            Some("updated summary"),
        )
        .unwrap();

        let (stored_notes, stored_summary): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT notes, summary_text FROM documents WHERE doc_id = 'doc1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(stored_notes.as_deref(), Some("updated notes"));
        assert_eq!(stored_summary.as_deref(), Some("updated summary"));
    }

    #[test]
    fn test_meetings_per_day() {
        let conn = open_in_memory().unwrap();
        let meta1 = make_test_metadata("Morning Standup", &["Alice"], &[]);
        let meta2 = make_test_metadata("Afternoon Sync", &["Bob"], &[]);
        upsert_document(&conn, &meta1, "doc1", "standup", None, None).unwrap();
        upsert_document(&conn, &meta2, "doc2", "sync", None, None).unwrap();

        // Both documents have the same created_at date, so should group into one day
        let counts = meetings_per_day(&conn, 365).unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].day, "2025-10-28");
        assert_eq!(counts[0].count, 2);
    }

    #[test]
    fn test_meetings_per_day_empty() {
        let conn = open_in_memory().unwrap();
        let counts = meetings_per_day(&conn, 30).unwrap();
        assert!(counts.is_empty());
    }

    #[test]
    fn test_meetings_per_month() {
        let conn = open_in_memory().unwrap();
        let meta1 = make_test_metadata("Planning", &["Alice"], &[]);
        let meta2 = make_test_metadata("Review", &["Bob"], &[]);
        upsert_document(&conn, &meta1, "doc1", "planning", None, None).unwrap();
        upsert_document(&conn, &meta2, "doc2", "review", None, None).unwrap();

        // Both documents have created_at in 2025-10, so should group into one month
        let counts = meetings_per_month(&conn, 24).unwrap();
        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].month, "2025-10");
        assert_eq!(counts[0].count, 2);
    }

    #[test]
    fn test_meetings_per_month_empty() {
        let conn = open_in_memory().unwrap();
        let counts = meetings_per_month(&conn, 12).unwrap();
        assert!(counts.is_empty());
    }

    #[test]
    fn test_upsert_attendee_uses_display_name() {
        let conn = open_in_memory().unwrap();
        let meta = DocumentMetadata {
            id: None,
            title: Some("Meeting".to_string()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec![],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: Some(vec![Attendee {
                name: None,
                email: Some("alice@acme.com".into()),
                details: Some(PersonDetails {
                    person: Some(PersonInfo {
                        name: Some(PersonName {
                            full_name: Some("Alice Smith".into()),
                        }),
                        employment: None,
                        linkedin: None,
                    }),
                    company: None,
                }),
            }]),
        };
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let name: Option<String> = conn
            .query_row(
                "SELECT name FROM attendees WHERE doc_id = 'doc1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name.as_deref(), Some("Alice Smith"));
    }

    #[test]
    fn test_meetings_by_weekday() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting", &[], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let counts = meetings_by_weekday(&conn).unwrap();
        assert!(!counts.is_empty());
        let total: i64 = counts.iter().map(|c| c.count).sum();
        assert_eq!(total, 1);
        // Verify day_name is valid
        for c in &counts {
            assert!(
                ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"].contains(&c.day_name.as_str())
            );
        }
    }

    #[test]
    fn test_meetings_by_hour() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting", &[], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let counts = meetings_by_hour(&conn).unwrap();
        assert!(!counts.is_empty());
        let total: i64 = counts.iter().map(|c| c.count).sum();
        assert_eq!(total, 1);
        for c in &counts {
            assert!(c.hour >= 0 && c.hour <= 23);
        }
    }

    #[test]
    fn test_avg_attendees_by_month() {
        let conn = open_in_memory().unwrap();
        let mut meta = make_metadata_with_attendees("Meeting", &["Alice", "Bob"], &[]);
        meta.created_at = Some(Utc::now());
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let sizes = avg_attendees_by_month(&conn, 24).unwrap();
        assert!(!sizes.is_empty());
        assert!(sizes[0].avg_attendees >= 1.0);
    }

    #[test]
    fn test_upsert_filters_non_person_attendees() {
        let conn = open_in_memory().unwrap();
        let meta = DocumentMetadata {
            id: None,
            title: Some("Meeting".to_string()),
            created_at: Some("2025-10-28T15:04:05Z".parse().unwrap()),
            updated_at: None,
            participants: vec![],
            duration_seconds: None,
            labels: vec![],
            creator: None,
            attendees: Some(vec![
                Attendee {
                    name: Some("Alice".into()),
                    email: Some("alice@acme.com".into()),
                    details: None,
                },
                Attendee {
                    name: None,
                    email: Some("boardroom@resource.calendar.google.com".into()),
                    details: None,
                },
            ]),
        };
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM attendees WHERE doc_id = 'doc1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "room resource should be filtered out");
    }

    #[test]
    fn test_top_collaborators_recent() {
        let conn = open_in_memory().unwrap();
        let mut meta = make_metadata_with_attendees("Meeting", &["Alice", "Bob"], &[]);
        meta.created_at = Some(Utc::now());
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let collabs = top_collaborators_recent(&conn, 30, 10).unwrap();
        assert_eq!(collabs.len(), 2);
        assert_eq!(collabs[0].count, 1);
    }

    #[test]
    fn test_new_attendees_this_month() {
        let conn = open_in_memory().unwrap();
        let mut meta = make_metadata_with_attendees("Meeting", &["Alice"], &[]);
        meta.created_at = Some(Utc::now());
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let new_faces = new_attendees_this_month(&conn).unwrap();
        // Alice is new because there are no prior months
        assert!(new_faces.contains(&"Alice".to_string()));
    }

    #[test]
    fn test_top_companies() {
        let conn = open_in_memory().unwrap();
        let meta = make_metadata_with_attendees("Meeting", &["Alice"], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let companies = top_companies(&conn, 10).unwrap();
        // make_metadata_with_attendees sets company to "Acme"
        assert_eq!(companies.len(), 1);
        assert_eq!(companies[0].company, "Acme");
    }

    #[test]
    fn test_label_trends() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting", &[], &["Planning", "Review"]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let trends = label_trends(&conn, 24).unwrap();
        assert!(!trends.is_empty());
        let labels: Vec<&str> = trends.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"Planning"));
    }

    #[test]
    fn test_busiest_weeks() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting", &[], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();
        upsert_document(&conn, &meta, "doc2", "b", None, None).unwrap();

        let weeks = busiest_weeks(&conn, 5).unwrap();
        assert!(!weeks.is_empty());
        assert_eq!(weeks[0].count, 2);
    }

    #[test]
    fn test_superlatives_empty_db() {
        let conn = open_in_memory().unwrap();
        let s = superlatives(&conn).unwrap();
        assert!(s.marathon.is_none());
        assert!(s.social_butterfly.is_none());
        assert_eq!(s.email_meetings, 0);
        assert_eq!(s.streak_days, 0);
        assert_eq!(s.solo_meetings, 0);
        assert!(s.recurring_champ.is_none());
    }

    #[test]
    fn test_superlatives_default_has_no_meaningful_data() {
        // The UI checks these exact fields to decide whether to render "No data available".
        // This test ensures Superlatives::default() is treated as empty by that check.
        let s = Superlatives::default();
        let has_any = s.marathon.is_some()
            || s.social_butterfly.is_some()
            || s.email_meetings > 0
            || s.streak_days > 1
            || s.solo_meetings > 0
            || s.recurring_champ.is_some();
        assert!(
            !has_any,
            "Superlatives::default() should have no meaningful data for the UI empty-state"
        );
    }

    #[test]
    fn test_superlatives_with_data() {
        let conn = open_in_memory().unwrap();
        let meta = make_metadata_with_attendees("Standup", &["Alice", "Bob"], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();
        upsert_document(&conn, &meta, "doc2", "b", None, None).unwrap();

        let s = superlatives(&conn).unwrap();
        // Marathon: duration_seconds=3600 from test metadata
        assert!(s.marathon.is_some());
        let (title, _, dur) = s.marathon.unwrap();
        assert_eq!(title, "Standup");
        assert_eq!(dur, 3600);
        // Recurring: "Standup" appears twice
        assert!(s.recurring_champ.is_some());
        let (champ_title, champ_count) = s.recurring_champ.unwrap();
        assert_eq!(champ_title, "Standup");
        assert_eq!(champ_count, 2);
    }

    #[test]
    fn test_busiest_weeks_excludes_null_created_at() {
        let conn = open_in_memory().unwrap();
        let meta = make_test_metadata("Meeting", &[], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        // Manually insert a row with NULL created_at
        conn.execute(
            "INSERT INTO documents (doc_id, filename) VALUES ('doc_null', 'null-doc')",
            [],
        )
        .unwrap();

        let weeks = busiest_weeks(&conn, 52).unwrap();
        let total: i64 = weeks.iter().map(|w| w.count).sum();
        assert_eq!(total, 1, "NULL created_at rows should be excluded");
    }

    #[test]
    fn test_superlatives_excludes_null_created_at() {
        let conn = open_in_memory().unwrap();
        let meta = make_metadata_with_attendees("Standup", &["Alice", "Bob"], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        // Insert a row with NULL created_at and a long duration (would win marathon if included)
        conn.execute(
            "INSERT INTO documents (doc_id, filename, title, duration_seconds) \
             VALUES ('doc_null', 'null-doc', 'Ghost Meeting', 999999)",
            [],
        )
        .unwrap();

        let s = superlatives(&conn).unwrap();
        // Marathon should be the real meeting, not the ghost
        assert!(s.marathon.is_some());
        let (title, _, _) = s.marathon.unwrap();
        assert_eq!(
            title, "Standup",
            "NULL created_at document should be excluded from marathon"
        );
    }

    #[test]
    fn test_avg_attendees_by_month_zero_attendee_docs() {
        let conn = open_in_memory().unwrap();
        // Insert a doc with attendees
        let mut meta = make_metadata_with_attendees("Meeting", &["Alice", "Bob"], &[]);
        meta.created_at = Some(Utc::now());
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        // Insert a doc with no attendees (uses make_test_metadata which has no attendees)
        let mut meta_solo = make_test_metadata("Solo", &[], &[]);
        meta_solo.created_at = Some(Utc::now());
        upsert_document(&conn, &meta_solo, "doc2", "b", None, None).unwrap();

        let sizes = avg_attendees_by_month(&conn, 24).unwrap();
        assert!(!sizes.is_empty());
        // Average should be (2 + 0) / 2 = 1.0 (COALESCE ensures zero, not NULL skip)
        assert!(
            sizes[0].avg_attendees < 2.0,
            "zero-attendee docs should contribute 0 to avg, got {}",
            sizes[0].avg_attendees
        );
    }
}
