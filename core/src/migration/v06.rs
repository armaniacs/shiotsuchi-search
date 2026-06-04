use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v5→v6: add tags, frontmatter_date, title columns to chunks table
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(chunks)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !cols.iter().any(|c| c == "tags") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN tags TEXT NOT NULL DEFAULT ''")?;
    }
    if !cols.iter().any(|c| c == "frontmatter_date") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN frontmatter_date TEXT NOT NULL DEFAULT ''")?;
    }
    if !cols.iter().any(|c| c == "title") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN title TEXT NOT NULL DEFAULT ''")?;
    }
    conn.execute_batch("PRAGMA user_version = 6")?;
    Ok(())
}
