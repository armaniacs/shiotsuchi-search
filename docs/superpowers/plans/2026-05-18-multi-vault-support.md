# Multi-Vault Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow configuration of multiple notes directories ("vaults") in `config.toml`, all sharing a single SQLite database, with a `config-migrate` subcommand for transition.

**Architecture:** Config layer (`ShiotsuchiConfig`) gains `[database]`, `[vaults.xxx]` sections; core model (`IndexConfig`) replaces single `notes_dir` with `Vec<(String, PathBuf)>`; DB schema adds `vault_name` column; indexer loops over vaults; watcher spawns one watcher per vault; search filters by vault.

**Tech Stack:** Rust, SQLite/FTS5, serde+toml, notify 9

---

## File Change Summary

| File | Change |
|---|---|
| `core/src/models.rs` | `IndexConfig.vaults`, `ChunkSearchResult.vault_name` |
| `core/src/db.rs` | Schema v3 migration, `vault_name` params on all methods |
| `core/src/indexer.rs` | Vault loop, `vault_name` propagation |
| `core/src/search.rs` | `vault_filter` param, `vault_name` in results |
| `core/src/watcher.rs` | Multi-vault watchers, vault-aware path checks |
| `cli/src/config.rs` | `DatabaseConfig`, `VaultEntry`, backward compat |
| `cli/src/commands/config_migrate.rs` | **NEW** |
| `cli/src/commands/mod.rs` | Add `config_migrate` module |
| `cli/src/main.rs` | `ConfigMigrate` command, vaults plumbing |
| `cli/src/commands/chart.rs` | Build `IndexConfig` with vaults |
| `cli/src/commands/scan.rs` | Build `IndexConfig` with vaults |
| `cli/src/commands/dredge.rs` | Iterate vaults for cleanup |
| `cli/src/commands/delete.rs` | Accept vault context |
| `cli/src/commands/config.rs` | Scan all vaults |
| `mcp/src/main.rs` | Vaults support in McpConfig |
| `mcp/src/handler.rs` | Vault filter in search |

---

### Task 1: Core Models — IndexConfig vaults + ChunkSearchResult vault_name

**Files:**
- Modify: `core/src/models.rs`

- [ ] **Step 1: Add vault_name to ChunkSearchResult**

```rust
pub struct ChunkSearchResult {
    pub vault_name: String, // NEW
    pub chunk_id: i64,
    pub file_path: String,
    pub parent_header: Option<String>,
    pub content: String,
    pub score: f64,
    pub search_mode: SearchMode,
}
```

- [ ] **Step 2: Replace notes_dir with vaults in IndexConfig**

```rust
pub struct IndexConfig {
    /// Named vaults: (vault_name, notes_dir). At least one entry.
    pub vaults: Vec<(String, PathBuf)>,
    pub include_extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub auto_exclude_hidden: bool,
    pub follow_links: bool,
    pub dynamic_threshold: usize,
}
```

Add convenience constructors:
```rust
impl IndexConfig {
    /// Create a single-vault config (backward compat).
    pub fn single(notes_dir: PathBuf) -> Self {
        Self {
            vaults: vec![("default".to_string(), notes_dir)],
            ..Default::default()
        }
    }

    /// Create a multi-vault config from named pairs.
    pub fn with_vaults(vaults: Vec<(String, PathBuf)>) -> Self {
        Self {
            vaults,
            ..Default::default()
        }
    }
}
```

- [ ] **Step 3: Update Default impl**

```rust
impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            vaults: vec![("default".to_string(), PathBuf::from("."))],
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            exclude_dirs: vec!["node_modules".to_string()],
            auto_exclude_hidden: true,
            follow_links: false,
            dynamic_threshold: 5,
        }
    }
}
```

- [ ] **Step 4: Update tests (models.rs)** to use `IndexConfig::single()` or populate `.vaults`.

- [ ] **Step 5: Commit**

```bash
git add core/src/models.rs
git commit -m "feat(core): add vault support to IndexConfig and ChunkSearchResult"
```

---

### Task 2: DB Schema Migration + vault_name Support

**Files:**
- Modify: `core/src/db.rs`

- [ ] **Step 1: Add vault_name column migration in migrate()**

In `migrate()`, add a v3 migration that runs after the existing v2 check:

```rust
fn migrate(&self) -> Result<(), DbError> {
    let conn = self.write_conn.borrow();
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0);

    if version < 2 {
        // Existing v1→v2 migration (drops old notes_fts/notes_meta, creates new schema)
        conn.execute_batch("
            DROP TABLE IF EXISTS notes_fts;
            DROP TABLE IF EXISTS notes_meta;
        ")?;
        self.create_schema(&conn)?;
        conn.execute_batch("PRAGMA user_version = 2")?;
    }

    if version < 3 {
        // v2→v3: add vault_name column
        // chunks: add vault_name (PK is id, no conflict)
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN vault_name TEXT NOT NULL DEFAULT 'default'")?;
        // file_cache: recreate with composite PK (vault_name, path)
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS file_cache_v2 (
                vault_name TEXT NOT NULL,
                path TEXT NOT NULL,
                hash TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                model_id TEXT NOT NULL,
                PRIMARY KEY (vault_name, path)
            )
        ")?;
        conn.execute_batch("
            INSERT INTO file_cache_v2 (vault_name, path, hash, mtime, model_id)
            SELECT 'default', path, hash, mtime, model_id FROM file_cache
        ")?;
        conn.execute_batch("DROP TABLE file_cache")?;
        conn.execute_batch("ALTER TABLE file_cache_v2 RENAME TO file_cache")?;
        conn.execute_batch("PRAGMA user_version = 3")?;
    }
    Ok(())
}
```

- [ ] **Step 2: Update insert_chunks to read vault_name from Chunk**

```rust
pub fn insert_chunks(&self, chunks: &[Chunk]) -> Result<Vec<i64>, DbError> {
    let mut conn = self.write_conn.borrow_mut();
    let tx = conn.transaction()?;
    let mut ids = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        tx.execute(
            "INSERT INTO chunks (file_path, chunk_index, parent_header, content, tokenized_content, vault_name)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![chunk.file_path, chunk.chunk_index, chunk.parent_header, chunk.content, chunk.tokenized_content, chunk.vault_name],
        )?;
        // FTS insert unchanged (fts_chunks maps to tokenized_content via external content)
        let id = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO fts_chunks(rowid, tokenized_content) VALUES (?1, ?2)",
            params![id, chunk.tokenized_content],
        )?;
        ids.push(id);
    }
    tx.commit()?;
    Ok(ids)
}
```

- [ ] **Step 3: Update delete_chunks_for_file to accept vault_name**

```rust
pub fn delete_chunks_for_file(&self, vault_name: &str, file_path: &str) -> Result<(), DbError> {
    let mut conn = self.write_conn.borrow_mut();
    let tx = conn.transaction()?;

    let ids: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT id FROM chunks WHERE vault_name = ?1 AND file_path = ?2"
        )?;
        let rows = stmt.query_map(params![vault_name, file_path], |r| r.get(0))?;
        rows.collect::<SqliteResult<Vec<_>>>()?
    };

    for id in &ids {
        tx.execute("DELETE FROM fts_chunks WHERE rowid = ?1", [id])?;
        tx.execute("DELETE FROM vec_chunks WHERE chunk_id = ?1", [id])?;
    }
    tx.execute(
        "DELETE FROM chunks WHERE vault_name = ?1 AND file_path = ?2",
        params![vault_name, file_path],
    )?;
    tx.commit()?;
    Ok(())
}
```

- [ ] **Step 4: Update file_cache methods to accept vault_name**

```rust
pub fn upsert_file_cache(
    &self,
    vault_name: &str,
    path: &str,
    hash: &str,
    mtime: i64,
    model_id: &str,
) -> Result<(), DbError> {
    self.write_conn.borrow().execute(
        "INSERT INTO file_cache (vault_name, path, hash, mtime, model_id)
         VALUES (?1,?2,?3,?4,?5)
         ON CONFLICT(vault_name, path) DO UPDATE SET
             hash=excluded.hash, mtime=excluded.mtime, model_id=excluded.model_id",
        params![vault_name, path, hash, mtime, model_id],
    )?;
    Ok(())
}

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

pub fn delete_file_cache(&self, vault_name: &str, path: &str) -> Result<(), DbError> {
    self.write_conn.borrow().execute(
        "DELETE FROM file_cache WHERE vault_name = ?1 AND path = ?2",
        params![vault_name, path],
    )?;
    Ok(())
}
```

- [ ] **Step 5: Update get_chunks_by_ids to select vault_name**

Change the SQL to include `vault_name` and popuate it in Chunk:

```rust
let sql = format!(
    "SELECT id, file_path, chunk_index, parent_header, content, tokenized_content, vault_name \
     FROM chunks WHERE id IN ({})",
    placeholders
);
// ...
let rows = stmt.query_map(params_vec.as_slice(), |r| {
    Ok(Chunk {
        id: Some(r.get(0)?),
        file_path: r.get(1)?,
        chunk_index: r.get(2)?,
        parent_header: r.get(3)?,
        content: r.get(4)?,
        tokenized_content: r.get(5)?,
        vault_name: r.get(6)?,
    })
})?;
```

- [ ] **Step 6: Update get_surrounding_chunks to select vault_name**

Same change: add `vault_name` to the SELECT list and Chunk construction.

```rust
let mut stmt = conn.prepare(
    "SELECT id, file_path, chunk_index, parent_header, content, tokenized_content, vault_name \
     FROM chunks WHERE file_path = ?1 AND chunk_index BETWEEN ?2 AND ?3 \
     ORDER BY chunk_index"
)?;
// ...
let rows = stmt.query_map(..., |r| {
    Ok(Chunk {
        id: Some(r.get(0)?),
        file_path: r.get(1)?,
        chunk_index: r.get(2)?,
        parent_header: r.get(3)?,
        content: r.get(4)?,
        tokenized_content: r.get(5)?,
        vault_name: r.get(6)?,
    })
})?;
```

- [ ] **Step 7: Add list_cached_paths to accept vault_name (optional, for cleanup)**

```rust
pub fn list_cached_paths(&self, vault_name: &str) -> Result<Vec<String>, DbError> {
    let conn = self.write_conn.borrow();
    let mut stmt = conn.prepare(
        "SELECT path FROM file_cache WHERE vault_name = ?1"
    )?;
    let rows = stmt.query_map([vault_name], |r| r.get(0))?;
    rows.collect::<SqliteResult<Vec<_>>>().map_err(DbError::Sqlite)
}
```

- [ ] **Step 8: Update all tests in db.rs** to add `vault_name: "default".to_string()` to each Chunk instance, and pass `"default"` as vault_name to `delete_chunks_for_file`, `upsert_file_cache`, `cached_hash`, `delete_file_cache`.

- [ ] **Step 9: Commit**

```bash
git add core/src/db.rs
git commit -m "feat(core): add vault_name to DB schema and methods"
```

---

### Task 3: Indexer — Vault Loop + vault_name Propagation

**Files:**
- Modify: `core/src/indexer.rs`

- [ ] **Step 1: Update index_directory to loop over vaults**

```rust
pub fn index_directory(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
    embedder: Option<&Embedder>,
    progress: Option<IndexProgress>,
) -> Result<(Vec<(String, String, IndexResult)>, usize), DbError> {
//                                 ^^^^^^^^ vault_name
    let (exclude_globset, invalid_patterns) = build_exclude_globset(&config.exclude_dirs);
    let mut all_results = Vec::new();

    for (vault_name, notes_dir) in &config.vaults {
        let notes_canonical = if config.follow_links {
            Some(notes_dir.canonicalize().map_err(|e| {
                DbError::Io(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("cannot canonicalize notes_dir '{}': {}", vault_name, e),
                ))
            })?)
        } else {
            None
        };

        let entries: Vec<_> = WalkDir::new(notes_dir)
            .follow_links(config.follow_links)
            .into_iter()
            .filter_entry(|e| {
                if e.file_type().is_dir() && e.depth() > 0 {
                    if config.auto_exclude_hidden && e.file_name().to_string_lossy().starts_with('.') {
                        return false;
                    }
                    let name = e.file_name().to_string_lossy();
                    if exclude_globset.is_match(name.as_ref()) {
                        return false;
                    }
                }
                true
            })
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect();

        let total = entries.len();
        // ... rest of existing loop but with vault_name passed through
        for (i, entry) in entries.iter().enumerate() {
            let path = entry.path();
            let relative = if let Ok(p) = path.strip_prefix(notes_dir) {
                p.to_string_lossy().into_owned()
            } else {
                log::warn!("File path {:?} outside vault '{}' root {:?}", path, vault_name, notes_dir);
                continue;
            };

            // Use existing index_file_with_embedder but pass vault_name
            let result = index_file_with_embedder(
                db, tokenizer, embedder, path, vault_name, &relative, config,
            );
            all_results.push((relative, result));
            if let Some(ref cb) = progress {
                cb(i + 1, total);
            }
        }
    }

    Ok((all_results, invalid_patterns))
}
```

- [ ] **Step 2: Update index_file_with_embedder signature**

```rust
pub fn index_file_with_embedder(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    embedder: Option<&Embedder>,
    file_path: &Path,
    vault_name: &str,         // NEW
    relative_path: &str,      // was just path: &str
    config: &IndexConfig,
) -> IndexResult {
```

Inside this function, replace all `config.notes_dir` usage with `config.vaults[0].1` or pass `notes_dir` separately.

Actually, looking at the existing code more carefully, `index_file_with_embedder` doesn't use `config.notes_dir` directly — it uses `relative_path` which is already computed. The `_config` parameter is only used for `include_extensions` check. Let me verify by reading the full function.

Let me re-read the function carefully. Actually, I saw in index_file():
```rust
pub fn index_file(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    file_path: &Path,
    relative_path: &str,
    _config: &IndexConfig,
) -> IndexResult {
    index_file_with_embedder(db, tokenizer, None, file_path, relative_path, _config)
}
```

And index_file_with_embedder uses `_config` for extension checking and such. The vault_name is needed for DB operations inside the function.

- [ ] **Step 3: Inside index_file_with_embedder, use vault_name for DB calls**

```rust
// Instead of:
//   db.upsert_file_cache(relative_path, &hash, mtime, model_id)?;
// Use:
db.upsert_file_cache(vault_name, relative_path, &hash, mtime, model_id)?;

// Set vault_name on each chunk before inserting
for chunk in &mut chunks {
    chunk.vault_name = vault_name.to_string();
}
db.insert_chunks(&chunks)?;

// Instead of:
//   db.delete_chunks_for_file(relative_path)?;
// Use:
db.delete_chunks_for_file(vault_name, relative_path)?;
```

- [ ] **Step 4: Update cleanup_deleted function** to accept vault_name

```rust
pub fn cleanup_deleted(
    db: &NoteDatabase,
    config: &IndexConfig,
) -> Result<usize, DbError> {
    let mut total_deleted = 0;
    for (vault_name, notes_dir) in &config.vaults {
        let cached = db.list_cached_paths(vault_name)?;
        for rel_path in cached {
            let full_path = notes_dir.join(&rel_path);
            if !full_path.exists() {
                db.delete_chunks_for_file(vault_name, &rel_path)?;
                total_deleted += 1;
            }
        }
    }
    Ok(total_deleted)
}
```

- [ ] **Step 5: Update all tests in indexer.rs** to pass vault_name

All test IndexConfig instances need `.vaults` populated and DB calls need `"default"`.

- [ ] **Step 6: Commit**

```bash
git add core/src/indexer.rs
git commit -m "feat(core): indexer vault loop and vault_name propagation"
```

---

### Task 4: Search — vault_filter Support

**Files:**
- Modify: `core/src/search.rs`

- [ ] **Step 1: Add vault_filter parameter to search()**

```rust
pub fn search(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    query: &str,
    limit: usize,
    mode: SearchMode,
    embedder: Option<&crate::embedder::Embedder>,
    min_score: Option<f64>,
    vault_filter: Option<&str>,   // NEW
) -> Result<Vec<ChunkSearchResult>, DbError> {
```

Pass `vault_filter` through to `search_fts()`, `search_vec()`, `search_hybrid()`.

- [ ] **Step 2: Add vault_filter to search_fts**

Instead of using FTS5 MATCH for vault filtering (since vault_name is not in fts_chunks), use a JOIN with chunks:

```rust
fn search_fts(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    query: &str,
    limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
) -> Result<Vec<ChunkSearchResult>, DbError> {
    // ... existing tokenizer logic ...
    let hits = db.fts_search(&fts5_query, limit)?;
    // ... existing logic to get chunks ...

    // Filter by vault if requested
    let results = if let Some(vault) = vault_filter {
        results.into_iter()
            .filter(|r| r.vault_name == vault)
            .collect()
    } else {
        results
    };

    // ... rest unchanged ...
    Ok(results)
}
```

`Chunk` already has `vault_name` (Task 1) and `get_chunks_by_ids` already selects it (Task 2). So search can use `c.vault_name` directly.

- [ ] **Step 3: Update search result construction** to populate vault_name

```rust
Some(ChunkSearchResult {
    vault_name: c.vault_name,    // NEW
    chunk_id: id,
    file_path: c.file_path,
    parent_header: c.parent_header,
    content: c.content,
    score,
    search_mode: SearchMode::Fts,
})
```

Same pattern for search_vec and search_hybrid.

- [ ] **Step 4: Handle vault_filter in search_fts**

```rust
fn search_fts(..., vault_filter: Option<&str>) -> ... {
    // ... get hits, get chunks, build results ...
    let mut results: Vec<ChunkSearchResult> = chunks.into_iter().filter_map(|c| { ... }).collect();

    if let Some(vault) = vault_filter {
        results.retain(|r| r.vault_name == vault);
    }

    // ... sort, min_score ...
    Ok(results)
}
```

- [ ] **Step 5: Pass vault_filter through search_vec and search_hybrid**

Same pattern: accept `vault_filter: Option<&str>`, filter results after construction.

- [ ] **Step 6: Update all tests in search.rs** to include vault_name in ChunkSearchResult assertions and pass `None` for vault_filter.

- [ ] **Step 7: Commit**

```bash
git add core/src/models.rs core/src/db.rs core/src/search.rs
git commit -m "feat(core): add vault_name to Chunk and vault_filter to search"
```

---

### Task 5: Watcher — Multiple Vault Watchers

**Files:**
- Modify: `core/src/watcher.rs`

- [ ] **Step 1: Update VaultWatcher to hold vaults list**

```rust
pub struct VaultWatcher {
    db: Arc<Mutex<NoteDatabase>>,
    tokenizer: Arc<JapaneseTokenizer>,
    config: IndexConfig,
    embedder: Option<Embedder>,
    watchers: Arc<Mutex<Vec<notify::RecommendedWatcher>>>,  // NEW: keep watchers alive
}
```

Update the constructor to initialize `watchers: Arc::new(Mutex::new(Vec::new()))`.

- [ ] **Step 2: Update watch() to create one watcher per vault**

```rust
pub fn watch(&self) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = channel();

    for (vault_name, notes_dir) in &self.config.vaults {
        let tx = tx.clone();
        let vname = vault_name.clone();
        let n_dir = notes_dir.clone();

        let mut watcher = notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send((vname.clone(), event));
            }
        })?;

        watcher.watch(&n_dir, RecursiveMode::Recursive)?;
        eprintln!("Watching vault '{}': {} for changes...", vname, n_dir.display());
        // Keep watcher alive: store in a Vec
        self.watchers.lock().unwrap().push(watcher);
    }

    loop {
        match rx.recv() {
            Ok((vault_name, event)) => self.handle_event(&vault_name, &event)?,
            Err(e) => {
                eprintln!("Watch error: {}", e);
                break;
            }
        }
    }
    Ok(())
}
```

But wait — the current VaultWatcher doesn't store watchers. We need to add a field to keep them alive. Let me add:

```rust
pub struct VaultWatcher {
    db: Arc<Mutex<NoteDatabase>>,
    tokenizer: Arc<JapaneseTokenizer>,
    config: IndexConfig,
    embedder: Option<Embedder>,
    watchers: Arc<Mutex<Vec<notify::RecommendedWatcher>>>,
}
```

- [ ] **Step 3: Update handle_event to accept vault_name**

```rust
fn handle_event(&self, vault_name: &str, event: &notify::Event) -> Result<(), Box<dyn std::error::Error>> {
```

All internal DB calls use `vault_name`:
```rust
db.delete_chunks_for_file(vault_name, &rel_str)?;
db.delete_file_cache(vault_name, &rel_str)?;
index_file_with_embedder(&db, &self.tokenizer, self.embedder.as_ref(), path, vault_name, &rel_str, &self.config)
```

- [ ] **Step 4: Update is_path_within_vault to check against all vaults**

```rust
/// Find which vault a path belongs to. Returns None if outside all vaults.
fn resolve_vault_for_path(&self, path: &Path) -> Option<(String, PathBuf)> {
    for (vault_name, notes_dir) in &self.config.vaults {
        let vault_canonical = match notes_dir.canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };
        let file_canonical = match path.canonicalize() {
            Ok(c) => c,
            Err(_) => continue,
        };
        if file_canonical.starts_with(&vault_canonical) {
            return Some((vault_name.clone(), notes_dir.clone()));
        }
    }
    None
}
```

Update `handle_event` to use `resolve_vault_for_path` for each event path:

```rust
fn handle_event(&self, vault_name: &str, event: &notify::Event) -> Result<(), Box<dyn std::error::Error>> {
    use notify::event::{EventKind, ModifyKind, RenameMode};

    // Resolve the vault's notes_dir for strip_prefix
    let notes_dir = match self.config.vaults.iter().find(|(n, _)| n == vault_name) {
        Some((_, dir)) => dir.clone(),
        None => return Ok(()),
    };

    match event.kind {
        EventKind::Modify(ModifyKind::Data(_)) | EventKind::Create(_) => {
            for path in &event.paths {
                if let Ok(rel) = path.strip_prefix(&notes_dir) {
                    let rel_str = rel.to_string_lossy();
                    let db = self.db.lock().unwrap();
                    if let IndexResult::Error(e) =
                        index_file_with_embedder(&db, &self.tokenizer, self.embedder.as_ref(), path, vault_name, &rel_str, &self.config)
                    {
                        log::warn!("watcher: failed to index {}: {}", rel_str, e);
                    }
                }
            }
        }
        EventKind::Remove(_) => {
            for path in &event.paths {
                if let Ok(rel) = path.strip_prefix(&notes_dir) {
                    let rel_str = rel.to_string_lossy();
                    let db = self.db.lock().unwrap();
                    db.delete_chunks_for_file(vault_name, &rel_str)?;
                    db.delete_file_cache(vault_name, &rel_str)?;
                }
            }
        }
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() == 2 => {
            let old = &event.paths[0];
            let new = &event.paths[1];
            if let Ok(old_rel) = old.strip_prefix(&notes_dir) {
                let rel_str = old_rel.to_string_lossy();
                let db = self.db.lock().unwrap();
                db.delete_chunks_for_file(vault_name, &rel_str)?;
                let _ = db.delete_file_cache(vault_name, &rel_str);
            }
            if let Ok(new_rel) = new.strip_prefix(&notes_dir) {
                let db = self.db.lock().unwrap();
                if let IndexResult::Error(e) = index_file_with_embedder(
                    &db, &self.tokenizer, self.embedder.as_ref(), new, vault_name, &new_rel.to_string_lossy(), &self.config,
                ) {
                    log::warn!("watcher: failed to index new path {}: {}", new_rel.to_string_lossy(), e);
                }
            }
        }
        _ => {}
    }
    Ok(())
}
```

- [ ] **Step 5: Update all tests in watcher.rs** to use `vault_name` with DB methods and IndexConfig with vaults.

- [ ] **Step 6: Commit**

```bash
git add core/src/watcher.rs
git commit -m "feat(core): multi-vault watcher support"
```

---

### Task 6: CLI Config — New Format + Backward Compat

**Files:**
- Modify: `cli/src/config.rs`

- [ ] **Step 1: Add DatabaseConfig and VaultEntry structs**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DatabaseConfig {
    pub db_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VaultEntry {
    pub notes_dir: Option<PathBuf>,
    #[serde(default)]
    pub db_path: Option<PathBuf>, // legacy field from old [vault]
}
```

- [ ] **Step 2: Restructure ShiotsuchiConfig**

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ShiotsuchiConfig {
    pub database: DatabaseConfig,
    pub vaults: HashMap<String, VaultEntry>,
    pub vault: Option<VaultEntry>,        // legacy [vault] section
    pub indexing: IndexingConfig,
    pub watcher: WatcherConfig,
}
```

- [ ] **Step 3: Add resolution logic**

After deserialization, resolve into flat vectors for downstream use:

```rust
impl ShiotsuchiConfig {
    /// Resolve vault entries: merge legacy [vault] + new [vaults.xxx]
    pub fn resolved_vaults(&self) -> Vec<(String, PathBuf)> {
        let mut vaults: Vec<(String, PathBuf)> = Vec::new();

        // Legacy single vault
        if let Some(ref v) = self.vault {
            if let Some(ref dir) = v.notes_dir {
                vaults.push(("default".to_string(), dir.clone()));
            }
        }

        // Named vaults (new format)
        for (name, entry) in &self.vaults {
            if let Some(ref dir) = entry.notes_dir {
                vaults.push((name.clone(), dir.clone()));
            }
        }

        if vaults.is_empty() {
            vaults.push(("default".to_string(), PathBuf::from(".")));
            eprintln!("[warn] No vaults configured. Using current directory as 'default' vault.");
        }

        vaults
    }

    /// Resolve db_path from [database] or legacy [vault]
    pub fn resolved_db_path(&self) -> PathBuf {
        self.database.db_path.clone()
            .or_else(|| self.vault.as_ref().and_then(|v| v.db_path.clone()))
            .unwrap_or_else(core_default_db_path)
    }
}
```

- [ ] **Step 4: Update load() to warn about old format**

```rust
pub fn load() -> Self {
    let default_path = xdg_config_home().join("shiotsuchi").join("config.toml");
    if default_path.exists() {
        let cfg = Self::load_from(&default_path).unwrap_or_else(|e| {
            eprintln!(
                "Warning: failed to load config from {}: {}. Using defaults.",
                default_path.display(),
                e
            );
            Self::default()
        });
        // Warn about legacy format
        if cfg.vault.is_some() {
            eprintln!(
                "[hint] Your config uses the old [vault] format. Run 'shiotsuchi config-migrate' to upgrade."
            );
        }
        // Warn if no vaults configured
        if cfg.vaults.is_empty() && cfg.vault.is_none() {
            eprintln!("[warn] No vaults configured. Use [vaults.xxx] or [vault] in your config.");
        }
        cfg
    } else {
        Self::default()
    }
}
```

- [ ] **Step 5: Update tests in config.rs** to test both old and new formats.

- [ ] **Step 6: Commit**

```bash
git add cli/src/config.rs
git commit -m "feat(cli): multi-vault config format with backward compat"
```

---

### Task 7: config-migrate Subcommand

**Files:**
- Create: `cli/src/commands/config_migrate.rs`
- Modify: `cli/src/commands/mod.rs`

- [ ] **Step 1: Create config_migrate.rs**

```rust
use crate::config::{self, ShiotsuchiConfig, VaultEntry, DatabaseConfig};
use clap::Args;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Args, Debug)]
pub struct ConfigMigrateArgs {
    /// Path to config file. Defaults to ~/.config/shiotsuchi/config.toml
    #[arg(long)]
    pub config: Option<PathBuf>,
}

pub fn run_config_migrate(args: &ConfigMigrateArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = args.config.clone().unwrap_or_else(config::default_config_path);

    if !config_path.exists() {
        eprintln!("Config file not found: {}", config_path.display());
        return Ok(());
    }

    // Read current config using old format
    let old_cfg = ShiotsuchiConfig::load_from(&config_path)?;

    if old_cfg.vault.is_none() {
        eprintln!("Config is already in new format — no migration needed.");
        return Ok(());
    }

    // Build new config
    let legacy_vault = old_cfg.vault.as_ref().unwrap();
    let new_db_path = old_cfg.database.db_path.clone()
        .or_else(|| legacy_vault.db_path.clone());
    let mut new_vaults: HashMap<String, VaultEntry> = HashMap::new();
    if let Some(ref nd) = legacy_vault.notes_dir {
        new_vaults.insert("default".to_string(), VaultEntry {
            notes_dir: Some(nd.clone()),
            db_path: None,
        });
    }

    let new_cfg = ShiotsuchiConfig {
        database: DatabaseConfig {
            db_path: new_db_path,
        },
        vaults: new_vaults,
        vault: None,
        indexing: old_cfg.indexing,
        watcher: old_cfg.watcher,
    };

    // Create backup
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let backup_path = config_path.with_extension(format!("toml.bak.{}", timestamp));
    fs::copy(&config_path, &backup_path)?;

    // Write new config
    let toml_str = toml::to_string_pretty(&new_cfg)?;
    fs::write(&config_path, toml_str)?;

    // Set permissions (same as init)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600))?;
    }

    eprintln!("Config migrated successfully.");
    eprintln!("Backup saved to: {}", backup_path.display());
    eprintln!("New format written to: {}", config_path.display());

    Ok(())
}
```

- [ ] **Step 2: Add config_migrate to mod.rs**

```rust
pub mod chart;
pub mod config;
pub mod config_migrate;   // NEW
pub mod delete;
pub mod dive;
pub mod dredge;
pub mod init;
pub mod log;
pub mod noise;
pub mod scan;
pub mod setup;
pub mod support;
pub mod tide;
```

- [ ] **Step 3: Commit**

```bash
git add cli/src/commands/config_migrate.rs cli/src/commands/mod.rs
git commit -m "feat(cli): add config-migrate subcommand"
```

---

### Task 8: CLI Plumbing — Update main.rs + Subcommands

**Files:**
- Modify: `cli/src/main.rs`
- Modify: `cli/src/commands/chart.rs`
- Modify: `cli/src/commands/scan.rs`
- Modify: `cli/src/commands/dredge.rs`
- Modify: `cli/src/commands/delete.rs`
- Modify: `cli/src/commands/config.rs`

- [ ] **Step 1: Add ConfigMigrate to Commands enum in main.rs**

```rust
#[derive(Subcommand)]
enum Commands {
    Chart(commands::chart::ChartArgs),
    Config(commands::config::ConfigArgs),
    ConfigMigrate(commands::config_migrate::ConfigMigrateArgs), // NEW
    Delete(commands::delete::DeleteArgs),
    Dive(commands::dive::DiveArgs),
    Dredge(commands::dredge::DredgeArgs),
    Init(commands::init::InitArgs),
    Log,
    Scan(commands::scan::ScanArgs),
    Setup(commands::setup::SetupArgs),
    Support(commands::support::SupportArgs),
    Tide,
}
```

- [ ] **Step 2: Update main() dispatch to use resolved_vaults()**

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ...
    let mut cfg = config::ShiotsuchiConfig::load();
    if let Some(ref dir) = cli.notes_dir {
        // Override first vault's notes_dir (backward compat)
        let vaults = cfg.resolved_vaults();
        if let Some((name, _)) = vaults.first() {
            // Preserve vault name, override path
            let mut new_vaults = cfg.vaults.clone();
            new_vaults.insert(name.clone(), config::VaultEntry {
                notes_dir: Some(dir.clone()),
                db_path: None,
            });
            cfg.vaults = new_vaults;
        }
    }
    if let Some(ref db) = cli.db_path {
        cfg.database.db_path = Some(db.clone());
    }

    let db_path = cfg.resolved_db_path();
    let vaults = cfg.resolved_vaults();

    match cli.command {
        Commands::ConfigMigrate(args) => {
            commands::config_migrate::run_config_migrate(&args)?;
        }
        Commands::Chart(args) => {
            commands::chart::run_chart(&args, &vaults, &db_path, &cfg.indexing)?;
        }
        Commands::Scan(args) => {
            commands::scan::run_scan(&args, &vaults, &db_path, &cfg.watcher, &cfg.indexing)?;
        }
        Commands::Dredge(args) => {
            commands::dredge::run_dredge(&args, &vaults, &db_path, &cfg.indexing)?;
        }
        Commands::Delete(args) => {
            commands::delete::run_delete(&args, &vaults, &db_path)?;
        }
        Commands::Dive(args) => {
            commands::dive::run_dive(&args, &db_path)?;
        }
        Commands::Config(args) => {
            commands::config::run_config(&args, &vaults, &cfg.indexing.include_extensions,
                cfg.indexing.auto_exclude_hidden, cfg.indexing.dynamic_threshold)?;
        }
        Commands::Tide => {
            commands::tide::run_tide(&db_path)?;
        }
        Commands::Log => commands::log::run_log(&db_path)?,
        Commands::Setup(args) => commands::setup::run_setup(&args)?,
        Commands::Init(args) => {
            commands::init::run_init(&args, &cfg, &config::default_config_path(),
                cli.notes_dir.as_deref(), cli.db_path.as_deref())?;
        }
        Commands::Support(args) => {
            commands::support::run_support(&args, &cfg)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Update chart.rs**

```rust
pub fn run_chart(
    args: &ChartArgs,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    indexing_cfg: &IndexingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Build IndexConfig with vaults
    let config = IndexConfig {
        vaults: vaults.to_vec(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
    };
    // ... rest unchanged ...
}
```

- [ ] **Step 4: Update scan.rs**

Same pattern: accept `&[(String, PathBuf)]`, build `IndexConfig` with vaults.

- [ ] **Step 5: Update dredge.rs**

Same pattern: accept `&[(String, PathBuf)]`, iterate vaults for cleanup.

```rust
pub fn run_dredge(
    args: &DredgeArgs,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    indexing_cfg: &IndexingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = IndexConfig {
        vaults: vaults.to_vec(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
    };
    // ... rest uses config ...
}
```

- [ ] **Step 6: Update delete.rs**

Accept vaults, resolve relative path against each vault's notes_dir.

- [ ] **Step 7: Update config.rs subcommand** (noise detection)

Iterate all vaults' notes_dir for `detect-noise`.

- [ ] **Step 8: Update tests** in cli crate to match new signatures.

- [ ] **Step 9: Commit**

```bash
git add cli/src/main.rs cli/src/commands/chart.rs cli/src/commands/scan.rs cli/src/commands/dredge.rs cli/src/commands/delete.rs cli/src/commands/config.rs
git commit -m "feat(cli): update subcommands for multi-vault support"
```

---

### Task 9: MCP — Vaults Support

**Files:**
- Modify: `mcp/src/main.rs`
- Modify: `mcp/src/handler.rs`

- [ ] **Step 1: Update McpConfig in mcp/src/main.rs**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct McpConfig {
    database: Option<DatabaseConfig>,
    vaults: HashMap<String, VaultEntry>,
    vault: Option<VaultEntry>,   // legacy
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DatabaseConfig {
    db_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct VaultEntry {
    notes_dir: Option<PathBuf>,
    db_path: Option<PathBuf>,
}
```

- [ ] **Step 2: Add resolution methods**

```rust
impl McpConfig {
    fn resolved_vaults(&self) -> Vec<(String, PathBuf)> {
        let mut vaults = Vec::new();
        if let Some(ref v) = self.vault {
            if let Some(ref dir) = v.notes_dir {
                vaults.push(("default".to_string(), dir.clone()));
            }
        }
        for (name, entry) in &self.vaults {
            if let Some(ref dir) = entry.notes_dir {
                vaults.push((name.clone(), dir.clone()));
            }
        }
        if vaults.is_empty() {
            vaults.push(("default".to_string(), PathBuf::from(".")));
        }
        vaults
    }

    fn resolved_db_path(&self) -> PathBuf {
        self.database.as_ref()
            .and_then(|d| d.db_path.clone())
            .or_else(|| self.vault.as_ref().and_then(|v| v.db_path.clone()))
            .unwrap_or_else(core_default_db_path)
    }
}
```

- [ ] **Step 3: Update main() to pass vaults**

The `dispatch` function currently takes `(notes_dir, db_path)`. Change to accept `vaults: &[(String, PathBuf)]` and an index or vault name for the first vault.

Actually, for simplicity, keep the existing `dispatch(notes_dir: &Path, db_path: &Path)` for most tools. Only `search_local_notes` needs the vault list. We can add an overloaded dispatch or store vaults in a shared context.

Simpler approach: keep `dispatch` with a single `notes_dir` (using first vault), and pass vaults separately to `handler::call_tool`.

- [ ] **Step 4: Update handler.rs — add vault param to search**

```rust
pub fn call_tool(
    name: &str,
    args: &Value,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    match name {
        "search_local_notes" => {
            // ...
            let vault_filter = args["vault"].as_str();
            // Use first vault's notes_dir for path traversal check
            let notes_dir = &vaults.first().ok_or("No vaults configured")?.1;
            // ...
            let results = search(&db, &tokenizer, &query, limit, mode, None, min_score, vault_filter)?;
            // ...
        }
        // other tools unchanged
    }
}
```

- [ ] **Step 5: Update spawn_rebuild** to use vaults list

```rust
fn spawn_rebuild(
    vaults: Vec<(String, PathBuf)>,
    db_path: &Path,
    stdout: &Arc<Mutex<dyn io::Write + Send>>,
    _args: &serde_json::Value,
    progress_token: Option<u64>,
) {
    // ...
    let config = IndexConfig {
        vaults,
        ..Default::default()
    };
    // ...
}
```

- [ ] **Step 6: Update tests** in mcp crate to match new signatures.

- [ ] **Step 7: Commit**

```bash
git add mcp/src/main.rs mcp/src/handler.rs
git commit -m "feat(mcp): multi-vault support in config and handler"
```

---

### Task 10: Fix References Across All Crates

**Files:**
- Modify: `core/benches/search_bench.rs`
- Modify: `core/tests/integration_test.rs`
- Modify: `e2e/src/lib.rs`
- Modify: `cli/src/commands/support.rs`
- Modify: `cli/src/commands/init.rs`
- Modify: all test code creating Chunk or IndexConfig instances

- [ ] **Step 1: Update benchmarks** — add vault_name to Chunk, vaults to IndexConfig

- [ ] **Step 2: Update integration tests** — match new signatures

- [ ] **Step 3: Update e2e tests** — pass vault context

- [ ] **Step 4: Update support.rs** — vaults in output

- [ ] **Step 5: Build and fix compilation errors**

```bash
cargo build 2>&1
```

Fix any remaining compile errors iteratively.

- [ ] **Step 6: Run tests**

```bash
make test 2>&1
```

Fix any test failures.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "fix: update all references for multi-vault support"
```
