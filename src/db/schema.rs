// ABOUTME: DuckDB schema definitions and table initialization
// ABOUTME: Creates normalized tables for documents, attendees, labels, and sync cache

use duckdb::Connection;

use crate::Result;

/// Initialize the database schema, creating tables if they don't exist.
/// Also runs migrations for existing databases that may lack newer columns.
pub fn initialize(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS documents (
            doc_id VARCHAR PRIMARY KEY,
            title VARCHAR,
            created_at VARCHAR,
            updated_at VARCHAR,
            duration_seconds BIGINT,
            source VARCHAR DEFAULT 'granola',
            filename VARCHAR,
            synced_at VARCHAR,
            notes VARCHAR,
            summary_text VARCHAR
        );

        CREATE SEQUENCE IF NOT EXISTS attendees_id_seq;

        CREATE TABLE IF NOT EXISTS attendees (
            id BIGINT DEFAULT nextval('attendees_id_seq') PRIMARY KEY,
            doc_id VARCHAR NOT NULL REFERENCES documents(doc_id),
            name VARCHAR,
            email VARCHAR,
            employment_title VARCHAR,
            company_name VARCHAR,
            linkedin_handle VARCHAR,
            is_creator BOOLEAN DEFAULT false
        );

        CREATE TABLE IF NOT EXISTS labels (
            doc_id VARCHAR NOT NULL REFERENCES documents(doc_id),
            label VARCHAR NOT NULL,
            PRIMARY KEY (doc_id, label)
        );

        CREATE TABLE IF NOT EXISTS participants (
            doc_id VARCHAR NOT NULL REFERENCES documents(doc_id),
            name VARCHAR NOT NULL,
            PRIMARY KEY (doc_id, name)
        );

        CREATE TABLE IF NOT EXISTS sync_cache (
            doc_id VARCHAR PRIMARY KEY,
            filename VARCHAR NOT NULL,
            updated_at VARCHAR NOT NULL
        );
        ",
    )?;

    // Migrate: add notes and summary_text columns for databases created before
    // these columns existed. DuckDB does not support IF NOT EXISTS for ADD COLUMN,
    // so we silently ignore errors when the columns already exist.
    let _ = conn.execute_batch("ALTER TABLE documents ADD COLUMN notes VARCHAR");
    let _ = conn.execute_batch("ALTER TABLE documents ADD COLUMN summary_text VARCHAR");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_creates_tables() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();

        // Verify tables exist via information_schema
        let mut stmt = conn
            .prepare(
                "SELECT table_name FROM information_schema.tables
                 WHERE table_schema = 'main'
                 ORDER BY table_name",
            )
            .unwrap();

        let tables: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert!(tables.contains(&"documents".to_string()));
        assert!(tables.contains(&"attendees".to_string()));
        assert!(tables.contains(&"labels".to_string()));
        assert!(tables.contains(&"participants".to_string()));
        assert!(tables.contains(&"sync_cache".to_string()));
    }

    #[test]
    fn test_initialize_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        initialize(&conn).unwrap();
        initialize(&conn).unwrap();
    }

    #[test]
    fn test_migrate_adds_columns_to_legacy_schema() {
        let conn = Connection::open_in_memory().unwrap();

        // Simulate a legacy database without notes/summary_text columns
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS documents (
                doc_id VARCHAR PRIMARY KEY,
                title VARCHAR,
                created_at VARCHAR,
                updated_at VARCHAR,
                duration_seconds BIGINT,
                source VARCHAR DEFAULT 'granola',
                filename VARCHAR,
                synced_at VARCHAR
            );
            CREATE SEQUENCE IF NOT EXISTS attendees_id_seq;
            CREATE TABLE IF NOT EXISTS attendees (
                id BIGINT DEFAULT nextval('attendees_id_seq') PRIMARY KEY,
                doc_id VARCHAR NOT NULL REFERENCES documents(doc_id),
                name VARCHAR,
                email VARCHAR,
                employment_title VARCHAR,
                company_name VARCHAR,
                linkedin_handle VARCHAR,
                is_creator BOOLEAN DEFAULT false
            );
            CREATE TABLE IF NOT EXISTS labels (
                doc_id VARCHAR NOT NULL REFERENCES documents(doc_id),
                label VARCHAR NOT NULL,
                PRIMARY KEY (doc_id, label)
            );
            CREATE TABLE IF NOT EXISTS participants (
                doc_id VARCHAR NOT NULL REFERENCES documents(doc_id),
                name VARCHAR NOT NULL,
                PRIMARY KEY (doc_id, name)
            );
            CREATE TABLE IF NOT EXISTS sync_cache (
                doc_id VARCHAR PRIMARY KEY,
                filename VARCHAR NOT NULL,
                updated_at VARCHAR NOT NULL
            );",
        )
        .unwrap();

        // Insert a document without notes/summary_text columns
        conn.execute(
            "INSERT INTO documents (doc_id, title, created_at) VALUES ('doc1', 'Old Meeting', '2025-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        // Run initialize, which should add the missing columns via migration
        initialize(&conn).unwrap();

        // Verify that notes and summary_text columns now exist and are queryable
        let (notes, summary): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT notes, summary_text FROM documents WHERE doc_id = 'doc1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert!(notes.is_none());
        assert!(summary.is_none());

        // Verify we can update the new columns
        conn.execute(
            "UPDATE documents SET notes = 'some notes', summary_text = 'a summary' WHERE doc_id = 'doc1'",
            [],
        )
        .unwrap();

        let (notes, summary): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT notes, summary_text FROM documents WHERE doc_id = 'doc1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(notes.as_deref(), Some("some notes"));
        assert_eq!(summary.as_deref(), Some("a summary"));
    }
}
