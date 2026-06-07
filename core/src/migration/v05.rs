use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v4→v5: add file_size column to file_cache for two-stage skip (mtime+size).
    super::add_column_if_missing(conn, "file_cache", "file_size", "INTEGER NOT NULL DEFAULT 0")?;
    conn.execute_batch("PRAGMA user_version = 5")?;
    Ok(())
}
