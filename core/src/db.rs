use crate::models::{NoteMetadata, VaultStats};
use rusqlite::{params, Connection, Result as SqliteResult};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Note not found: {0}")]
    NotFound(String),
}

/// Manages the SQLite database including FTS5 and metadata tables.
pub struct NoteDatabase {
    conn: Connection,
}

impl NoteDatabase {
    /// Open or create a database at the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    /// Create an in-memory database (for testing).
    pub fn open_in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> SqliteResult<()> {
        // Main FTS5 table for tokenized body search
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                path UNINDEXED,
                title,
                body,
                tokenize='unicode61 remove_diacritics 0'
            )",
            [],
        )?;

        // Metadata table for hash/mtime tracking
        self.conn.execute(
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
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_notes_meta_hash ON notes_meta(hash)",
            [],
        )?;

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

        // Check existing hash
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT hash FROM notes_meta WHERE path = ?1",
                [path],
                |row| row.get(0),
            )
            .ok();

        if let Some(old_hash) = existing {
            if old_hash == hash {
                // Unchanged
                return Ok(false);
            }
            // Update: delete old FTS row first
            self.conn
                .execute("DELETE FROM notes_fts WHERE path = ?1", [path])?;
        }

        // Insert into FTS
        self.conn.execute(
            "INSERT INTO notes_fts (path, title, body) VALUES (?1, ?2, ?3)",
            params![path, title, tokenized_body],
        )?;

        // Upsert metadata
        self.conn.execute(
            "INSERT INTO notes_meta (path, hash, mtime, indexed_at, title)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                hash=excluded.hash,
                mtime=excluded.mtime,
                indexed_at=excluded.indexed_at,
                title=excluded.title",
            params![path, hash, mtime, now, title],
        )?;

        Ok(true)
    }

    /// Get metadata for a specific note.
    pub fn get_metadata(&self, path: &str) -> Result<NoteMetadata, DbError> {
        self.conn
            .query_row(
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
        let mut stmt = self.conn.prepare("SELECT path FROM notes_meta")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    /// Delete a note from the index.
    pub fn delete_note(&self, path: &str) -> SqliteResult<()> {
        self.conn
            .execute("DELETE FROM notes_fts WHERE path = ?1", [path])?;
        self.conn
            .execute("DELETE FROM notes_meta WHERE path = ?1", [path])?;
        Ok(())
    }

    /// Get vault statistics.
    pub fn stats(&self) -> Result<VaultStats, DbError> {
        let total_notes: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM notes_meta", [], |row| row.get(0))?;

        let total_size: usize = self
            .conn
            .query_row(
                "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let last_indexed: Option<i64> = self
            .conn
            .query_row(
                "SELECT MAX(indexed_at) FROM notes_meta",
                [],
                |row| row.get(0),
            )
            .ok();

        let db_path = self.conn.path().map(|p| PathBuf::from(p)).unwrap_or_default();

        Ok(VaultStats {
            total_notes,
            total_size_bytes: total_size,
            last_indexed_at: last_indexed,
            db_path,
        })
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
}
