use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v3→v4: recreate vec_chunks to ensure FLOAT type.
    // (sqlite-vec 0.1.x does not support FLOAT2/FLOAT4_BINARY.)
    // vec0 is a virtual table, so we must DROP and recreate.
    // Wrapped in a transaction for crash consistency.
    conn.execute_batch("BEGIN TRANSACTION")?;
    conn.execute_batch("DROP TABLE IF EXISTS vec_chunks")?;
    conn.execute_batch("
        CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
            chunk_id  INTEGER PRIMARY KEY,
            embedding FLOAT[1024]
        )
    ")?;
    conn.execute_batch("PRAGMA user_version = 4")?;
    conn.execute_batch("COMMIT")?;
    Ok(())
}
