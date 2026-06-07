use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Transaction is managed by migration::run() — don't BEGIN/COMMIT here.
    conn.execute_batch("
        DROP TABLE IF EXISTS notes_fts;
        DROP TABLE IF EXISTS notes_meta;
    ")?;
    super::create_schema(conn)?;
    conn.execute_batch("PRAGMA user_version = 2")?;
    Ok(())
}
