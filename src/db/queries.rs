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
    let created_at_str: String = row.get(2)?;
    Ok(DocumentRow {
        doc_id: row.get(0)?,
        title: row.get(1)?,
        created_at: parse_ts(&created_at_str),
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

    let created_at = fmt_ts(&meta.created_at);
    let updated_at = meta.updated_at.as_ref().map(fmt_ts);
    let synced_at = fmt_ts(&Utc::now());

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

    // Replace attendees: delete old, insert new
    conn.execute("DELETE FROM attendees WHERE doc_id = ?", params![doc_id])?;
    if let Some(attendees) = &meta.attendees {
        for att in attendees {
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
                    att.name.as_deref(),
                    att.email.as_deref(),
                    emp_title.as_deref(),
                    company.as_deref(),
                    linkedin.as_deref(),
                    is_creator,
                ],
            )?;
        }
    }

    // Replace labels
    conn.execute("DELETE FROM labels WHERE doc_id = ?", params![doc_id])?;
    for label in &meta.labels {
        conn.execute(
            "INSERT INTO labels (doc_id, label) VALUES (?, ?)",
            params![doc_id, label],
        )?;
    }

    // Replace participants
    conn.execute("DELETE FROM participants WHERE doc_id = ?", params![doc_id])?;
    for name in &meta.participants {
        conn.execute(
            "INSERT INTO participants (doc_id, name) VALUES (?, ?)",
            params![doc_id, name],
        )?;
    }

    conn.execute_batch("COMMIT")?;
    Ok(())
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
        // Compute week span from min/max created_at strings (ISO 8601 sorts lexicographically)
        let min_ts: String =
            conn.query_row("SELECT min(created_at) FROM documents", [], |row| {
                row.get(0)
            })?;
        let max_ts: String =
            conn.query_row("SELECT max(created_at) FROM documents", [], |row| {
                row.get(0)
            })?;
        let min_dt = parse_ts(&min_ts);
        let max_dt = parse_ts(&max_ts);
        let duration_secs = (max_dt - min_dt).num_seconds().max(0) as f64;
        let weeks = (duration_secs / 604800.0).max(1.0);
        total_meetings as f64 / weeks
    };

    Ok(Stats {
        total_meetings,
        total_duration_seconds: total_duration,
        unique_attendees,
        meetings_per_week,
    })
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
        Attendee, CompanyInfo, DocumentMetadata, Employment, PersonDetails, PersonInfo,
    };

    fn make_test_metadata(title: &str, participants: &[&str], labels: &[&str]) -> DocumentMetadata {
        DocumentMetadata {
            id: None,
            title: Some(title.to_string()),
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
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
            created_at: "2025-10-28T15:04:05Z".parse().unwrap(),
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
        let meta = make_metadata_with_attendees("Meeting", &["Alice", "Bob"], &[]);
        upsert_document(&conn, &meta, "doc1", "a", None, None).unwrap();

        let stats = get_stats(&conn).unwrap();
        assert_eq!(stats.total_meetings, 1);
        assert_eq!(stats.total_duration_seconds, 3600);
        assert_eq!(stats.unique_attendees, 2);
        assert!(stats.meetings_per_week >= 1.0);
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

        // Verify data exists
        let att_count: i64 = conn
            .query_row("SELECT count(*) FROM attendees", [], |row| row.get(0))
            .unwrap();
        assert_eq!(att_count, 1);

        // Delete related rows first (DuckDB doesn't support CASCADE)
        conn.execute("DELETE FROM attendees WHERE doc_id = 'doc1'", [])
            .unwrap();
        conn.execute("DELETE FROM labels WHERE doc_id = 'doc1'", [])
            .unwrap();
        conn.execute("DELETE FROM participants WHERE doc_id = 'doc1'", [])
            .unwrap();
        conn.execute("DELETE FROM documents WHERE doc_id = 'doc1'", [])
            .unwrap();

        // Attendees should be gone
        let att_count: i64 = conn
            .query_row("SELECT count(*) FROM attendees", [], |row| row.get(0))
            .unwrap();
        assert_eq!(att_count, 0);

        // Labels should be gone
        let label_count: i64 = conn
            .query_row("SELECT count(*) FROM labels", [], |row| row.get(0))
            .unwrap();
        assert_eq!(label_count, 0);

        // Documents should be gone
        let doc_count: i64 = conn
            .query_row("SELECT count(*) FROM documents", [], |row| row.get(0))
            .unwrap();
        assert_eq!(doc_count, 0);
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
}
