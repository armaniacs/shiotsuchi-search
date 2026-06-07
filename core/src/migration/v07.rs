use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v6→v7: create tasks table (runs AFTER v6 to avoid column-loss on crash).
    // Defensively check for v6 columns — if missing, add them before proceeding.
    // This self-heals any database that was bumped to a version >= 6 via the
    // old (buggy) migration ordering where v7 ran before v6.
    super::add_column_if_missing(conn, "chunks", "tags", "TEXT NOT NULL DEFAULT ''")?;
    super::add_column_if_missing(conn, "chunks", "frontmatter_date", "TEXT NOT NULL DEFAULT ''")?;
    super::add_column_if_missing(conn, "chunks", "title", "TEXT NOT NULL DEFAULT ''")?;
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
