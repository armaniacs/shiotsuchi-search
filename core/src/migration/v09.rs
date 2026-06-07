use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Transaction is managed by migration::run() — don't BEGIN/COMMIT here.
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
    super::add_column_if_missing(conn, "file_cache", "backlink_count", "INTEGER NOT NULL DEFAULT 0")?;
    conn.execute_batch("PRAGMA user_version = 9")?;
    Ok(())
}
