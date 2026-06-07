use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Transaction is managed by migration::run() — don't BEGIN/COMMIT here.
    super::add_column_if_missing(conn, "file_cache", "char_count", "INTEGER NOT NULL DEFAULT 0")?;
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS tag_counts (
            tag        TEXT NOT NULL,
            vault_name TEXT NOT NULL,
            count      INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (tag, vault_name)
        ) WITHOUT ROWID
    ")?;
    // NOTE: char_count is intentionally NOT backfilled here. SQLite LENGTH()
    // returns UTF-8 byte count, not Unicode character count, which would
    // inflate values for non-ASCII text. char_count is computed correctly
    // via .chars().count() in reindex_file(), so upgraded databases get
    // accurate values on the next re-index — same design as tag_counts.
    conn.execute_batch("PRAGMA user_version = 10")?;
    Ok(())
}
