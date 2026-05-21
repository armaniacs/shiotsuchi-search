use rusqlite::Connection;
use shiotsuchi_core::db::NoteDatabase;
use tempfile::TempDir;

fn create_v1_db(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.execute_batch("
        CREATE VIRTUAL TABLE notes_fts USING fts5(
            path UNINDEXED, title, body,
            tokenize='unicode61 remove_diacritics 0'
        );
        CREATE TABLE notes_meta (
            path TEXT PRIMARY KEY, hash TEXT NOT NULL, mtime INTEGER NOT NULL,
            indexed_at INTEGER NOT NULL, title TEXT
        );
        INSERT INTO notes_meta VALUES ('old.md', 'abc', 1000, 1000, 'Old Note');
        INSERT INTO notes_fts (path, title, body) VALUES ('old.md', 'Old Note', 'body');
        PRAGMA user_version = 1;
    ").unwrap();
}

#[test]
fn migrate_v1_to_v2_drops_old_tables_and_creates_new() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    create_v1_db(&db_path);

    // Opening via NoteDatabase should trigger migration
    let db = NoteDatabase::open(&db_path).unwrap();

    let conn = db.write_conn.borrow();
    // Old tables gone
    let notes_fts_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='notes_fts'",
        [], |r| r.get(0)
    ).unwrap();
    assert_eq!(notes_fts_exists, 0, "notes_fts should be dropped");

    let notes_meta_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='notes_meta'",
        [], |r| r.get(0)
    ).unwrap();
    assert_eq!(notes_meta_exists, 0, "notes_meta should be dropped");

    // New tables present
    let chunks_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name='chunks'",
        [], |r| r.get(0)
    ).unwrap();
    assert_eq!(chunks_exists, 1, "chunks table should exist");

    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 4);
}

#[test]
fn open_fresh_db_has_version_4() {
    let temp = TempDir::new().unwrap();
    let db = NoteDatabase::open(temp.path().join("fresh.db")).unwrap();
    let version: i64 = db.write_conn.borrow()
        .query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 4);
}

#[test]
fn migrate_is_idempotent_when_interrupted() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("interrupted.db");

    // Create a DB with v2 schema but v1 user_version (simulating crash after
    // create_schema but before PRAGMA user_version = 2)
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS file_cache (
                path     TEXT PRIMARY KEY,
                hash     TEXT NOT NULL,
                mtime    INTEGER NOT NULL,
                model_id TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS chunks (
                id                INTEGER PRIMARY KEY,
                file_path         TEXT NOT NULL,
                chunk_index       INTEGER NOT NULL,
                parent_header     TEXT,
                content           TEXT NOT NULL,
                tokenized_content TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON chunks(file_path);
            CREATE VIRTUAL TABLE IF NOT EXISTS fts_chunks USING fts5(
                tokenized_content,
                content='chunks',
                content_rowid='id',
                tokenize='unicode61 remove_diacritics 0'
            );
            PRAGMA user_version = 1;
        ").unwrap();
    }

    // Opening via NoteDatabase should detect version=1, drop tables, and recreate
    let db = NoteDatabase::open(&db_path).unwrap();

    let conn = db.write_conn.borrow();

    // Version should now be 3
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 4, "migration should upgrade to version 4");

    // All expected tables exist

    // Opening AGAIN should be a no-op (version is already 4)
    drop(conn);
    drop(db);
    let db2 = NoteDatabase::open(&db_path).unwrap();
    let version2: i64 = db2.write_conn.borrow()
        .query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version2, 4, "second open should remain at version 4");
}
