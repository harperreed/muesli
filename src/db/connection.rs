// ABOUTME: DuckDB connection management with automatic schema initialization
// ABOUTME: Opens or creates the database file and ensures tables exist

use std::path::Path;

use duckdb::Connection;

use crate::Result;

/// Open an existing database or create a new one at the given path.
/// Automatically initializes the schema on every open.
pub fn open_or_create(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    super::schema::initialize(&conn)?;
    Ok(conn)
}

/// Open an in-memory database for testing.
#[cfg(test)]
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    super::schema::initialize(&conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_open_or_create_new_db() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.duckdb");
        let conn = open_or_create(&db_path).unwrap();

        // Verify we can query
        let count: i64 = conn
            .query_row("SELECT count(*) FROM documents", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn test_open_or_create_existing_db() {
        let temp = TempDir::new().unwrap();
        let db_path = temp.path().join("test.duckdb");

        // Create and insert data
        {
            let conn = open_or_create(&db_path).unwrap();
            conn.execute(
                "INSERT INTO documents (doc_id, title, created_at) VALUES (?, ?, now())",
                duckdb::params!["doc1", "Test"],
            )
            .unwrap();
        }

        // Reopen and verify data persists
        let conn = open_or_create(&db_path).unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM documents", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
