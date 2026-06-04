use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v8→v9: add note_links table and backlink_count column to file_cache
    // Wrap multi-statement migration in a transaction for crash safety.
    conn.execute_batch("BEGIN TRANSACTION")?;
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS note_links (
            source_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            vault_name  TEXT NOT NULL,
            PRIMARY KEY (source_path, target_path, vault_name)
        )
    ")?;
    conn.execute_batch("
        CREATE INDEX IF NOT EXISTS idx_note_links_target
        ON note_links(target_path, vault_name)
    ")?;
    let fc_cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(file_cache)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !fc_cols.iter().any(|c| c == "backlink_count") {
        conn.execute_batch(
            "ALTER TABLE file_cache ADD COLUMN backlink_count INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    conn.execute_batch("PRAGMA user_version = 9")?;
    conn.execute_batch("COMMIT")?;
    Ok(())
}
