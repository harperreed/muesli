// ABOUTME: DuckDB schema definitions and table initialization
// ABOUTME: Creates normalized tables for documents, attendees, labels, and sync cache

use duckdb::Connection;

use crate::Result;

/// Initialize the database schema, creating tables if they don't exist.
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
        );
        ",
    )?;
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
}
