use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v6→v7: create tasks table (runs AFTER v6 to avoid column-loss on crash).
    // Defensively check for v6 columns — if missing, add them before proceeding.
    // This self-heals any database that was bumped to a version >= 6 via the
    // old (buggy) migration ordering where v7 ran before v6.
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
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY,
            vault_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            content TEXT NOT NULL,
            checked INTEGER NOT NULL DEFAULT 0,
            line_number INTEGER NOT NULL DEFAULT 0,
            indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
    ")?;
    conn.execute_batch("PRAGMA user_version = 7")?;
    Ok(())
}
