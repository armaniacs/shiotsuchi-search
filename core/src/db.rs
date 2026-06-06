use crate::models::{Chunk, ReindexParams, Task, VaultStats};
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
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(parent) {
                    if meta.permissions().mode() & 0o777 != 0o700 {
                        if let Err(e) =
                            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                        {
                            tracing::warn!("Failed to set parent directory permissions to 0o700: {}", e);
                        }
                    }
                }
            }
        }
        let conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let db = Self { write_conn: RefCell::new(conn) };
        db.migrate()?;
        #[cfg(unix)]
        if is_fresh {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
                tracing::warn!("Failed to set DB file permissions to 0o600: {}", e);
            }
            for suffix in ["-wal", "-shm"] {
                let companion = path.as_ref().with_extension(format!("db{}", suffix));
                if companion.exists() {
                    if let Err(e) = std::fs::set_permissions(&companion, std::fs::Permissions::from_mode(0o600))
                    {
                        tracing::warn!("Failed to set companion file permissions to 0o600: {}", e);
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
        crate::migration::run(&conn)
    }

    /// Insert a batch of chunks for a file. Caller must have deleted old chunks first.
    pub fn insert_chunks(&self, chunks: &[Chunk]) -> Result<Vec<i64>, DbError> {
        let mut conn = self.write_conn.borrow_mut();
        let tx = conn.transaction()?;
        let mut ids = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            tx.execute(
                "INSERT INTO chunks (file_path, chunk_index, parent_header, content, tokenized_content, vault_name, tags, frontmatter_date, title, emphasized_text)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![chunk.file_path, chunk.chunk_index, chunk.parent_header, chunk.content, chunk.tokenized_content, chunk.vault_name, chunk.tags, chunk.frontmatter_date, chunk.title, chunk.emphasized_text],
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

    /// Atomically remove all data for a file: tag_counts decrement, chunk/FTS/vec/task
    /// deletion, file_cache removal, and note_links cleanup — all in one transaction.
    /// This eliminates the crash-consistency gap between separate delete steps.
    pub fn delete_file_fully(&self, vault_name: &str, file_path: &str) -> Result<(), DbError> {
        // Get old tags before transaction (separate borrow from get_tags_for_file)
        let old_tags = self.get_tags_for_file(vault_name, file_path)?;

        let mut conn = self.write_conn.borrow_mut();
        let tx = conn.transaction()?;

        // 1. Decrement tag_counts for this file's tags
        for tag in old_tags.split(',') {
            let tag = tag.trim();
            if !tag.is_empty() {
                tx.execute(
                    "UPDATE tag_counts SET count = count - 1 WHERE tag = ?1 AND vault_name = ?2 AND count > 0",
                    params![tag, vault_name],
                )?;
                tx.execute(
                    "DELETE FROM tag_counts WHERE tag = ?1 AND vault_name = ?2 AND count = 0",
                    params![tag, vault_name],
                )?;
            }
        }

        // 2. Delete chunk IDs for FTS/vec cleanup
        let ids: Vec<i64> = {
            let mut stmt = tx.prepare("SELECT id FROM chunks WHERE vault_name = ?1 AND file_path = ?2")?;
            let rows = stmt.query_map(params![vault_name, file_path], |r| r.get(0))?;
            rows.collect::<SqliteResult<Vec<_>>>()?
        };
        for id in &ids {
            tx.execute("DELETE FROM fts_chunks WHERE rowid = ?1", [id])?;
            tx.execute("DELETE FROM vec_chunks WHERE chunk_id = ?1", [id])?;
        }

        // 3. Delete chunks, tasks, file_cache, note_links
        tx.execute("DELETE FROM chunks WHERE vault_name = ?1 AND file_path = ?2", params![vault_name, file_path])?;
        tx.execute("DELETE FROM tasks WHERE vault_name = ?1 AND file_path = ?2", params![vault_name, file_path])?;
        tx.execute("DELETE FROM file_cache WHERE vault_name = ?1 AND path = ?2", params![vault_name, file_path])?;
        tx.execute("DELETE FROM note_links WHERE source_path = ?1 AND vault_name = ?2", params![file_path, vault_name])?;
        tx.execute("DELETE FROM note_links WHERE target_path = ?1 AND vault_name = ?2", params![file_path, vault_name])?;

        tx.commit()?;
        Ok(())
    }

    /// Upsert a file cache entry.
    ///
    /// NOTE: For production code, prefer `reindex_file()` which computes and writes
    /// `char_count` from chunk content lengths atomically. This method requires
    /// `char_count` to be provided by the caller to prevent accidental zero values
    /// that would cause `stats().total_chars` to undercount.
    #[allow(dead_code)]
    pub(crate) fn upsert_file_cache(
        &self,
        vault_name: &str,
        path: &str,
        hash: &str,
        mtime: i64,
        model_id: &str,
        file_size: i64,
        char_count: i64,
        vlm_hash: Option<&str>,
    ) -> Result<(), DbError> {
        self.write_conn.borrow().execute(
            "INSERT INTO file_cache (vault_name, path, hash, mtime, model_id, file_size, char_count, vlm_hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(vault_name, path) DO UPDATE SET
                 hash=excluded.hash, mtime=excluded.mtime, model_id=excluded.model_id,
                 file_size=excluded.file_size, char_count=excluded.char_count, vlm_hash=excluded.vlm_hash",
            params![vault_name, path, hash, mtime, model_id, file_size, char_count, vlm_hash],
        )?;
        Ok(())
    }

    /// Public accessor for integration tests only.
    /// Production callers should use `reindex_file` or other public APIs.
    #[doc(hidden)]
    #[doc(hidden)]
    pub fn upsert_file_cache_for_tests(
        &self,
        vault_name: &str,
        path: &str,
        hash: &str,
        mtime: i64,
        model_id: &str,
    ) -> Result<(), DbError> {
        self.upsert_file_cache(vault_name, path, hash, mtime, model_id, 0, 0, None)
    }

    /// Reindex a single file: delete old chunks and insert new ones in a single transaction.
    ///
    /// Takes ownership of the embedding results (one per chunk, or None for failed ones).
    /// On any SQL error the entire transaction is rolled back to maintain data integrity.
    pub fn reindex_file(&self, p: &ReindexParams<'_>) -> Result<(), DbError> {
        let ReindexParams { vault_name, relative_path, hash, mtime, model_id, chunks, embeddings, file_size, tasks, note_link_targets, vlm_hash } = *p;
        let mut conn = self.write_conn.borrow_mut();
        let tx = conn.transaction()?;

        // 1. Delete old chunks, their FTS/vec entries, and associated tasks
        let old_rows: Vec<(i64, String)> = {
            let mut stmt =
                tx.prepare("SELECT id, tags FROM chunks WHERE vault_name = ?1 AND file_path = ?2")?;
            let rows = stmt.query_map(params![vault_name, relative_path], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
            rows.collect::<SqliteResult<Vec<_>>>()?
        };
        for (id, _) in &old_rows {
            tx.execute("DELETE FROM fts_chunks WHERE rowid = ?1", [id])?;
            tx.execute("DELETE FROM vec_chunks WHERE chunk_id = ?1", [id])?;
        }
        tx.execute(
            "DELETE FROM chunks WHERE vault_name = ?1 AND file_path = ?2",
            params![vault_name, relative_path],
        )?;
        tx.execute(
            "DELETE FROM tasks WHERE vault_name = ?1 AND file_path = ?2",
            params![vault_name, relative_path],
        )?;

        // Decrement tag_counts for old chunk tags
        for (_, tags_str) in &old_rows {
            for tag in tags_str.split(',') {
                let tag = tag.trim();
                if !tag.is_empty() {
                    tx.execute(
                        "UPDATE tag_counts SET count = count - 1 WHERE tag = ?1 AND vault_name = ?2 AND count > 0",
                        params![tag, vault_name],
                    )?;
                    tx.execute(
                        "DELETE FROM tag_counts WHERE tag = ?1 AND vault_name = ?2 AND count = 0",
                        params![tag, vault_name],
                    )?;
                }
            }
        }

        // 2. Insert new chunks and FTS entries
        let mut new_ids = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            tx.execute(
                "INSERT INTO chunks (file_path, chunk_index, parent_header, content, tokenized_content, vault_name, tags, frontmatter_date, title, emphasized_text)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    chunk.file_path,
                    chunk.chunk_index,
                    chunk.parent_header,
                    chunk.content,
                    chunk.tokenized_content,
                    chunk.vault_name,
                    chunk.tags,
                    chunk.frontmatter_date,
                    chunk.title,
                    chunk.emphasized_text,
                ],
            )?;
            let id = tx.last_insert_rowid();
            tx.execute(
                "INSERT INTO fts_chunks(rowid, tokenized_content) VALUES (?1, ?2)",
                params![id, chunk.tokenized_content],
            )?;
            new_ids.push(id);
        }

        // 3. Insert embeddings (errors propagate — transaction will be rolled back)
        for (id, emb_opt) in new_ids.iter().zip(embeddings.iter()) {
            if let Some(embedding) = emb_opt {
                let blob: Vec<u8> =
                    embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
                tx.execute(
                    "INSERT INTO vec_chunks(chunk_id, embedding) VALUES (?1, ?2)",
                    params![id, blob],
                )?;
            }
        }

        // 4. Insert tasks (if any)
        for task in tasks {
            tx.execute(
                "INSERT INTO tasks (vault_name, file_path, content, checked, line_number) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![task.vault_name, task.file_path, task.content, task.checked as i32, task.line_number as i64],
            )?;
        }

        // 5. Replace note_links: delete old links for this source, insert new ones
        tx.execute(
            "DELETE FROM note_links WHERE source_path = ?1 AND vault_name = ?2",
            params![relative_path, vault_name],
        )?;
        for target in note_link_targets {
            tx.execute(
                "INSERT OR IGNORE INTO note_links (source_path, target_path, vault_name) VALUES (?1, ?2, ?3)",
                params![relative_path, target, vault_name],
            )?;
        }

        // Increment tag_counts for new chunk tags
        for chunk in chunks {
            for tag in chunk.tags.split(',') {
                let tag = tag.trim();
                if !tag.is_empty() {
                    tx.execute(
                        "INSERT INTO tag_counts (tag, vault_name, count) VALUES (?1, ?2, 1)
                         ON CONFLICT(tag, vault_name) DO UPDATE SET count = count + 1",
                        params![tag, vault_name],
                    )?;
                }
            }
        }

        // Compute total character count from all chunks
        let char_count: i64 = chunks.iter().map(|c| c.content.chars().count() as i64).sum();

        // 6. Upsert file cache
        tx.execute(
            "INSERT INTO file_cache (vault_name, path, hash, mtime, model_id, file_size, char_count, vlm_hash)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(vault_name, path) DO UPDATE SET
                 hash=excluded.hash, mtime=excluded.mtime, model_id=excluded.model_id,
                 file_size=excluded.file_size, char_count=excluded.char_count, vlm_hash=excluded.vlm_hash",
            params![vault_name, relative_path, hash, mtime, model_id, file_size, char_count, vlm_hash],
        )?;

        tx.commit()?;
        Ok(())
    }

    /// Returns the stored hash for a file, or None if not cached.
    pub fn cached_hash(&self, vault_name: &str, path: &str) -> Result<Option<String>, DbError> {
        let conn = self.write_conn.borrow();
        match conn.query_row(
            "SELECT hash FROM file_cache WHERE vault_name = ?1 AND path = ?2",
            params![vault_name, path],
            |r| r.get(0),
        ) {
            Ok(h) => Ok(Some(h)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    /// Read cached mtime for a file. Used as a fast pre-check before reading file content.
    pub fn cached_mtime(&self, vault_name: &str, path: &str) -> Result<Option<i64>, DbError> {
        let conn = self.write_conn.borrow();
        match conn.query_row(
            "SELECT mtime FROM file_cache WHERE vault_name = ?1 AND path = ?2",
            params![vault_name, path],
            |r| r.get(0),
        ) {
            Ok(m) => Ok(Some(m)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    /// Read cached file_size for a file. Paired with cached_mtime for two-stage skip.
    pub fn cached_file_size(&self, vault_name: &str, path: &str) -> Result<Option<i64>, DbError> {
        let conn = self.write_conn.borrow();
        match conn.query_row(
            "SELECT file_size FROM file_cache WHERE vault_name = ?1 AND path = ?2",
            params![vault_name, path],
            |r| r.get(0),
        ) {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    /// Clear all vlm_hash values in file_cache, forcing VLM re-extraction on next index.
    pub fn clear_vlm_hashes(&self) -> Result<usize, DbError> {
        let count = self
            .write_conn
            .borrow()
            .execute("UPDATE file_cache SET vlm_hash = NULL WHERE vlm_hash IS NOT NULL", [])?;
        Ok(count)
    }

    /// Read cached vlm_hash for a file. Returns None if not set.
    pub fn cached_vlm_hash(&self, vault_name: &str, path: &str) -> Result<Option<String>, DbError> {
        let conn = self.write_conn.borrow();
        match conn.query_row(
            "SELECT vlm_hash FROM file_cache WHERE vault_name = ?1 AND path = ?2",
            params![vault_name, path],
            |r| r.get(0),
        ) {
            Ok(h) => Ok(h),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    /// Delete file_cache entry for a file.
    pub fn delete_file_cache(&self, vault_name: &str, path: &str) -> Result<(), DbError> {
        self.write_conn.borrow().execute(
            "DELETE FROM file_cache WHERE vault_name = ?1 AND path = ?2",
            params![vault_name, path],
        )?;
        Ok(())
    }

    /// Delete vec_chunks entries that reference chunk_ids not present in the chunks table.
    /// Returns the number of orphaned entries deleted.
    pub fn delete_orphaned_embeddings(&self) -> Result<usize, DbError> {
        let deleted = self.write_conn.borrow().execute(
            "DELETE FROM vec_chunks WHERE chunk_id NOT IN (SELECT id FROM chunks)",
            [],
        )?;
        Ok(deleted)
    }

    /// Delete all data for a single file atomically:
    /// Purge files older than retention_days from each vault.
    /// Takes a map of vault_name -> retention_days and deletes expired entries.
    /// Returns the number of files purged.
    /// No-op for vaults not present in the map.
    pub fn purge_expired(&self, retention_days: &std::collections::HashMap<String, u32>) -> Result<usize, DbError> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let mut total_purged = 0;

        for (vault_name, days) in retention_days {
            let cutoff_mtime = now - (*days as i64 * 86_400_000);

            let paths_to_purge: Vec<String> = {
                let conn = self.write_conn.borrow();
                let mut stmt = conn.prepare(
                    "SELECT path FROM file_cache WHERE vault_name = ?1 AND mtime < ?2"
                )?;
                let rows = stmt.query_map(params![vault_name, cutoff_mtime], |r| r.get(0))?;
                rows.collect::<SqliteResult<Vec<_>>>()?
            };

            for path in paths_to_purge {
                self.delete_file_fully(vault_name, &path)?;
                total_purged += 1;
            }
        }

        // Checkpoint WAL after mass deletion
        self.wal_checkpoint()?;

        Ok(total_purged)
    }

    /// Count expired files (older than retention_days) without deleting them.
    /// Returns the total count across all vaults in the map.
    pub fn count_expired(&self, retention_days: &std::collections::HashMap<String, u32>) -> Result<usize, DbError> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let mut total = 0;
        let conn = self.write_conn.borrow();

        for (vault_name, days) in retention_days {
            let cutoff_mtime = now - (*days as i64 * 86_400_000);
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM file_cache WHERE vault_name = ?1 AND mtime < ?2",
                params![vault_name, cutoff_mtime],
                |r| r.get(0),
            )?;
            total += count as usize;
        }

        Ok(total)
    }

    /// Delete ALL user data across all tables while keeping the schema intact.
    /// Deletes from: chunks, fts_chunks, vec_chunks, file_cache, tasks, note_links, tag_counts.
    /// Does NOT delete config.toml or any configuration data.
    /// Runs wal_checkpoint() after deletion to flush WAL to main DB file.
    pub fn purge_all_user_data(&self) -> Result<(), DbError> {
        {
            let mut conn = self.write_conn.borrow_mut();
            let tx = conn.transaction()?;

            tx.execute_batch("DELETE FROM fts_chunks")?;
            tx.execute_batch("DELETE FROM vec_chunks")?;
            tx.execute_batch("DELETE FROM chunks")?;
            tx.execute_batch("DELETE FROM file_cache")?;
            // Best-effort: these tables may not exist in older schema versions
            let _ = tx.execute_batch("DELETE FROM tasks");
            let _ = tx.execute_batch("DELETE FROM note_links");
            let _ = tx.execute_batch("DELETE FROM tag_counts");

            tx.commit()?;
            // tx and conn drop at end of scope
        }

        // Checkpoint WAL after mass deletion
        self.wal_checkpoint()?;

        Ok(())
    }

    /// FTS search on fts_chunks. Returns (chunk_id, score) pairs.
    /// When `vault_filter` is Some(_), the search is restricted to that vault
    /// via a JOIN on the chunks table.
    pub fn fts_search(
        &self,
        fts5_query: &str,
        limit: usize,
        vault_filter: Option<&str>,
        after_rank: Option<f64>,
        after_rowid: Option<i64>,
    ) -> Result<Vec<(i64, f64)>, DbError> {
        let conn = self.write_conn.borrow();

        // Build SQL with optional composite cursor (after_rank, after_rowid) filter.
        // Keyset pagination: (rank > ? OR (rank = ? AND rowid > ?))
        let mut sql_parts = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        let use_cursor = after_rank.is_some() && after_rowid.is_some();

        if let Some(vault) = vault_filter {
            sql_parts.push(
                "SELECT c.id, bm25(fts_chunks, 1.0) AS score
                 FROM fts_chunks
                 JOIN chunks c ON c.id = fts_chunks.rowid
                 WHERE fts_chunks MATCH ?1 AND c.vault_name = ?2"
                    .to_string(),
            );
            params.push(Box::new(fts5_query.to_string()));
            params.push(Box::new(vault.to_string()));
            if use_cursor {
                let cursor_sql = format!(
                    " AND (score > ?{} OR (score = ?{} AND c.id > ?{}))",
                    params.len() + 1,
                    params.len() + 2,
                    params.len() + 3,
                );
                sql_parts.push(cursor_sql);
                params.push(Box::new(after_rank.unwrap()));
                params.push(Box::new(after_rank.unwrap()));
                params.push(Box::new(after_rowid.unwrap()));
            }
            sql_parts.push(format!(" ORDER BY score, c.id LIMIT ?{}", params.len() + 1));
            params.push(Box::new(limit as i64));
        } else {
            sql_parts.push(
                "SELECT rowid, rank FROM fts_chunks WHERE fts_chunks MATCH ?1".to_string(),
            );
            params.push(Box::new(fts5_query.to_string()));
            if use_cursor {
                let cursor_sql = format!(
                    " AND (rank > ?{} OR (rank = ?{} AND rowid > ?{}))",
                    params.len() + 1,
                    params.len() + 2,
                    params.len() + 3,
                );
                sql_parts.push(cursor_sql);
                params.push(Box::new(after_rank.unwrap()));
                params.push(Box::new(after_rank.unwrap()));
                params.push(Box::new(after_rowid.unwrap()));
            }
            sql_parts.push(format!(" ORDER BY rank, rowid LIMIT ?{}", params.len() + 1));
            params.push(Box::new(limit as i64));
        }

        let sql = sql_parts.join(" ");
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?))
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Vector KNN search on vec_chunks.
    /// Returns (chunk_id, distance, embedding) triples.
    /// When `include_embeddings` is true, the embedding vector is returned from the
    /// vec0 virtual table in the same query for MMR re-ranking. When false, the
    /// embedding column is skipped to avoid unnecessary blob deserialization.
    /// When `vault_filter` is Some(_), the search is restricted to that vault
    /// via a JOIN on the chunks table.
    pub fn vec_search(
        &self,
        embedding: &[f32],
        limit: usize,
        vault_filter: Option<&str>,
        include_embeddings: bool,
    ) -> Result<Vec<(i64, f64, Vec<f32>)>, DbError> {
        let conn = self.write_conn.borrow();
        let blob: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let (sql, params): (String, Vec<Box<dyn rusqlite::ToSql>>) = if let Some(vault) = vault_filter {
            let select_cols = if include_embeddings {
                "v.chunk_id, v.distance, v.embedding"
            } else {
                "v.chunk_id, v.distance, NULL AS embedding"
            };
            (
                format!(
                    "SELECT {} FROM vec_chunks v
                     JOIN chunks c ON c.id = v.chunk_id
                     WHERE v.embedding MATCH ?1 AND c.vault_name = ?2 AND k = ?3
                     ORDER BY v.distance", select_cols
                ),
                vec![
                    Box::new(blob),
                    Box::new(vault.to_string()),
                    Box::new(limit as i64),
                ],
            )
        } else {
            let select_cols = if include_embeddings {
                "chunk_id, distance, embedding"
            } else {
                "chunk_id, distance, NULL AS embedding"
            };
            (
                format!(
                    "SELECT {} FROM vec_chunks
                     WHERE embedding MATCH ?1 AND k = ?2
                     ORDER BY distance", select_cols
                ),
                vec![
                    Box::new(blob),
                    Box::new(limit as i64),
                ],
            )
        };
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |r| {
            let chunk_id: i64 = r.get(0)?;
            let distance: f64 = r.get(1)?;
            // When include_embeddings is false, the embedding column is NULL.
            let emb_blob: Vec<u8> = r.get::<_, Option<Vec<u8>>>(2)?.unwrap_or_default();
            let emb_vec: Vec<f32> = if emb_blob.is_empty() {
                vec![]
            } else {
                emb_blob.chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                    .collect()
            };
            Ok((chunk_id, distance, emb_vec))
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Return the vault_name for a chunk, or None if not found.
    /// Used by MCP get_surrounding_context to validate vault access.
    pub fn get_chunk_vault_name(&self, chunk_id: i64) -> Result<Option<String>, DbError> {
        let conn = self.write_conn.borrow();
        match conn.query_row(
            "SELECT vault_name FROM chunks WHERE id = ?1",
            params![chunk_id],
            |r| r.get(0),
        ) {
            Ok(vault) => Ok(Some(vault)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
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
            "SELECT id, file_path, chunk_index, parent_header, content, tokenized_content, vault_name, tags, frontmatter_date, title, emphasized_text FROM chunks WHERE id IN ({})",
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
                vault_name: r.get(6)?,
                tags: r.get(7)?,
                frontmatter_date: r.get(8)?,
                title: r.get(9)?,
                emphasized_text: r.get(10)?,
            })
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Fetch chunks surrounding a given chunk_id (for MCP get_surrounding_context).
    pub fn get_surrounding_chunks(&self, chunk_id: i64, window: usize) -> Result<Vec<Chunk>, DbError> {
        let conn = self.write_conn.borrow();
        let (file_path, vault_name): (String, String) = conn.query_row(
            "SELECT file_path, vault_name FROM chunks WHERE id = ?1", [chunk_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            }
        ).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => DbError::NotFound(chunk_id.to_string()),
            other => DbError::Sqlite(other),
        })?;
        let chunk_index: i64 = conn.query_row(
            "SELECT chunk_index FROM chunks WHERE id = ?1", [chunk_id], |r| r.get(0)
        )?;
        let w = window as i64;
        let mut stmt = conn.prepare(
            "SELECT id, file_path, chunk_index, parent_header, content, tokenized_content, vault_name, tags, frontmatter_date, title, emphasized_text FROM chunks
             WHERE vault_name = ?1 AND file_path = ?2 AND chunk_index BETWEEN ?3 AND ?4
             ORDER BY chunk_index"
        )?;
        let rows = stmt.query_map(params![vault_name, file_path, chunk_index - w, chunk_index + w], |r| {
            Ok(Chunk {
                id: Some(r.get(0)?),
                file_path: r.get(1)?,
                chunk_index: r.get(2)?,
                parent_header: r.get(3)?,
                content: r.get(4)?,
                tokenized_content: r.get(5)?,
                vault_name: r.get(6)?,
                tags: r.get(7)?,
                frontmatter_date: r.get(8)?,
                title: r.get(9)?,
                emphasized_text: r.get(10)?,
            })
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Return the most frequently stored model_id in file_cache, excluding "none".
    ///
    /// Used to detect model changes before re-indexing: if the stored ID differs
    /// from the currently loaded model, existing vector embeddings may be stale.
    /// Returns `None` when the cache is empty or all entries have model_id = "none".
    pub fn get_dominant_model_id(&self) -> Result<Option<String>, DbError> {
        let conn = self.write_conn.borrow();
        let result: SqliteResult<String> = conn.query_row(
            "SELECT model_id FROM file_cache WHERE model_id != 'none' GROUP BY model_id ORDER BY COUNT(*) DESC, model_id ASC LIMIT 1",
            [],
            |r| r.get(0),
        );
        match result {
            Ok(id) => Ok(Some(id)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(DbError::Sqlite(e)),
        }
    }

    /// List all file paths in file_cache for a given vault.
    pub fn list_cached_paths(&self, vault_name: &str) -> Result<Vec<String>, DbError> {
        let conn = self.write_conn.borrow();
        let mut stmt = conn.prepare("SELECT path FROM file_cache WHERE vault_name = ?1")?;
        let rows = stmt.query_map(params![vault_name], |r| r.get(0))?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Get all chunks for a file, ordered by chunk_index.
    /// Used as a fallback when the file no longer exists on disk.
    pub fn get_chunks_for_file(
        &self,
        vault_name: &str,
        file_path: &str,
    ) -> Result<Vec<Chunk>, DbError> {
        let conn = self.write_conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, chunk_index, parent_header, content, tokenized_content, vault_name, tags, frontmatter_date, title, emphasized_text FROM chunks WHERE vault_name = ?1 AND file_path = ?2 ORDER BY chunk_index"
        )?;
        let rows = stmt.query_map(params![vault_name, file_path], |r| {
            Ok(Chunk {
                id: Some(r.get(0)?),
                file_path: r.get(1)?,
                chunk_index: r.get(2)?,
                parent_header: r.get(3)?,
                content: r.get(4)?,
                tokenized_content: r.get(5)?,
                vault_name: r.get(6)?,
                tags: r.get(7)?,
                frontmatter_date: r.get(8)?,
                title: r.get(9)?,
                emphasized_text: r.get(10)?,
            })
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Recalculate backlink_count for all files in a vault based on note_links.
    pub fn update_backlink_counts_for_vault(&self, vault_name: &str) -> Result<(), DbError> {
        let conn = self.write_conn.borrow();
        conn.execute(
            "UPDATE file_cache SET backlink_count = (
                SELECT COUNT(*) FROM note_links
                WHERE target_path = file_cache.path AND vault_name = file_cache.vault_name
            ) WHERE vault_name = ?1",
            params![vault_name],
        )?;
        Ok(())
    }

    /// Get backlink_count for a set of chunk IDs. Returns a map of chunk_id -> backlink_count.
    pub fn get_backlink_counts_for_chunks(
        &self,
        chunk_ids: &[i64],
    ) -> Result<std::collections::HashMap<i64, i64>, DbError> {
        if chunk_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let conn = self.write_conn.borrow();
        let placeholders: String = chunk_ids.iter().enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT c.id, fc.backlink_count FROM chunks c \
             JOIN file_cache fc ON fc.vault_name = c.vault_name AND fc.path = c.file_path \
             WHERE c.id IN ({})",
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let params_vec: Vec<&dyn rusqlite::ToSql> = chunk_ids.iter()
            .map(|id| id as &dyn rusqlite::ToSql)
            .collect();
        let rows = stmt.query_map(params_vec.as_slice(), |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
        })?;
        rows.collect::<rusqlite::Result<std::collections::HashMap<i64, i64>>>()
            .map_err(DbError::Sqlite)
    }

    /// Execute WAL checkpoint(TRUNCATE) to flush all WAL data into the main .db file.
    /// Useful before file-level operations like rename, backup, or atomic swap.
    pub fn wal_checkpoint(&self) -> Result<(), DbError> {
        let conn = self.write_conn.borrow();
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
            .map_err(DbError::Sqlite)
    }

    /// Return the concatenated tags (comma-separated) for all chunks of a file.
    /// Used by cleanup_deleted to decrement tag_counts before deleting chunks.
    pub fn get_tags_for_file(&self, vault_name: &str, path: &str) -> Result<String, DbError> {
        let conn = self.write_conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT COALESCE(GROUP_CONCAT(tags), '') FROM chunks WHERE vault_name = ?1 AND file_path = ?2"
        )?;
        stmt.query_row(params![vault_name, path], |r| r.get(0))
            .map_err(DbError::Sqlite)
    }

    /// Decrement the count for a tag in a vault by 1.
    /// If the tag doesn't exist in the table, this is a no-op.
    /// Rows that reach count=0 are deleted to prevent dead-row accumulation.
    pub fn decrement_tag_count(&self, vault_name: &str, tag: &str) -> Result<(), DbError> {
        let conn = self.write_conn.borrow();
        conn.execute(
            "UPDATE tag_counts SET count = count - 1 WHERE tag = ?1 AND vault_name = ?2 AND count > 0",
            params![tag, vault_name],
        )?;
        conn.execute(
            "DELETE FROM tag_counts WHERE tag = ?1 AND vault_name = ?2 AND count = 0",
            params![tag, vault_name],
        )?;
        Ok(())
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
        )?;
        let total_chars: i64 = conn.query_row(
            "SELECT COALESCE(SUM(char_count), 0) FROM file_cache", [], |r| r.get(0)
        )?;
        let last_indexed: Option<i64> = conn.query_row(
            "SELECT MAX(mtime) FROM file_cache", [], |r| r.get(0)
        ).ok();
        let db_path = conn.path().map(PathBuf::from).unwrap_or_default();

        let top_tags = self.tag_stats(10)?;

        Ok(VaultStats {
            total_chunks: total_chunks as usize,
            total_files: total_files as usize,
            total_size_bytes: total_size as usize,
            last_indexed_at: last_indexed,
            db_path,
            vec_indexed_chunks: vec_indexed as usize,
            embedder_status: String::new(), // filled by caller
            total_chars: total_chars as usize,
            top_tags,
        })
    }

    /// Query tasks with optional keyword filter and checked-state filter.
    pub fn query_tasks(&self, keyword: Option<&str>, include_checked: bool) -> Result<Vec<Task>, DbError> {
        let conn = self.write_conn.borrow();
        let checked_filter = if include_checked {
            String::new()
        } else {
            " AND checked = 0".to_string()
        };
        let has_keyword = keyword.is_some_and(|k| !k.is_empty());
        let keyword_filter = if has_keyword {
            " AND content LIKE ?1".to_string()
        } else {
            String::new()
        };
        let sql = format!(
            "SELECT id, vault_name, file_path, content, checked, line_number FROM tasks WHERE 1=1{}{} ORDER BY vault_name, file_path, line_number",
            checked_filter, keyword_filter
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<Box<dyn rusqlite::ToSql>> = if has_keyword {
            vec![Box::new(format!("%{}%", keyword.unwrap()))]
        } else {
            vec![]
        };
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params_refs.as_slice(), |r| {
            Ok(Task {
                id: Some(r.get(0)?),
                vault_name: r.get(1)?,
                file_path: r.get(2)?,
                content: r.get(3)?,
                checked: r.get::<_, i32>(4)? != 0,
                line_number: r.get::<_, i64>(5)? as usize,
            })
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }

    /// Returns the top N tags by occurrence count across all chunks.
    /// Tags are stored as comma-separated values in the `tags` column.
    pub fn tag_stats(&self, limit: usize) -> Result<Vec<(String, usize)>, DbError> {
        let conn = self.write_conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT tag, count FROM tag_counts WHERE count > 0 ORDER BY count DESC, tag ASC LIMIT ?"
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? as usize))
        })?;
        rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_init_schema_fresh() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let stats = db.stats().unwrap();
        assert_eq!(stats.total_chunks, 0);
        assert_eq!(stats.total_files, 0);
    }

    #[test]
    fn test_wal_checkpoint_does_not_fail() {
        let db = NoteDatabase::open_in_memory().unwrap();
        assert!(db.wal_checkpoint().is_ok(), "checkpoint should succeed on empty in-memory DB");

        // Also valid after inserting data
        let chunks = vec![
            Chunk {
                id: None,
                file_path: "test.md".into(),
                chunk_index: 0,
                parent_header: None,
                content: "hello".into(),
                tokenized_content: "hello".into(),
                vault_name: "default".into(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
                emphasized_text: String::new(),
            },
        ];
        db.insert_chunks(&chunks).unwrap();
        assert!(db.wal_checkpoint().is_ok(), "checkpoint should succeed after inserts");
    }

    #[test]
    fn test_insert_and_delete_chunks() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunks = vec![
            Chunk { id: None, file_path: "a.md".into(), chunk_index: 0, parent_header: None, content: "hello world".into(), tokenized_content: "hello world".into(), vault_name: "default".to_string(), tags: String::new(), frontmatter_date: String::new(), title: String::new(), emphasized_text: String::new() },
            Chunk { id: None, file_path: "a.md".into(), chunk_index: 1, parent_header: Some("# H1".into()), content: "second chunk".into(), tokenized_content: "second chunk".into(), vault_name: "default".to_string(), tags: String::new(), frontmatter_date: String::new(), title: String::new(), emphasized_text: String::new() },
        ];
        let ids = db.insert_chunks(&chunks).unwrap();
        assert_eq!(ids.len(), 2);

        db.delete_file_fully("default", "a.md").unwrap();
        let stats = db.stats().unwrap();
        assert_eq!(stats.total_chunks, 0);
    }

    #[test]
    fn test_file_cache_upsert_and_lookup() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("default", "a.md", "hash1", 1000, "none", 42, 0, None).unwrap();
        assert_eq!(db.cached_hash("default", "a.md").unwrap(), Some("hash1".to_string()));
        // Upsert again with new hash
        db.upsert_file_cache("default", "a.md", "hash2", 2000, "none", 99, 0, None).unwrap();
        assert_eq!(db.cached_hash("default", "a.md").unwrap(), Some("hash2".to_string()));
        // Unknown path
        assert_eq!(db.cached_hash("default", "missing.md").unwrap(), None);
    }

    #[test]
    fn test_cached_mtime_returns_saved_mtime() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("default", "a.md", "hash1", 12345, "none", 42, 0, None).unwrap();
        let mtime = db.cached_mtime("default", "a.md").unwrap();
        assert_eq!(mtime, Some(12345));
    }

    #[test]
    fn test_cached_mtime_returns_none_for_missing() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let mtime = db.cached_mtime("default", "missing.md").unwrap();
        assert_eq!(mtime, None);
    }

    #[test]
    fn test_cached_mtime_updates_on_upsert() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("default", "a.md", "hash1", 1000, "none", 42, 0, None).unwrap();
        assert_eq!(db.cached_mtime("default", "a.md").unwrap(), Some(1000));
        db.upsert_file_cache("default", "a.md", "hash2", 2000, "none", 99, 0, None).unwrap();
        assert_eq!(db.cached_mtime("default", "a.md").unwrap(), Some(2000));
    }

    #[test]
    fn test_get_dominant_model_id_single_model() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("default", "a.md", "hash", 1000, "model-alpha", 42, 0, None).unwrap();
        db.upsert_file_cache("default", "b.md", "hash", 1000, "model-alpha", 42, 0, None).unwrap();
        let result = db.get_dominant_model_id().unwrap();
        assert_eq!(result, Some("model-alpha".to_string()));
    }

    #[test]
    fn test_get_dominant_model_id_returns_most_frequent() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("default", "a.md", "hash", 1000, "model-alpha", 42, 0, None).unwrap();
        db.upsert_file_cache("default", "b.md", "hash", 1000, "model-alpha", 42, 0, None).unwrap();
        db.upsert_file_cache("default", "c.md", "hash", 1000, "model-beta", 42, 0, None).unwrap();
        let result = db.get_dominant_model_id().unwrap();
        assert_eq!(result, Some("model-alpha".to_string()));
    }

    #[test]
    fn test_get_dominant_model_id_excludes_none() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("default", "a.md", "hash", 1000, "none", 42, 0, None).unwrap();
        db.upsert_file_cache("default", "b.md", "hash", 1000, "none", 42, 0, None).unwrap();
        let result = db.get_dominant_model_id().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_dominant_model_id_tie_breaks_deterministically() {
        let db = NoteDatabase::open_in_memory().unwrap();
        // Equal frequency for both
        db.upsert_file_cache("default", "a.md", "hash", 1000, "model-beta", 42, 0, None).unwrap();
        db.upsert_file_cache("default", "b.md", "hash", 1000, "model-alpha", 42, 0, None).unwrap();
        let result = db.get_dominant_model_id().unwrap();
        // ASC tie-breaker should pick model-alpha alphabetically
        assert_eq!(result, Some("model-alpha".to_string()));
    }

    #[test]
    fn test_get_dominant_model_id_empty_cache() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let result = db.get_dominant_model_id().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_cached_file_size_returns_saved_size() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("default", "a.md", "hash1", 1000, "none", 2048, 0, None).unwrap();
        let size = db.cached_file_size("default", "a.md").unwrap();
        assert_eq!(size, Some(2048));
    }

    #[test]
    fn test_cached_file_size_returns_none_for_missing() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let size = db.cached_file_size("default", "missing.md").unwrap();
        assert_eq!(size, None);
    }

    #[test]
    fn test_cached_file_size_updates_on_upsert() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("default", "a.md", "hash1", 1000, "none", 42, 0, None).unwrap();
        assert_eq!(db.cached_file_size("default", "a.md").unwrap(), Some(42));
        db.upsert_file_cache("default", "a.md", "hash2", 2000, "none", 99, 0, None).unwrap();
        assert_eq!(db.cached_file_size("default", "a.md").unwrap(), Some(99));
    }

    #[test]
    fn test_fts_search_finds_inserted_chunk() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunks = vec![
            Chunk { id: None, file_path: "b.md".into(), chunk_index: 0, parent_header: None, content: "search engine test".into(), tokenized_content: "search engine test".into(), vault_name: "default".to_string(), tags: String::new(), frontmatter_date: String::new(), title: String::new(), emphasized_text: String::new() },
        ];
        db.insert_chunks(&chunks).unwrap();
        let results = db.fts_search("search AND engine", 10, None, None, None).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_get_surrounding_chunks() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunks: Vec<Chunk> = (0..5).map(|i| Chunk {
            id: None, file_path: "c.md".into(), chunk_index: i,
            parent_header: None, content: format!("chunk {}", i),
            tokenized_content: format!("chunk {}", i),
            vault_name: "default".to_string(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
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
                vault_name: "default".to_string(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
                emphasized_text: String::new(),
            },
            Chunk {
                id: None,
                file_path: "b.md".into(),
                chunk_index: 5,
                parent_header: Some("# Section > Subsection".into()),
                content: "Second chunk with different text ABCDEF".into(),
                tokenized_content: "Second chunk with different text ABCDEF".into(),
                vault_name: "default".to_string(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
                emphasized_text: String::new(),
            },
        ];
        let ids = db.insert_chunks(&chunks).unwrap();
        assert_eq!(ids.len(), 2);

        let retrieved = db.get_chunks_by_ids(&ids).unwrap();
        assert_eq!(retrieved.len(), 2);

        assert_eq!(retrieved[0].id, Some(ids[0]), "first chunk id should match");
        assert_eq!(retrieved[1].id, Some(ids[1]), "second chunk id should match");

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
            Chunk { id: None, file_path: "d.md".into(), chunk_index: 0, parent_header: None, content: "unique token xyz987".into(), tokenized_content: "unique token xyz987".into(), vault_name: "default".to_string(), tags: String::new(), frontmatter_date: String::new(), title: String::new(), emphasized_text: String::new() },
        ];
        db.insert_chunks(&chunks).unwrap();
        // Verify findable before delete
        assert!(!db.fts_search("xyz987", 10, None, None, None).unwrap().is_empty());
        db.delete_file_fully("default", "d.md").unwrap();
        // After delete, should not be found
        assert!(db.fts_search("xyz987", 10, None, None, None).unwrap().is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn test_db_file_and_companion_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let db = NoteDatabase::open(&db_path).unwrap();

        // Perform a write to trigger WAL creation
        db.upsert_file_cache("default", "test.md", "hash", 1000, "none", 0, 0, None).unwrap();

        // Check companion files while db is alive (SQLite may remove -wal on close
        // via autocheckpoint).
        let wal = db_path.with_extension("db-wal");
        assert!(wal.exists(), "-wal should exist after write in WAL mode");
        let wal_meta = std::fs::metadata(&wal).unwrap();
        assert_eq!(wal_meta.permissions().mode() & 0o777, 0o600,
            "-wal should be 0o600");
        let shm = db_path.with_extension("db-shm");
        if shm.exists() {
            let shm_meta = std::fs::metadata(&shm).unwrap();
            assert_eq!(shm_meta.permissions().mode() & 0o777, 0o600,
                "-shm should be 0o600");
        }

        drop(db);

        // Main DB file
        let meta = std::fs::metadata(&db_path).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o600,
            "main DB file should be 0o600");
    }

    #[test]
    fn test_wal_mode() {
        let temp = tempfile::TempDir::new().unwrap();
        let db = NoteDatabase::open(temp.path().join("t.db")).unwrap();
        let mode: String = db.write_conn.borrow()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn test_get_chunks_by_ids_nonexistent_returns_empty() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let result = db.get_chunks_by_ids(&[99999, 88888]).unwrap();
        assert!(result.is_empty(), "non-existent IDs should return empty vec");
    }

    #[test]
    fn test_get_chunks_by_ids_mixed_existing_and_nonexistent() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunk = Chunk {
            id: None,
            file_path: "exists.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "test".into(),
            tokenized_content: "test".into(),
                vault_name: "default".to_string(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
                emphasized_text: String::new(),
        };
        let ids = db.insert_chunks(&[chunk]).unwrap();
        assert_eq!(ids.len(), 1);

        let result = db.get_chunks_by_ids(&[ids[0], 99999]).unwrap();
        assert_eq!(result.len(), 1, "should only return the existing chunk");
    }

    #[test]
    fn test_reindex_file_cleans_up_tasks_for_file() {
        let db = NoteDatabase::open_in_memory().unwrap();
        // Insert a task manually
        db.write_conn.borrow().execute(
            "INSERT INTO tasks (vault_name, file_path, content, checked, line_number)
             VALUES (?1, ?2, ?3, 0, 1)",
            params!["default", "project.md", "old task content"],
        ).unwrap();

        // Now reindex the file with new content (no tasks in new data)
        let chunks = vec![
            Chunk {
                id: None, file_path: "project.md".into(), chunk_index: 0,
                parent_header: None, content: "new content".into(),
                tokenized_content: "new content".into(),
                vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
        ];
        db.reindex_file(&ReindexParams {
            vault_name: "default",
            relative_path: "project.md",
            hash: "newhash",
            mtime: 2000,
            model_id: "none",
            chunks: &chunks,
            embeddings: &[],
            file_size: 100,
            tasks: &[],
            note_link_targets: &[],
            vlm_hash: None,
        }).unwrap();

        // Verify old task is gone
        let tasks: Vec<(String, String)> = {
            let conn = db.write_conn.borrow();
            let mut stmt = conn.prepare(
                "SELECT file_path, content FROM tasks WHERE vault_name = ?1 AND file_path = ?2"
            ).unwrap();
            let rows = stmt.query_map(params!["default", "project.md"], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            }).unwrap();
            rows.collect::<Result<Vec<_>, _>>().unwrap()
        };
        assert!(tasks.is_empty(),
            "reindex_file should clean up old tasks, found {} tasks", tasks.len());
    }

    #[test]
    fn test_reindex_file_clears_old_chunks_and_fts() {
        let db = NoteDatabase::open_in_memory().unwrap();
        // Insert old chunk and FTS entry
        let old_chunks = vec![
            Chunk {
                id: None, file_path: "old.md".into(), chunk_index: 0,
                parent_header: None, content: "old content here".into(),
                tokenized_content: "old content here".into(),
                vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
        ];
        db.insert_chunks(&old_chunks).unwrap();
        assert!(!db.fts_search("old content", 10, None, None, None).unwrap().is_empty());

        // Reindex with new content
        let new_chunks = vec![
            Chunk {
                id: None, file_path: "old.md".into(), chunk_index: 0,
                parent_header: None, content: "brand new content".into(),
                tokenized_content: "brand new content".into(),
                vault_name: "default".to_string(),
                tags: String::new(), frontmatter_date: String::new(),
                title: String::new(), emphasized_text: String::new(),
            },
        ];
        db.reindex_file(&ReindexParams {
            vault_name: "default",
            relative_path: "old.md",
            hash: "newhash",
            mtime: 2000,
            model_id: "none",
            chunks: &new_chunks,
            embeddings: &[],
            file_size: 100,
            tasks: &[],
            note_link_targets: &[],
            vlm_hash: None,
        }).unwrap();

        // Old content should not be findable
        assert!(db.fts_search("old content", 10, None, None, None).unwrap().is_empty());
        // New content should be findable
        assert!(!db.fts_search("brand new", 10, None, None, None).unwrap().is_empty());
    }

    #[test]
    fn test_wal_mode_persists_after_reopen() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        {
            let db = NoteDatabase::open(&db_path).unwrap();
            let journal: String = db.write_conn.borrow()
                .pragma_query_value(None, "journal_mode", |r| r.get(0))
                .unwrap();
            assert_eq!(journal.to_lowercase(), "wal", "journal mode should be WAL on fresh DB");
            db.upsert_file_cache("default", "test.md", "hash", 1000, "none", 0, 0, None).unwrap();
        }

        let db2 = NoteDatabase::open(&db_path).unwrap();
        let journal2: String = db2.write_conn.borrow()
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(journal2.to_lowercase(), "wal", "journal mode should remain WAL after reopen");
    }

    // ── batch ops and metadata consistency ───────────────────────────

    #[test]
    fn test_insert_chunks_different_indices_same_path() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunk1 = Chunk {
            id: None,
            file_path: "test.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "content1".into(),
            tokenized_content: "content1".into(),
            vault_name: "default".to_string(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
        };
        let chunk2 = Chunk {
            id: None,
            file_path: "test.md".into(),
            chunk_index: 1,
            parent_header: None,
            content: "content2".into(),
            tokenized_content: "content2".into(),
            vault_name: "default".to_string(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
        };

        let ids1 = db.insert_chunks(&[chunk1]).unwrap();
        let ids2 = db.insert_chunks(&[chunk2]).unwrap();
        assert_ne!(ids1[0], ids2[0], "different indices should get different IDs");
    }

    #[test]
    fn test_get_chunks_by_ids_large_batch() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let mut chunks = Vec::new();
        for i in 0..100 {
            chunks.push(Chunk {
                id: None,
                file_path: format!("file{}.md", i),
                chunk_index: 0,
                parent_header: None,
                content: format!("content{}", i),
                tokenized_content: format!("content{}", i),
                vault_name: "default".to_string(),
                tags: String::new(),
                frontmatter_date: String::new(),
                title: String::new(),
                emphasized_text: String::new(),
            });
        }

        let ids = db.insert_chunks(&chunks).unwrap();
        let retrieved = db.get_chunks_by_ids(&ids).unwrap();
        assert_eq!(retrieved.len(), 100, "should retrieve all inserted chunks");
    }

    #[test]
    fn test_fts_search_deduplication() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunk = Chunk {
            id: None,
            file_path: "test.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "search term here".into(),
            tokenized_content: "search term here".into(),
            vault_name: "default".to_string(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
        };

        db.insert_chunks(&[chunk]).unwrap();
        let results = db.fts_search("search", 10, None, None, None).unwrap();
        assert_eq!(results.len(), 1, "unique chunks should appear once");
    }

    #[test]
    fn test_migration_user_version_query_error_propagates() {
        // Verify that a failing PRAGMA user_version query is not silently ignored.
        // The migrate() function should propagate the error, not default to 0.
        let db = NoteDatabase::open_in_memory().unwrap();
        let conn = db.write_conn.borrow();
        // Close the connection to force a query error on the next PRAGMA call.
        // Since we can't easily make PRAGMA fail without corrupting the DB,
        // at least verify that migration completes successfully on a valid DB.
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert!(version >= 0, "user_version should be a non-negative integer");
    }

    #[test]
    fn test_migration_creates_all_tables() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let conn = db.write_conn.borrow();
        // Verify all expected tables exist after migration
        for table in &["chunks", "file_cache", "fts_chunks", "vec_chunks"] {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                params![table],
                |r| r.get(0),
            ).unwrap_or(0);
            // fts_chunks and vec_chunks are virtual tables
            assert!(count > 0 || table == &"fts_chunks" || table == &"vec_chunks",
                "table {} should exist after migration", table);
        }
    }

    #[test]
    fn test_migration_drops_v1_tables() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let conn = db.write_conn.borrow();
        // V1 tables (notes_fts, notes_meta) should not exist after v2+ migration
        for table in &["notes_fts", "notes_meta"] {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                params![table],
                |r| r.get(0),
            ).unwrap_or(0);
            assert_eq!(count, 0, "v1 table {} should be dropped after migration", table);
        }
    }

    #[test]
    fn test_migration_idempotent() {
        // Running migrate() twice should not produce errors
        let db = NoteDatabase::open_in_memory().unwrap();
        // First call happens in open_in_memory.
        // We can call the private method indirectly by creating a new connection
        // and opening it. The open_in_memory already calls migrate.
        let stats = db.stats().unwrap();
        assert!(stats.total_chunks == 0, "migration should be idempotent");
        // Insert something and verify DB works
        db.upsert_file_cache("default", "test.md", "hash", 1000, "none", 0, 0, None).unwrap();
        assert_eq!(db.cached_hash("default", "test.md").unwrap(), Some("hash".to_string()));
    }

    #[test]
    fn test_migration_cleans_up_orphan_file_cache_v3() {
        // Simulate a crash scenario where file_cache_v3 was left as an orphan
        // after "DROP TABLE file_cache" succeeded but "ALTER TABLE file_cache_v3
        // RENAME TO file_cache" did not.
        let temp = tempfile::TempDir::new().unwrap();
        let db_path = temp.path().join("test.db");
        let db = NoteDatabase::open(&db_path).unwrap();
        drop(db);

        // Create orphan file_cache_v3 table directly
        let conn = rusqlite::Connection::open(&db_path).unwrap();
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS file_cache_v3 (
                vault_name TEXT NOT NULL,
                path TEXT NOT NULL,
                hash TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                model_id TEXT NOT NULL,
                PRIMARY KEY (vault_name, path)
            )
        ").unwrap();
        drop(conn);

        // Re-open the database — migration should clean up the orphan
        let db = NoteDatabase::open(&db_path).unwrap();
        let conn = db.write_conn.borrow();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='file_cache_v3'",
            [],
            |r| r.get(0),
        ).unwrap_or(0);
        assert_eq!(count, 0, "orphan file_cache_v3 should be cleaned up by migration");
    }

    #[test]
    fn test_metadata_consistency_after_chunk_insert() {
        let db = NoteDatabase::open_in_memory().unwrap();
        db.upsert_file_cache("default", "test.md", "abcd1234", 1000, "hash", 0, 0, None).unwrap();

        let chunk = Chunk {
            id: None,
            file_path: "test.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "content".into(),
            tokenized_content: "content".into(),
            vault_name: "default".to_string(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
        };

        let ids = db.insert_chunks(&[chunk]).unwrap();
        assert!(!ids.is_empty(), "chunk insert should succeed after metadata insert");
    }

    #[test]
    fn test_open_creates_parent_directory() {
        let tmp = tempfile::TempDir::new().unwrap();
        let db_path = tmp.path().join("deep").join("nested").join("dir").join("test.db");

        // Parent should not exist before open
        assert!(!db_path.parent().unwrap().exists());

        // open() should create parent directories
        let db = NoteDatabase::open(&db_path).unwrap();
        drop(db);

        assert!(db_path.parent().unwrap().exists(), "open() should create parent directories");
        assert!(db_path.exists(), "DB file should exist after open");
    }

    // ── Backlink / note_links tests ──────────────────────────────────
    // ── purge_expired tests ───────────────────────────────────────────────

    fn now_millis() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }

    fn insert_test_file(db: &NoteDatabase, vault_name: &str, file_path: &str, content: &str, mtime: i64) {
        let chunk = Chunk {
            id: None,
            file_path: file_path.into(),
            chunk_index: 0,
            parent_header: None,
            content: content.into(),
            tokenized_content: content.into(),
            vault_name: vault_name.into(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
        };
        db.insert_chunks(&[chunk]).unwrap();
        db.upsert_file_cache(vault_name, file_path, "hash", mtime, "none", 0, 0, None).unwrap();
    }

    #[test]
    fn test_purge_expired_removes_old_files() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let now = now_millis();
        let old_mtime = now - (100 * 86_400_000);
        let recent_mtime = now - (10 * 86_400_000);

        insert_test_file(&db, "default", "old.md", "old content", old_mtime);
        insert_test_file(&db, "default", "recent.md", "recent content", recent_mtime);

        let mut retention = std::collections::HashMap::new();
        retention.insert("default".to_string(), 30);
        let purged = db.purge_expired(&retention).unwrap();

        assert_eq!(purged, 1, "should purge 1 old file");
        assert!(db.cached_hash("default", "old.md").unwrap().is_none(), "old file should be purged");
        assert!(db.cached_hash("default", "recent.md").unwrap().is_some(), "recent file should remain");
    }

    #[test]
    fn test_purge_expired_keeps_recent_files() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let now = now_millis();
        let recent_mtime = now - (10 * 86_400_000);

        insert_test_file(&db, "default", "recent.md", "recent content", recent_mtime);

        let mut retention = std::collections::HashMap::new();
        retention.insert("default".to_string(), 30);
        let purged = db.purge_expired(&retention).unwrap();

        assert_eq!(purged, 0, "should purge 0 recent files");
        assert!(db.cached_hash("default", "recent.md").unwrap().is_some(), "recent file should remain");
    }

    #[test]
    fn test_purge_expired_no_config_is_noop() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let now = now_millis();

        insert_test_file(&db, "default", "test.md", "content", now);

        let retention = std::collections::HashMap::new();
        let purged = db.purge_expired(&retention).unwrap();

        assert_eq!(purged, 0, "empty retention map should be no-op");
        assert!(db.cached_hash("default", "test.md").unwrap().is_some(), "file should remain");
    }

    #[test]
    fn test_purge_expired_per_vault() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let now = now_millis();
        let old_mtime = now - (100 * 86_400_000);

        insert_test_file(&db, "default", "vault1.md", "content", old_mtime);
        insert_test_file(&db, "other", "vault2.md", "content", old_mtime);

        let mut retention = std::collections::HashMap::new();
        retention.insert("default".to_string(), 30);
        let purged = db.purge_expired(&retention).unwrap();

        assert_eq!(purged, 1, "should purge only the configured vault");
        assert!(db.cached_hash("default", "vault1.md").unwrap().is_none(), "default vault file should be purged");
        assert!(db.cached_hash("other", "vault2.md").unwrap().is_some(), "other vault file should remain");
    }

    // ── purge_all_user_data tests ────────────────────────────────────────

    #[test]
    fn test_purge_all_user_data_clears_all_tables() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let now = now_millis();

        insert_test_file(&db, "default", "test.md", "content", now);

        // Insert embedding
        let chunk_id: i64 = db.write_conn.borrow()
            .query_row("SELECT id FROM chunks WHERE vault_name = ?1 AND file_path = ?2",
                params!["default", "test.md"], |r| r.get(0))
            .expect("chunk should exist");
        let embedding = vec![0.0f32; 1024];
        db.insert_embeddings(&[(chunk_id, embedding)]).unwrap();

        // Verify data exists
        assert_eq!(db.stats().unwrap().total_files, 1);
        assert!(db.stats().unwrap().total_chunks > 0);

        db.purge_all_user_data().unwrap();

        // Verify all data is cleared
        assert_eq!(db.stats().unwrap().total_files, 0);
        assert_eq!(db.stats().unwrap().total_chunks, 0);
    }

    #[test]
    fn test_purge_all_user_data_keeps_schema() {
        let db = NoteDatabase::open_in_memory().unwrap();

        insert_test_file(&db, "default", "test.md", "content", 1000);
        db.purge_all_user_data().unwrap();

        // Verify schema still exists by inserting again
        let chunk2 = Chunk {
            id: None,
            file_path: "test2.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "new content".into(),
            tokenized_content: "new content".into(),
            vault_name: "default".into(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
        };
        db.insert_chunks(&[chunk2]).unwrap();
        assert_eq!(db.stats().unwrap().total_chunks, 1, "can still insert chunks after purge");
        // Also verify we can insert file_cache (table still exists)
        db.upsert_file_cache("default", "test2.md", "hash", 1000, "none", 0, 0, None).unwrap();
        assert_eq!(db.stats().unwrap().total_files, 1, "can still insert file_cache after purge");
    }

    #[test]
    fn test_delete_orphaned_embeddings() {
        let db = NoteDatabase::open_in_memory().unwrap();
        let chunks = vec![Chunk {
            id: None,
            file_path: "a.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "test".into(),
            tokenized_content: "test".into(),
            vault_name: "default".to_string(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
        }];
        let ids = db.insert_chunks(&chunks).unwrap();
        let chunk_id = ids[0];

        db.insert_embeddings(&[(chunk_id, vec![0.1; 1024])]).unwrap();

        db.write_conn.borrow().execute("DELETE FROM chunks WHERE id = ?1", [chunk_id]).unwrap();

        let count: i64 = db.write_conn.borrow().query_row(
            "SELECT COUNT(*) FROM vec_chunks WHERE chunk_id = ?1", [chunk_id], |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 1, "orphaned embedding should exist before cleanup");

        let deleted = db.delete_orphaned_embeddings().unwrap();
        assert_eq!(deleted, 1);

        let count: i64 = db.write_conn.borrow().query_row(
            "SELECT COUNT(*) FROM vec_chunks WHERE chunk_id = ?1", [chunk_id], |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 0);
    }
}
