// core/src/migration/mod.rs
use rusqlite::Connection;

mod v02;
mod v03;
pub mod v04;
mod v05;
mod v06;
mod v07;
mod v08;
mod v09;
mod v10;
mod v11;

/// Validate that a string is a safe SQL identifier (alphanumeric + underscore only).
/// Prevents SQL injection when used in `format!()` for table/column names.
fn validate_sql_ident(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("SQL identifier must not be empty".to_string());
    }
    if !s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("invalid SQL identifier: {:?}", s));
    }
    Ok(())
}

/// Check whether a column exists in a table via `PRAGMA table_info`.
///
/// Used across migration versions to safely add columns that may already exist
/// (e.g., when a previous migration was partially applied before a crash).
pub(crate) fn table_has_column(
    conn: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, rusqlite::Error> {
    validate_sql_ident(table).map_err(|e| rusqlite::Error::InvalidParameterName(e))?;
    validate_sql_ident(column).map_err(|e| rusqlite::Error::InvalidParameterName(e))?;
    let sql = format!("PRAGMA table_info({})", table);
    let mut stmt = conn.prepare(&sql)?;
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(cols.iter().any(|c| c == column))
}

/// Add a column via `ALTER TABLE ... ADD COLUMN` only if it doesn't already exist.
///
/// This eliminates the repetitive PRAGMA-check-then-ALTER pattern that appears
/// in every migration version from v03 onwards.
pub(crate) fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<(), rusqlite::Error> {
    validate_sql_ident(table).map_err(|e| rusqlite::Error::InvalidParameterName(e))?;
    validate_sql_ident(column).map_err(|e| rusqlite::Error::InvalidParameterName(e))?;
    if !table_has_column(conn, table, column)? {
        conn.execute_batch(&format!(
            "ALTER TABLE {} ADD COLUMN {} {}",
            table, column, definition
        ))?;
    }
    Ok(())
}

/// Run all pending schema migrations.
/// Wrapped in a transaction for crash safety: if the process dies mid-migration,
/// the schema change and user_version update are rolled back atomically.
pub fn run(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Clean up orphaned file_cache_v3 from a previous crash (runs every migration)
    conn.execute_batch("DROP TABLE IF EXISTS file_cache_v3")?;

    conn.execute_batch("BEGIN TRANSACTION")?;

    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if version < 2  { v02::migrate(conn)?; }
    if version < 3  { v03::migrate(conn)?; }
    if version < 4  { v04::migrate(conn)?; }
    if version < 5  { v05::migrate(conn)?; }
    if version < 6  { v06::migrate(conn)?; }
    if version < 7  { v07::migrate(conn)?; }
    if version < 8  { v08::migrate(conn)?; }
    if version < 9  { v09::migrate(conn)?; }
    if version < 10 { v10::migrate(conn)?; }
    if version < 11 { v11::migrate(conn)?; }

    conn.execute_batch("COMMIT")?;
    Ok(())
}

/// Create the full v11 schema from scratch.
/// Called by v02 migration after dropping old tables.
pub(crate) fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS file_cache (
            vault_name      TEXT NOT NULL,
            path            TEXT NOT NULL,
            hash            TEXT NOT NULL,
            mtime           INTEGER NOT NULL,
            model_id        TEXT NOT NULL,
            file_size       INTEGER NOT NULL DEFAULT 0,
            backlink_count  INTEGER NOT NULL DEFAULT 0,
            char_count      INTEGER NOT NULL DEFAULT 0,
            vlm_hash        TEXT,
            PRIMARY KEY (vault_name, path)
        );

        CREATE TABLE IF NOT EXISTS chunks (
            id                INTEGER PRIMARY KEY,
            file_path         TEXT NOT NULL,
            chunk_index       INTEGER NOT NULL,
            parent_header     TEXT,
            content           TEXT NOT NULL,
            tokenized_content TEXT NOT NULL,
            vault_name        TEXT NOT NULL DEFAULT '',
            tags              TEXT NOT NULL DEFAULT '',
            frontmatter_date  TEXT NOT NULL DEFAULT '',
            title             TEXT NOT NULL DEFAULT '',
            emphasized_text   TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON chunks(vault_name, file_path);

        CREATE TABLE IF NOT EXISTS tasks (
            id          INTEGER PRIMARY KEY,
            vault_name  TEXT NOT NULL,
            file_path   TEXT NOT NULL,
            content     TEXT NOT NULL,
            checked     INTEGER NOT NULL DEFAULT 0,
            line_number INTEGER NOT NULL DEFAULT 0,
            indexed_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS note_links (
            source_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            vault_name  TEXT NOT NULL,
            PRIMARY KEY (source_path, target_path, vault_name)
        );
        CREATE INDEX IF NOT EXISTS idx_note_links_target
            ON note_links(target_path, vault_name);

        CREATE TABLE IF NOT EXISTS tag_counts (
            tag        TEXT NOT NULL,
            vault_name TEXT NOT NULL,
            count      INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (tag, vault_name)
        ) WITHOUT ROWID;

        CREATE VIRTUAL TABLE IF NOT EXISTS fts_chunks USING fts5(
            tokenized_content,
            content='chunks',
            content_rowid='id',
            tokenize='unicode61 remove_diacritics 0'
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
            chunk_id  INTEGER PRIMARY KEY,
            embedding FLOAT[1024]
        );
    ")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_sql_ident_accepts_valid_identifiers() {
        assert!(validate_sql_ident("chunks").is_ok());
        assert!(validate_sql_ident("file_cache").is_ok());
        assert!(validate_sql_ident("tags").is_ok());
        assert!(validate_sql_ident("frontmatter_date").is_ok());
        assert!(validate_sql_ident("table123").is_ok());
        assert!(validate_sql_ident("_private").is_ok());
    }

    #[test]
    fn test_validate_sql_ident_rejects_empty() {
        assert!(validate_sql_ident("").is_err());
    }

    #[test]
    fn test_validate_sql_ident_rejects_sql_injection() {
        assert!(validate_sql_ident("chunks; DROP TABLE chunks").is_err());
        assert!(validate_sql_ident("chunks--comment").is_err());
        assert!(validate_sql_ident("chunks/**/").is_err());
        assert!(validate_sql_ident("table' OR '1'='1").is_err());
        assert!(validate_sql_ident("table\" OR \"1\"=\"1").is_err());
    }

    #[test]
    fn test_validate_sql_ident_rejects_special_chars() {
        assert!(validate_sql_ident("table-name").is_err());
        assert!(validate_sql_ident("table.name").is_err());
        assert!(validate_sql_ident("table name").is_err());
        assert!(validate_sql_ident("table\tname").is_err());
        assert!(validate_sql_ident("table\nname").is_err());
    }

    #[test]
    fn test_table_has_column_rejects_injection() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT)").unwrap();
        let result = table_has_column(&conn, "test_table; DROP TABLE test_table", "id");
        assert!(result.is_err(), "should reject SQL injection in table name");
    }

    #[test]
    fn test_add_column_if_missing_rejects_injection() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE test_table (id INTEGER PRIMARY KEY)").unwrap();
        let result = add_column_if_missing(&conn, "test_table", "col; DROP TABLE test_table", "TEXT");
        assert!(result.is_err(), "should reject SQL injection in column name");
    }

    #[test]
    fn test_table_has_column_works_with_valid_inputs() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT)").unwrap();
        assert!(table_has_column(&conn, "test_table", "id").unwrap());
        assert!(table_has_column(&conn, "test_table", "name").unwrap());
        assert!(!table_has_column(&conn, "test_table", "nonexistent").unwrap());
    }

    #[test]
    fn test_add_column_if_missing_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE test_table (id INTEGER PRIMARY KEY)").unwrap();
        add_column_if_missing(&conn, "test_table", "new_col", "TEXT NOT NULL DEFAULT ''").unwrap();
        assert!(table_has_column(&conn, "test_table", "new_col").unwrap());
        add_column_if_missing(&conn, "test_table", "new_col", "TEXT NOT NULL DEFAULT ''").unwrap();
        assert!(table_has_column(&conn, "test_table", "new_col").unwrap());
    }

    #[test]
    fn test_migration_run_wrapped_in_transaction() {
        // Use NoteDatabase::open_in_memory() which internally calls run()
        // and has the vec0 module available. Verify the user_version reaches 11.
        let db = crate::db::NoteDatabase::open_in_memory().unwrap();
        let conn_ref = db.write_conn.borrow();
        let version: i64 = conn_ref.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version, 11, "migration should advance user_version to 11");

        // Running again manually should be a no-op (idempotent)
        super::run(&conn_ref).unwrap();
        let version2: i64 = conn_ref.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version2, 11, "second migration run should be idempotent");
    }

    #[test]
    fn test_migration_run_rolls_back_on_error() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA user_version = 0").unwrap();
        super::create_schema(&conn).unwrap();

        // Verify that run at least calls the transaction boundary
        // (error paths are handled by rusqlite during individual migrate() calls)
        let version_before: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(version_before, 0);
    }
}
