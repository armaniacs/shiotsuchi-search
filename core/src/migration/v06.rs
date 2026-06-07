use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v5→v6: add tags, frontmatter_date, title columns to chunks table
    super::add_column_if_missing(conn, "chunks", "tags", "TEXT NOT NULL DEFAULT ''")?;
    super::add_column_if_missing(conn, "chunks", "frontmatter_date", "TEXT NOT NULL DEFAULT ''")?;
    super::add_column_if_missing(conn, "chunks", "title", "TEXT NOT NULL DEFAULT ''")?;
    conn.execute_batch("PRAGMA user_version = 6")?;
    Ok(())
}
