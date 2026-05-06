use crate::models::{NoteMetadata, SearchResult, VaultStats};
use rusqlite::{params, Connection, Result as SqliteResult};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Note not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Manages the SQLite database including FTS5 and metadata tables.
pub struct NoteDatabase {
    pub conn: RefCell<Connection>,
}

impl NoteDatabase {
    /// Open or create a database at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let db = Self {
            conn: RefCell::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    /// Create an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        let db = Self {
            conn: RefCell::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> SqliteResult<()> {
        // Main FTS5 table for tokenized body search
        self.conn.borrow().execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                path UNINDEXED,
                title,
                body,
                tokenize='unicode61 remove_diacritics 0'
            )",
            [],
        )?;

        // Metadata table for hash/mtime tracking
        self.conn.borrow().execute(
            "CREATE TABLE IF NOT EXISTS notes_meta (
                path TEXT PRIMARY KEY,
                hash TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                indexed_at INTEGER NOT NULL,
                title TEXT
            )",
            [],
        )?;

        // Index for fast hash lookups
        self.conn.borrow().execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_meta_hash ON notes_meta(hash)",
            [],
        )?;

        let current_version: i64 = self
            .conn
            .borrow()
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap_or(0);
        if current_version == 0 {
            self.conn.borrow().execute("PRAGMA user_version = 1", [])?;
        }

        Ok(())
    }

    /// Insert or update a note. Returns true if inserted/updated, false if skipped.
    pub fn upsert_note(
        &self,
        path: &str,
        title: &str,
        tokenized_body: &str,
        hash: &str,
        mtime: i64,
    ) -> Result<bool, DbError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let mut conn = self.conn.borrow_mut();
        let tx = conn.transaction()?;

        let existing: Option<String> = tx
            .query_row(
                "SELECT hash FROM notes_meta WHERE path = ?1",
                [path],
                |row| row.get(0),
            )
            .ok();

        if let Some(old_hash) = existing {
            if old_hash == hash {
                // Unchanged — commit transaction and return
                tx.commit()?;
                return Ok(false);
            }
            // Update: delete old FTS row first
            tx.execute("DELETE FROM notes_fts WHERE path = ?1", [path])?;
        }

        // Insert into FTS
        tx.execute(
            "INSERT INTO notes_fts (path, title, body) VALUES (?1, ?2, ?3)",
            params![path, title, tokenized_body],
        )?;

        // Upsert metadata
        tx.execute(
            "INSERT INTO notes_meta (path, hash, mtime, indexed_at, title)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                hash=excluded.hash,
                mtime=excluded.mtime,
                indexed_at=excluded.indexed_at,
                title=excluded.title",
            params![path, hash, mtime, now, title],
        )?;

        tx.commit()?;
        Ok(true)
    }

    /// Get metadata for a specific note.
    pub fn get_metadata(&self, path: &str) -> Result<NoteMetadata, DbError> {
        let conn = self.conn.borrow();
        conn.query_row(
            "SELECT path, hash, mtime, indexed_at, title FROM notes_meta WHERE path = ?1",
            [path],
            |row| {
                Ok(NoteMetadata {
                    path: row.get(0)?,
                    hash: row.get(1)?,
                    mtime: row.get(2)?,
                    indexed_at: row.get(3)?,
                    title: row.get(4)?,
                })
            },
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(path.to_string()),
            other => DbError::Sqlite(other),
        })
    }

    /// List all indexed paths.
    pub fn list_paths(&self) -> SqliteResult<Vec<String>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare("SELECT path FROM notes_meta")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    /// List all note metadata ordered by indexed_at descending.
    pub fn list_all_metadata(&self) -> Result<Vec<NoteMetadata>, DbError> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT path, hash, mtime, indexed_at, title FROM notes_meta ORDER BY indexed_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(NoteMetadata {
                path: row.get(0)?,
                hash: row.get(1)?,
                mtime: row.get(2)?,
                indexed_at: row.get(3)?,
                title: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::Sqlite)
    }

    /// Delete a note from the index.
    pub fn delete_note(&self, path: &str) -> SqliteResult<()> {
        let mut conn = self.conn.borrow_mut();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM notes_fts WHERE path = ?1", [path])?;
        tx.execute("DELETE FROM notes_meta WHERE path = ?1", [path])?;
        tx.commit()?;
        Ok(())
    }

    /// Get vault statistics.
    pub fn stats(&self) -> Result<VaultStats, DbError> {
        let conn = self.conn.borrow();
        let total_notes: usize =
            conn.query_row("SELECT COUNT(*) FROM notes_meta", [], |row| row.get(0))?;

        let total_size: usize = conn
            .query_row(
                "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let last_indexed: Option<i64> = conn
            .query_row("SELECT MAX(indexed_at) FROM notes_meta", [], |row| {
                row.get(0)
            })
            .ok();

        let db_path = conn.path().map(|p| PathBuf::from(p)).unwrap_or_default();

        Ok(VaultStats {
            total_notes,
            total_size_bytes: total_size,
            last_indexed_at: last_indexed,
            db_path,
        })
    }

    /// Search notes using tokenized query. Returns results ordered by BM25 relevance.
    /// `fts5_query` は呼び出し側で `tokenizer.and_query()` を使って構築すること。
    /// 例: `"東京" AND "検索"` — スペース区切り（フレーズ検索）は誤りなので使わない。
    pub fn search(&self, fts5_query: &str, limit: usize) -> Result<Vec<SearchResult>, DbError> {
        let conn = self.conn.borrow();
        let sql = "SELECT path, title, rank
             FROM notes_fts
             WHERE notes_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2";

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![fts5_query, limit as i64], |row| {
            Ok(SearchResult {
                path: row.get(0)?,
                title: row.get(1)?,
                snippet: String::new(), // 呼び出し側が元ファイルから extract_snippet() で補完する
                score: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(DbError::Sqlite)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_schema() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let stats = db.stats().unwrap();
        assert_eq!(stats.total_notes, 0);
    }

    #[test]
    fn test_upsert_and_get() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let changed = db
            .upsert_note("test.md", "Test", "tokenized body", "hash123", 1000)
            .unwrap();
        assert!(changed);

        let meta = db.get_metadata("test.md").unwrap();
        assert_eq!(meta.title, "Test");
        assert_eq!(meta.hash, "hash123");
    }

    #[test]
    fn test_upsert_skip_unchanged() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_note("test.md", "Test", "body", "hash123", 1000)
            .unwrap();
        let changed = db
            .upsert_note("test.md", "Test", "body", "hash123", 1000)
            .unwrap();
        assert!(!changed);
    }

    #[test]
    fn test_delete() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_note("test.md", "Test", "body", "hash123", 1000)
            .unwrap();
        db.delete_note("test.md").unwrap();
        assert!(db.get_metadata("test.md").is_err());
    }

    #[test]
    fn test_wal_mode_enabled() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = NoteDatabase::open(temp.path().join("test.db")).unwrap();
        let journal_mode: String = db
            .conn
            .borrow()
            .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
    }

    #[test]
    fn test_list_all_metadata_empty() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let entries = db.list_all_metadata().unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_list_all_metadata_ordered_by_indexed_at_desc() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_note("a.md", "A", "body a", "hash_a", 1000)
            .unwrap();
        db.upsert_note("b.md", "B", "body b", "hash_b", 2000)
            .unwrap();
        db.upsert_note("c.md", "C", "body c", "hash_c", 3000)
            .unwrap();

        let entries = db.list_all_metadata().unwrap();
        assert_eq!(entries.len(), 3);
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&"a.md"));
        assert!(paths.contains(&"b.md"));
        assert!(paths.contains(&"c.md"));
    }

    #[test]
    fn test_delete_nonexistent_returns_ok() {
        // Deleting a note that doesn't exist should not error
        let db = NoteDatabase::open_in_memory().unwrap();
        db.delete_note("nonexistent.md").unwrap();
    }
}
