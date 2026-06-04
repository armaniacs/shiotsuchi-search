use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v10→v11: add vlm_hash column to file_cache for VLM extraction caching.
    let fc_cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(file_cache)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !fc_cols.iter().any(|c| c == "vlm_hash") {
        conn.execute_batch(
            "ALTER TABLE file_cache ADD COLUMN vlm_hash TEXT",
        )?;
    }
    conn.execute_batch("PRAGMA user_version = 11")?;
    Ok(())
}
