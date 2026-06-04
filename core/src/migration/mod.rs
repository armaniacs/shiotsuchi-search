// core/src/migration/mod.rs
use rusqlite::Connection;

mod v02;
mod v03;
mod v04;
mod v05;
mod v06;
mod v07;
mod v08;
mod v09;
mod v10;
mod v11;

/// Run all pending schema migrations.
pub fn run(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Clean up orphaned file_cache_v3 from a previous crash (runs every migration)
    conn.execute_batch("DROP TABLE IF EXISTS file_cache_v3")?;

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
