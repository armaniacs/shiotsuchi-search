use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Wrap v1→v2 migration in a transaction for crash safety.
    // DROP + schema creation + version bump must be atomic.
    conn.execute_batch("BEGIN TRANSACTION")?;
    conn.execute_batch("
        DROP TABLE IF EXISTS notes_fts;
        DROP TABLE IF EXISTS notes_meta;
    ")?;
    super::create_schema(conn)?;
    conn.execute_batch("PRAGMA user_version = 2")?;
    conn.execute_batch("COMMIT")?;
    Ok(())
}
