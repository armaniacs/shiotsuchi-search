use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v7→v8: add emphasized_text column to chunks table
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(chunks)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !cols.iter().any(|c| c == "emphasized_text") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN emphasized_text TEXT NOT NULL DEFAULT ''")?;
    }
    conn.execute_batch("PRAGMA user_version = 8")?;
    Ok(())
}
