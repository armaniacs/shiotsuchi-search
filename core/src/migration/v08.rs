use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v7→v8: add emphasized_text column to chunks table
    super::add_column_if_missing(conn, "chunks", "emphasized_text", "TEXT NOT NULL DEFAULT ''")?;
    conn.execute_batch("PRAGMA user_version = 8")?;
    Ok(())
}
