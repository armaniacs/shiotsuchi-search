use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v10→v11: add vlm_hash column to file_cache for VLM extraction caching.
    super::add_column_if_missing(conn, "file_cache", "vlm_hash", "TEXT")?;
    conn.execute_batch("PRAGMA user_version = 11")?;
    Ok(())
}
