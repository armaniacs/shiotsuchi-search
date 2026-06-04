use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v4→v5: add file_size column to file_cache for two-stage skip (mtime+size).
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(file_cache)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !cols.iter().any(|c| c == "file_size") {
        conn.execute_batch(
            "ALTER TABLE file_cache ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    conn.execute_batch("PRAGMA user_version = 5")?;
    Ok(())
}
