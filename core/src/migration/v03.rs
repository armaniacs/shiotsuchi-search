use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Check if vault_name column already exists (crash recovery)
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(chunks)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let has_vault_name = cols.iter().any(|c| c == "vault_name");

    if !has_vault_name {
        conn.execute_batch("BEGIN TRANSACTION")?;
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN vault_name TEXT NOT NULL DEFAULT 'default'")?;
        conn.execute_batch("DROP INDEX IF EXISTS idx_chunks_file_path")?;
        conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON chunks(vault_name, file_path)")?;
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS file_cache_v3 (
                vault_name TEXT NOT NULL,
                path TEXT NOT NULL,
                hash TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                model_id TEXT NOT NULL,
                file_size INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (vault_name, path)
            )
        ")?;
        // file_size may or may not exist in the source file_cache
        // depending on whether create_schema already included it.
        let fc_cols: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(file_cache)")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if fc_cols.iter().any(|c| c == "file_size") {
            conn.execute_batch("
                INSERT INTO file_cache_v3 (vault_name, path, hash, mtime, model_id, file_size)
                SELECT 'default', path, hash, mtime, model_id, file_size FROM file_cache
            ")?;
        } else {
            conn.execute_batch("
                INSERT INTO file_cache_v3 (vault_name, path, hash, mtime, model_id, file_size)
                SELECT 'default', path, hash, mtime, model_id, 0 FROM file_cache
            ")?;
        }
        conn.execute_batch("DROP TABLE file_cache")?;
        conn.execute_batch("ALTER TABLE file_cache_v3 RENAME TO file_cache")?;
        conn.execute_batch("PRAGMA user_version = 3")?;
        conn.execute_batch("COMMIT")?;
    } else {
        // Already partially/fully migrated — just ensure user_version is correct
        conn.execute_batch("PRAGMA user_version = 3")?;
    }
    Ok(())
}
