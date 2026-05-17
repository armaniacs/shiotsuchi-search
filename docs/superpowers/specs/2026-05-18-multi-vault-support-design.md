# Multi-Vault Support Design

> **Status:** Draft  
> **Date:** 2026-05-18  
> **Branch:** `feature-vault-detail`

## 1. Goals

Allow the user to configure multiple notes directories ("vaults") in `config.toml`,
index/search/watch all of them from a single shared SQLite database,
and migrate existing single-vault configurations seamlessly.

## 2. Config Format

### New Format

```toml
[database]
db_path = "/Users/yaar/.cache/shiotsuchi/db.sqlite3"

[vaults.work]
notes_dir = "/Users/yaar/Documents/work-notes"

[vaults.personal]
notes_dir = "/Users/yaar/Documents/personal-notes"

[indexing]
snippet_lines = 3
max_snippet_chars = 1000
include_extensions = ["md", "markdown"]
exclude_dirs = ["node_modules"]
auto_exclude_hidden = true
follow_links = false
dynamic_threshold = 5

[watcher]
enabled = true
```

### Old Format (Legacy, still readable)

```toml
[vault]
notes_dir = "/Users/yaar/Documents/notes"
db_path = "/Users/yaar/.cache/shiotsuchi/db.sqlite3"
```

### Backward Compatibility on Read

When the new code reads an old-format config:
- `[vault]` section with `notes_dir` is treated as a vault named `"default"`.
- `[vault].db_path` populates `[database].db_path` if `[database]` is absent.
- A warning is printed suggesting the user run `shiotsuchi config-migrate`.

### `shiotsuchi config-migrate` Subcommand

A new CLI subcommand that rewrites the config file from old to new format.

**Behavior:**
1. Reads `~/.config/shiotsuchi/config.toml` using the old deserialization path.
2. Constructs the new config structure:
   - `[vault].notes_dir` → `[vaults.default].notes_dir`
   - `[vault].db_path` → `[database].db_path`
   - All other sections (`[indexing]`, `[watcher]`) pass through unchanged.
3. Creates a timestamped backup of the original file (same as `init --backup` pattern).
4. Writes the new format to `config.toml`.
5. On parse failure, reports errors and leaves the original file intact.

## 3. Database Schema Migration

### Current Schema (v1)

```sql
CREATE TABLE notes_meta (
    path TEXT PRIMARY KEY,
    hash TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL,
    title TEXT NOT NULL DEFAULT ''
);

CREATE TABLE chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    parent_header TEXT,
    content TEXT NOT NULL,
    tokenized_content TEXT NOT NULL,
    FOREIGN KEY (file_path) REFERENCES notes_meta(path)
);

CREATE TABLE file_cache (
    path TEXT PRIMARY KEY,
    hash TEXT NOT NULL
);
```

### New Schema (v2)

`vault_name TEXT NOT NULL` columns added. Uniqueness shifts to `(vault_name, path)`.

```sql
CREATE TABLE notes_meta (
    vault_name TEXT NOT NULL DEFAULT 'default',
    path TEXT NOT NULL,
    hash TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (vault_name, path)
);

CREATE TABLE chunks (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    vault_name TEXT NOT NULL DEFAULT 'default',
    file_path TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    parent_header TEXT,
    content TEXT NOT NULL,
    tokenized_content TEXT NOT NULL
);

CREATE TABLE file_cache (
    vault_name TEXT NOT NULL DEFAULT 'default',
    path TEXT NOT NULL,
    hash TEXT NOT NULL,
    PRIMARY KEY (vault_name, path)
);
```

### Migration Strategy

Implemented in `db.rs`:

1. A `schema_version` key in a `_meta` table (or PRAGMA user_version) tracks the version.
2. On `open()`, check `user_version`:
   - `0` → legacy schema detected. Run `ALTER TABLE` statements to add `vault_name` columns.
     Set `user_version = 1` (v1) then `user_version = 2` (v2) after drops.
   - `1` → was already migrated to FTS5-only, needs vault_name migration.
   - `2` → current, no action.
3. After ALTER TABLE, drop and recreate primary key constraints:
   - For SQLite, `PRAGMA writable_schema = ON` is not safe. Instead, use a migration table:
     - Create new tables with correct PKs.
     - `INSERT INTO new_* SELECT 'default', * FROM old_*`
     - `DROP TABLE old_*`
     - Rename new tables to old names.
4. All inserts/updates going forward include `vault_name`.

## 4. Core Model Changes

### `IndexConfig`

```rust
pub struct IndexConfig {
    pub vaults: Vec<(String, PathBuf)>,  // (vault_name, notes_dir)
    pub include_extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub auto_exclude_hidden: bool,
    pub follow_links: bool,
    pub dynamic_threshold: usize,
}
```

- `notes_dir: PathBuf` removed. Replace with `vaults`.
- Backward compat: `IndexConfig::from_vault(name: &str, notes_dir: PathBuf) -> Self`.
- Default: single vault `("default", PathBuf::from("."))`.

### `ChunkSearchResult`

```rust
pub struct ChunkSearchResult {
    pub vault_name: String,       // NEW
    pub chunk_id: i64,
    pub file_path: String,
    pub parent_header: Option<String>,
    pub content: String,
    pub score: f64,
    pub search_mode: SearchMode,
}
```

## 5. Indexer Changes

### Signature

```rust
pub fn index_directory(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
    embedder: Option<&Embedder>,
    progress: Option<IndexProgress>,
) -> Result<(Vec<(String, String, IndexResult)>, usize), DbError>
//                ^^^^^^^^ vault_name
```

### Algorithm

```rust
for (vault_name, notes_dir) in &config.vaults {
    // Canonicalize notes_dir (with follow_links support)
    // WalkDir::new(notes_dir) with all existing filters
    // For each file:
    //   - Compute relative path within this vault
    //   - Call index_file_with_embedder(db, tokenizer, embedder, file_path, vault_name, relative_path, config)
}
```

### Internal Helpers

- `index_file_with_embedder()` gains `vault_name: &str` parameter.
- `index_file()` gains `vault_name: &str` parameter.
- All DB interactions (insert_chunks, file_cache upsert) pass `vault_name`.

## 6. DB Changes (`db.rs`)

### Methods

- `insert_chunks(chunks: &[Chunk], vault_name: &str)` — batch insert with vault_name.
- `delete_chunks_for_file(vault_name: &str, relative_path: &str)` — delete by vault+path.
- `cached_hash(vault_name: &str, relative_path: &str) -> Option<String>`.
- `upsert_file_cache(vault_name: &str, relative_path: &str, hash: &str)`.
- `delete_file_cache(vault_name: &str, relative_path: &str)`.
- `get_surrounding_chunks(chunk_id, window)` — unchanged (chunk_id is global).
- `stats()` — include vault breakdown.

### FTS5

The FTS5 `notes_fts` table currently indexes `file_path` and `body`.
We add `vault_name` as an unindexed column so FTS5 results can be filtered:

```sql
CREATE VIRTUAL TABLE notes_fts USING fts5(
    vault_name UNINDEXED,
    file_path UNINDEXED,
    body,
    content=chunks,
    content_rowid=id
);
```

This requires rebuilding the FTS5 table during migration.

## 7. Watcher Changes (`watcher.rs`)

- `VaultWatcher` holds a list of `(vault_name, PathBuf)` for all vaults.
- `watch()` creates one `notify::recommended_watcher()` per vault.
- Event handler resolves which vault a file belongs to by checking `is_path_within_vault()` against each vault.
- `handle_event()` receives `vault_name` and passes it to all DB operations.

## 8. Search Changes (`search.rs`)

### Signature

```rust
pub fn search(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    query: &str,
    limit: usize,
    mode: SearchMode,
    embedder: Option<&Embedder>,
    min_score: Option<f64>,
    vault_filter: Option<&str>,  // NEW
) -> Result<Vec<ChunkSearchResult>, DbError>
```

### FTS5 Query

- If `vault_filter` is Some, append `vault_name:{name}` to the FTS5 query.
- `vault_name` is stored as an UNINDEXED column in FTS5, but we can use an `=` comparison
  by joining `notes_fts` with `chunks` and filtering on `chunks.vault_name`.

### Result

- `ChunkSearchResult.vault_name` populated from the DB row.

## 9. CLI Changes

### `cli/src/main.rs`

- `Commands::ConfigMigrate` variant added.
- All subcommand dispatch that currently uses `cfg.vault.notes_dir`:
  - If single vault (backward compat): use `vaults[0].1`.
  - Build `IndexConfig` from all vaults.

### `cli/src/commands/config_migrate.rs` (NEW)

- Full subcommand implementation.
- Reads config using old deserialization.
- Constructs new config, creates backup, writes new format.

### `cli/src/config.rs`

- `VaultConfig` kept but made optional.
- `DatabaseConfig` new struct for `[database]`.
- `VaultsConfig` new struct: `HashMap<String, VaultEntry>`.
- `ShiotsuchiConfig.vault` → optional; `ShiotsuchiConfig.database` + `ShiotsuchiConfig.vaults` added.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DatabaseConfig {
    pub db_path: Option<PathBuf>,
}

/// A single vault entry used by both old `[vault]` and new `[vaults.xxx]`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct VaultEntry {
    pub notes_dir: Option<PathBuf>,
    // Legacy: old [vault] held db_path here; ignored in [vaults.xxx]
    #[serde(default)]
    pub db_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ShiotsuchiConfig {
    // New: [database] section
    pub database: DatabaseConfig,
    // New: [vaults.xxx] entries — TOML table-of-tables maps to HashMap
    pub vaults: HashMap<String, VaultEntry>,
    // Legacy: [vault] section (single vault, old format)
    pub vault: Option<VaultEntry>,
    pub indexing: IndexingConfig,
    pub watcher: WatcherConfig,
}
```

TOML deserialization of `[vaults.xxx]` works naturally:
```rust
// [vaults.work]  → vaults["work"] = VaultEntry { notes_dir: Some(...), .. }
// [vaults.home]  → vaults["home"] = VaultEntry { notes_dir: Some(...), .. }
// [vault]        → vault = Some(VaultEntry { notes_dir: Some(...), db_path: Some(...) })
```

### `--notes-dir` CLI flag

- Kept for backward compatibility.
- When specified, overrides the `notes_dir` of the **first** vault.
- If no vaults exist, creates a `"default"` vault with the specified dir.

## 10. MCP Changes

### `mcp/src/main.rs`

- `McpConfig.notes_dir` becomes `McpConfig.vaults: HashMap<String, VaultEntry>`.
- `spawn_rebuild()` accepts `IndexConfig` with vaults.
- `resolve_path_env` for `SHIOTSUCHI_NOTES_DIR` maps to first vault.

### `mcp/src/handler.rs`

- `search_local_notes` gains optional `vault` argument.
- `notes_dir` parameter becomes `(vaults, default_vault_name)` or similar.

## 11. File Change Summary

| File | Change Type |
|---|---|
| `cli/src/config.rs` | New `DatabaseConfig`, `VaultEntry`; `ShiotsuchiConfig` restructured |
| `cli/src/commands/config_migrate.rs` | **New file** |
| `cli/src/commands/mod.rs` | Add `config_migrate` module |
| `cli/src/commands/chart.rs` | Build `IndexConfig` from vaults |
| `cli/src/commands/scan.rs` | Build `IndexConfig` from vaults |
| `cli/src/commands/dredge.rs` | Iterate vaults for cleanup |
| `cli/src/commands/delete.rs` | Accept vault context |
| `cli/src/commands/config.rs` | Scan all vaults for detect-noise |
| `cli/src/main.rs` | Add `ConfigMigrate` command; vaults plumbing |
| `core/src/models.rs` | `IndexConfig.vaults`, `ChunkSearchResult.vault_name` |
| `core/src/db.rs` | Schema v2 migration, vault_name params |
| `core/src/indexer.rs` | Vault loop, vault_name propagation |
| `core/src/search.rs` | vault_filter, vault_name in results |
| `core/src/watcher.rs` | Multi-vault watchers |
| `mcp/src/main.rs` | Vaults support |
| `mcp/src/handler.rs` | Vault filter in search |

## 12. Out of Scope (Future)

- Per-vault exclude_dirs / indexing config.
- `vault:` prefix syntax in search query strings.
- Per-vault watcher enable/disable.
- Vault CRUD via CLI (add/remove/list vaults).
