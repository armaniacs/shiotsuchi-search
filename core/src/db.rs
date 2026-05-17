use crate::models::{Chunk, VaultStats};
use rusqlite::{params, Connection, OpenFlags, Result as SqliteResult};
use sqlite_vec;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Once;
use thiserror::Error;

/// Register the sqlite-vec extension once per process lifetime.
///
/// # Safety
///
/// `sqlite3_auto_extension` expects a C-ABI function pointer. The cast through
/// `*const ()` follows sqlite-vec's documented registration pattern. The
/// `Once` guard ensures the extension is registered exactly once even when
/// both `open()` and `open_in_memory()` are called.
unsafe fn register_vec_extension() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // SAFETY: sqlite-vec's `sqlite3_vec_init` has the correct signature for
        // `sqlite3_auto_extension` (it is `unsafe extern "C" fn(...)` matching
        // the expected `int (*)(sqlite3*, char**, const sqlite3_api_routines*)`).
        // The `transmute` through `*const ()` is the standard pattern from the
        // sqlite-vec crate documentation.
        // SAFETY: The transmute through `*const ()` is the standard pattern from
        // the sqlite-vec crate documentation.
        // In rusqlite 0.39+, sqlite3_auto_extension expects char** (writable) vs older versions
        // which used char*const*. We transmute to match the new signature.
        type AutoExtFn = unsafe extern "C" fn(
            *mut rusqlite::ffi::sqlite3,
            *mut *mut std::os::raw::c_char,
            *const rusqlite::ffi::sqlite3_api_routines,
        ) -> std::os::raw::c_int;
        let func: AutoExtFn =
            std::mem::transmute::<*const (), _>(sqlite_vec::sqlite3_vec_init as *const ());
        rusqlite::ffi::sqlite3_auto_extension(Some(func));
    });
}

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Note not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

/// Manages the SQLite database — write connection for indexer,
/// read-only connection for search (WAL allows concurrent readers).
pub struct NoteDatabase {
    pub write_conn: RefCell<Connection>,
}

impl NoteDatabase {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let is_fresh = !path.as_ref().exists();
        // SAFETY: sqlite-vec extension registration is safe under the Once guard
        // (see register_vec_extension doc-comment).
        unsafe { register_vec_extension(); }
        let conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let db = Self { write_conn: RefCell::new(conn) };
        db.migrate()?;
        #[cfg(unix)]
        if is_fresh {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
                log::warn!("Failed to set DB file permissions to 0o600: {}", e);
            }
            let base = path.as_ref().to_string_lossy();
            for suffix in ["-wal", "-shm"] {
                let companion = PathBuf::from(format!("{}{}", base, suffix));
                if companion.exists() {
                    if let Err(e) = std::fs::set_permissions(&companion, std::fs::Permissions::from_mode(0o600))
                    {
                        log::warn!("Failed to set companion file permissions to 0o600: {}", e);
                    }
                }
            }
        }
        Ok(db)
    }

    pub fn open_in_memory() -> Result<Self, DbError> {
        // SAFETY: sqlite-vec extension registration is safe under the Once guard
        // (see register_vec_extension doc-comment).
        unsafe { register_vec_extension(); }
        let conn = Connection::open_in_memory()?;
        let db = Self { write_conn: RefCell::new(conn) };
        db.migrate()?;
        Ok(db)
    }

    /// Open a read-only connection to an existing DB (for MCP search handler).
    pub fn open_readonly<P: AsRef<Path>>(path: P) -> Result<Connection, DbError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(conn)
    }

    fn migrate(&self) -> Result<(), DbError> {
        let conn = self.write_conn.borrow();
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0);

        if version < 2 {
            // Drop v1 tables if present
            conn.execute_batch("
                DROP TABLE IF EXISTS notes_fts;
                DROP TABLE IF EXISTS notes_meta;
            ")?;
            self.create_schema(&conn)?;
            conn.execute_batch("PRAGMA user_version = 2")?;
        }
        Ok(())
    }

    fn create_schema(&self, conn: &Connection) -> SqliteResult<()> {
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

            CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
                chunk_id  INTEGER PRIMARY KEY,
                embedding FLOAT[1024]
            );
        ")?;
        Ok(())
    }

    /// Insert a batch of chunks for a file. Caller must have deleted old chunks first.
    pub fn insert_chunks(&self, chunks: &[Chunk]) -> Result<Vec<i64>, DbError> {
        let mut conn = self.write_conn.borrow_mut();
        let tx = conn.transaction()?;
        let mut ids = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            tx.execute(
                "INSERT INTO chunks (file_path, chunk_index, parent_header, content, tokenized_content)
                 VALUES (?1,?2,?3,?4,?5)",
                params![chunk.file_path, chunk.chunk_index, chunk.parent_header, chunk.content, chunk.tokenized_content],
            )?;
            let id = tx.last_insert_rowid();
            // FTS insert (external content — fts_chunks maps to chunks.tokenized_content)
            tx.execute(
                "INSERT INTO fts_chunks(rowid, tokenized_content) VALUES (?1, ?2)",
                params![id, chunk.tokenized_content],
            )?;
            ids.push(id);
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Insert embeddings for a batch of (chunk_id, embedding) pairs.
    pub fn insert_embeddings(&self, pairs: &[(i64, Vec<f32>)]) -> Result<(), DbError> {
        let mut conn = self.write_conn.borrow_mut();
        let tx = conn.transaction()?;
        for (chunk_id, embedding) in pairs {
            let blob: Vec<u8> = embedding.iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();
            tx.execute(
                "INSERT INTO vec_chunks(chunk_id, embedding) VALUES (?1, ?2)",
                params![chunk_id, blob],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Delete all chunks (and their FTS/vec entries) for a file path.
    /// fts_chunks is external content (content='chunks'), so normal DELETE works.
    pub fn delete_chunks_for_file(&self, file_path: &str) -> Result<(), DbError> {
        let mut conn = self.write_conn.borrow_mut();
        let tx = conn.transaction()?;

        let ids: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT id FROM chunks WHERE file_path = ?1")?;
            let rows = stmt.query_map([file_path], |r| r.get(0))?;
            rows.collect::<SqliteResult<Vec<_>>>()?
        };
        log::debug!("Deleting {} chunks for {}", ids.len(), file_path);

        for id in &ids {
            log::trace!("  deleting fts_chunks rowid={}", id);
            tx.execute("DELETE FROM fts_chunks WHERE rowid = ?1", [id])?;
            log::trace!("  deleting vec_chunks chunk_id={}", id);
            tx.execute("DELETE FROM vec_chunks WHERE chunk_id = ?1", [id])?;
        }

        log::debug!("  deleting from chunks table");
        tx.execute("DELETE FROM chunks WHERE file_path = ?1", [file_path])?;
        tx.commit()?;
        log::debug!("  committed delete for {}", file_path);
        Ok(())
    }

    /// Upsert file_cache entry.
    pub fn upsert_file_cache(
        &self,
        path: &str,
        hash: &str,
        mtime: i64,
        model_id: &str,
    ) -> Result<(), DbError> {
        self.write_conn.borrow().execute(
            "INSERT INTO file_cache (path, hash, mtime, model_id)
             VALUES (?1,?2,?3,?4)
             ON CONFLICT(path) DO UPDATE SET
                 hash=excluded.hash, mtime=excluded.mtime, model_id=excluded.model_id",
            params![path, hash, mtime, model_id],
        )?;
        Ok(())
    }

    /// Returns the stored hash for a file, or None if not cached.
    pub fn cached_hash(&self, path: &str) -> Result<Option<String>, DbError> {
        let conn = self.write_conn.borrow();
        match conn.query_row(
            "SELECT hash FROM file_cache WHERE path = ?1",
            [path],
            |r| r.get(0),
        ) {
            Ok(h) => Ok(Some(h)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    /// Delete file_cache entry for a file.
    pub fn delete_file_cache(&self, path: &str) -> Result<(), DbError> {
        self.write_conn.borrow().execute(
            "DELETE FROM file_cache WHERE path = ?1",
            [path],
        )?;
        Ok(())
    }

    /// FTS search on fts_chunks. Returns (chunk_id, score) pairs.
    pub fn fts_search(&self, fts5_query: &str, limit: usize) -> Result<Vec<(i64, f64)>, DbError> {
        let conn = self.write_conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT rowid, rank FROM fts_chunks WHERE fts_chunks MATCH ?1 ORDER BY rank LIMIT ?2"
        )?;
        let rows = stmt.query_map(params![fts5_query, limit as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Vector KNN search on vec_chunks. Returns (chunk_id, distance) pairs.
    pub fn vec_search(&self, embedding: &[f32], limit: usize) -> Result<Vec<(i64, f64)>, DbError> {
        let conn = self.write_conn.borrow();
        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let mut stmt = conn.prepare(
            "SELECT chunk_id, distance FROM vec_chunks
             WHERE embedding MATCH ?1 AND k = ?2
             ORDER BY distance"
        )?;
        let rows = stmt.query_map(params![blob, limit as i64], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Fetch chunks by ids, preserving order.
    pub fn get_chunks_by_ids(&self, ids: &[i64]) -> Result<Vec<Chunk>, DbError> {
        if ids.is_empty() { return Ok(vec![]); }
        let conn = self.write_conn.borrow();
        let placeholders: String = ids.iter().enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, file_path, chunk_index, parent_header, content, tokenized_content FROM chunks WHERE id IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let params_vec: Vec<&dyn rusqlite::ToSql> = ids.iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let rows = stmt.query_map(params_vec.as_slice(), |r| {
            Ok(Chunk {
                id: Some(r.get(0)?),
                file_path: r.get(1)?,
                chunk_index: r.get(2)?,
                parent_header: r.get(3)?,
                content: r.get(4)?,
                tokenized_content: r.get(5)?,
            })
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Fetch chunks surrounding a given chunk_id (for MCP get_surrounding_context).
    pub fn get_surrounding_chunks(&self, chunk_id: i64, window: usize) -> Result<Vec<Chunk>, DbError> {
        let conn = self.write_conn.borrow();
        let file_path: String = conn.query_row(
            "SELECT file_path FROM chunks WHERE id = ?1", [chunk_id], |r| r.get(0)
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(chunk_id.to_string()),
            other => DbError::Sqlite(other),
        })?;
        let chunk_index: i64 = conn.query_row(
            "SELECT chunk_index FROM chunks WHERE id = ?1", [chunk_id], |r| r.get(0)
        )?;
        let w = window as i64;
        let mut stmt = conn.prepare(
            "SELECT id, file_path, chunk_index, parent_header, content, tokenized_content FROM chunks
             WHERE file_path = ?1 AND chunk_index BETWEEN ?2 AND ?3
             ORDER BY chunk_index"
        )?;
        let rows = stmt.query_map(params![file_path, chunk_index - w, chunk_index + w], |r| {
            Ok(Chunk {
                id: Some(r.get(0)?),
                file_path: r.get(1)?,
                chunk_index: r.get(2)?,
                parent_header: r.get(3)?,
                content: r.get(4)?,
                tokenized_content: r.get(5)?,
            })
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// List all file paths in file_cache.
    pub fn list_cached_paths(&self) -> Result<Vec<String>, DbError> {
        let conn = self.write_conn.borrow();
        let mut stmt = conn.prepare("SELECT path FROM file_cache")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Vault statistics.
    pub fn stats(&self) -> Result<VaultStats, DbError> {
        let conn = self.write_conn.borrow();
        // rusqlite 0.38+ disabled FromSql for usize by default; retrieve as i64 and cast.
        let total_chunks: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |r| r.get(0))?;
        let total_files: i64 = conn.query_row("SELECT COUNT(*) FROM file_cache", [], |r| r.get(0))?;
        let vec_indexed: i64 = conn.query_row("SELECT COUNT(*) FROM vec_chunks", [], |r| r.get(0))?;
        let total_size: i64 = conn.query_row(
            "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
            [], |r| r.get(0),
        ).unwrap_or(0);
        let last_indexed: Option<i64> = conn.query_row(
            "SELECT MAX(mtime) FROM file_cache", [], |r| r.get(0)
        ).ok();
        let db_path = conn.path().map(PathBuf::from).unwrap_or_default();

        Ok(VaultStats {
            total_chunks: total_chunks as usize,
            total_files: total_files as usize,
            total_size_bytes: total_size as usize,
            last_indexed_at: last_indexed,
            db_path,
            vec_indexed_chunks: vec_indexed as usize,
            embedder_status: String::new(), // filled by caller
        })
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_schema_fresh() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let stats = db.stats().unwrap();
        assert_eq!(stats.total_chunks, 0);
        assert_eq!(stats.total_files, 0);
    }

    #[test]
    fn test_insert_and_delete_chunks() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunks = vec![
            Chunk { id: None, file_path: "a.md".into(), chunk_index: 0, parent_header: None, content: "hello world".into(), tokenized_content: "hello world".into() },
            Chunk { id: None, file_path: "a.md".into(), chunk_index: 1, parent_header: Some("# H1".into()), content: "second chunk".into(), tokenized_content: "second chunk".into() },
        ];
        let ids = db.insert_chunks(&chunks).unwrap();
        assert_eq!(ids.len(), 2);

        db.delete_chunks_for_file("a.md").unwrap();
        let stats = db.stats().unwrap();
        assert_eq!(stats.total_chunks, 0);
    }

    #[test]
    fn test_file_cache_upsert_and_lookup() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("a.md", "hash1", 1000, "none").unwrap();
        assert_eq!(db.cached_hash("a.md").unwrap(), Some("hash1".to_string()));
        // Upsert again with new hash
        db.upsert_file_cache("a.md", "hash2", 2000, "none").unwrap();
        assert_eq!(db.cached_hash("a.md").unwrap(), Some("hash2".to_string()));
        // Unknown path
        assert_eq!(db.cached_hash("missing.md").unwrap(), None);
    }

    #[test]
    fn test_fts_search_finds_inserted_chunk() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunks = vec![
            Chunk { id: None, file_path: "b.md".into(), chunk_index: 0, parent_header: None, content: "search engine test".into(), tokenized_content: "search engine test".into() },
        ];
        db.insert_chunks(&chunks).unwrap();
        let results = db.fts_search("search AND engine", 10).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_get_surrounding_chunks() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunks: Vec<Chunk> = (0..5).map(|i| Chunk {
            id: None, file_path: "c.md".into(), chunk_index: i,
            parent_header: None, content: format!("chunk {}", i),
            tokenized_content: format!("chunk {}", i),
        }).collect();
        let ids = db.insert_chunks(&chunks).unwrap();
        let middle_id = ids[2];
        let surrounding = db.get_surrounding_chunks(middle_id, 1).unwrap();
        assert_eq!(surrounding.len(), 3); // index 1, 2, 3
    }

    #[test]
    fn test_content_roundtrip_via_get_chunks_by_ids() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunks = vec![
            Chunk {
                id: None,
                file_path: "a.md".into(),
                chunk_index: 0,
                parent_header: None,
                content: "Hello world content with unique marker 98765".into(),
                tokenized_content: "Hello world content with unique marker 98765".into(),
            },
            Chunk {
                id: None,
                file_path: "b.md".into(),
                chunk_index: 5,
                parent_header: Some("# Section > Subsection".into()),
                content: "Second chunk with different text ABCDEF".into(),
                tokenized_content: "Second chunk with different text ABCDEF".into(),
            },
        ];
        let ids = db.insert_chunks(&chunks).unwrap();
        assert_eq!(ids.len(), 2);

        let retrieved = db.get_chunks_by_ids(&ids).unwrap();
        assert_eq!(retrieved.len(), 2);

        // Verify field-by-field for each chunk
        // First chunk
        assert_eq!(retrieved[0].file_path, "a.md");
        assert_eq!(retrieved[0].chunk_index, 0);
        assert_eq!(retrieved[0].parent_header, None);
        assert_eq!(retrieved[0].content, "Hello world content with unique marker 98765");
        assert_eq!(retrieved[0].tokenized_content, "Hello world content with unique marker 98765");

        // Second chunk
        assert_eq!(retrieved[1].file_path, "b.md");
        assert_eq!(retrieved[1].chunk_index, 5);
        assert_eq!(retrieved[1].parent_header.as_deref(), Some("# Section > Subsection"));
        assert_eq!(retrieved[1].content, "Second chunk with different text ABCDEF");
        assert_eq!(retrieved[1].tokenized_content, "Second chunk with different text ABCDEF");
    }

    #[test]
    fn test_delete_chunks_removes_fts_entries() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunks = vec![
            Chunk { id: None, file_path: "d.md".into(), chunk_index: 0, parent_header: None, content: "unique token xyz987".into(), tokenized_content: "unique token xyz987".into() },
        ];
        db.insert_chunks(&chunks).unwrap();
        // Verify findable before delete
        assert!(!db.fts_search("xyz987", 10).unwrap().is_empty());
        db.delete_chunks_for_file("d.md").unwrap();
        // After delete, should not be found
        assert!(db.fts_search("xyz987", 10).unwrap().is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn test_db_file_and_companion_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let db = NoteDatabase::open(&db_path).unwrap();

        // Perform a write to trigger WAL creation
        db.upsert_file_cache("test.md", "hash", 1000, "none").unwrap();

        drop(db);

        // Main DB file
        let meta = std::fs::metadata(&db_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600,
            "main DB file should be 0o600");

        // Companion files (-wal, -shm)
        let base = db_path.to_string_lossy();
        for suffix in ["-wal", "-shm"] {
            let companion = std::path::PathBuf::from(format!("{}{}", base, suffix));
            if companion.exists() {
                let meta = std::fs::metadata(&companion).unwrap();
                assert_eq!(meta.permissions().mode() & 0o777, 0o600,
                    "companion file {} should be 0o600", companion.display());
            }
        }
    }

    #[test]
    fn test_wal_mode() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = NoteDatabase::open(temp.path().join("t.db")).unwrap();
        let mode: String = db.write_conn.borrow()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }
}
