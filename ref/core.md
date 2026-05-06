# Core Library (shiotsuchi-core)

Crate path: `core/`
Published name: `shiotsuchi-core`

## Modules

### `db.rs` — Database Operations

**Type**: `NoteDatabase { conn: Connection }`

**Key Methods**:
- `open(path)` — Opens SQLite DB, enables WAL mode, initializes schema
- `open_in_memory()` — In-memory DB for testing
- `upsert_note(path, title, tokenized_body, hash, mtime)` — Insert/update with hash skip optimization. Wraps FTS delete + insert + meta upsert in a transaction.
- `delete_note(path)` — Delete from both `notes_fts` and `notes_meta`. Transaction-wrapped.
- `search(fts5_query, limit)` — Execute FTS5 MATCH query with BM25 ranking. Uses parameter binding for both query and limit.
- `get_metadata(path)` — Lookup single note metadata
- `list_paths()` — All indexed paths
- `list_all_metadata()` — All metadata ordered by `indexed_at DESC`
- `stats()` — Vault statistics (total notes, DB size, last indexed)

**Schema**:
```sql
-- FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE notes_fts USING fts5(
    path UNINDEXED,
    title,
    body,
    tokenize='unicode61 remove_diacritics 0'
);

-- Metadata tracking table
CREATE TABLE notes_meta (
    path TEXT PRIMARY KEY,
    hash TEXT NOT NULL,
    mtime INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL,
    title TEXT
);

CREATE INDEX idx_notes_meta_hash ON notes_meta(hash);
```

**Error Type**: `DbError { Sqlite(rusqlite::Error), NotFound(String) }`

### `tokenizer.rs` — Japanese Tokenization

**Type**: `JapaneseTokenizer { predictor: Predictor, config: TokenizerConfig }`

**Construction**:
1. `EMBEDDED_PREDICTOR_BYTES` (compile-time embedded) → `deserialize_from_slice_unchecked`
2. `SHIOTSUCHI_MODEL_PATH` env var → decompress → `Model::read` → `Predictor::new`
3. None available → `TokenizerError::NoModel`

**Key Methods**:
- `split(text)` → space-separated tokenized string (for FTS5 body column)
- `and_query(text)` → `"東京" AND "検索" AND "エンジン"` (for FTS5 MATCH)
- `or_query(text)` → `"東京" OR "検索"` (for future OR search)

**Global Cache**:
- `static TOKENIZER: OnceLock<Arc<JapaneseTokenizer>>`
- `get_tokenizer()` — Returns cached instance (avoids ~500ms init cost)

**Fallbacks** (for testing without model):
- `simple_tokenize(text)` — whitespace split
- `simple_and_query(text)` — simple FTS5 AND query

### `indexer.rs` — File Indexing

**Key Functions**:
- `index_directory(db, tokenizer, config)` → Walk vault, index all matching files
- `index_file(db, tokenizer, file_path, relative_path, config)` → Index single file
- `cleanup_deleted(db, config)` → Remove DB entries for deleted files
- `extract_frontmatter(content)` → Parse YAML frontmatter, extract title
- `markdown_to_text(markdown)` → Strip markup to plain text
- `title_from_path(path)` → Derive title from filename

**Flow**:
```
Read file ──► Extract frontmatter ──► Markdown to text ──► Tokenize ──► Upsert to DB
              (YAML title)              (strip markup)      (Vaporetto)   (hash check)
```

**Hash**: SHA-256 of raw file content (used for change detection)

### `search.rs` — Search Engine

**Key Functions**:
- `search(db, tokenizer, notes_dir, query, limit)` → FTS5 search + snippet extraction

**Snippet Extraction** (`extract_snippet`):
- Finds first matching token position
- Extracts `max_lines * 2 + 1` lines around match
- Falls back to first 200 chars if no match
- Truncates at 500 chars

**Security**: Path traversal protection via `canonicalize` + `starts_with` vault check

### `models.rs` — Data Structures

| Type | Fields |
|------|--------|
| `NoteMetadata` | path, hash, mtime, indexed_at, title |
| `SearchResult` | path, title, snippet, score |
| `VaultStats` | total_notes, total_size_bytes, last_indexed_at, db_path |
| `IndexResult` | Inserted / Updated / Skipped / Error(String) |
| `IndexConfig` | notes_dir, include_extensions, exclude_patterns |

### `watcher.rs` — File System Watcher

**Type**: `VaultWatcher { db: Arc<Mutex<NoteDatabase>>, tokenizer: Arc<JapaneseTokenizer>, config: IndexConfig }`

**Behavior**:
- Uses `notify` crate for cross-platform file watching
- Handles: Create, Modify (data), Remove, Rename
- Incremental re-indexing on change
- Requires `watcher` feature flag (enabled by default)

## Build-time Model Embedding

`core/build.rs`:
- Checks `SHIOTSUCHI_MODEL_PATH` env var at build time
- Reads `.model.zst` file, decompresses if needed
- Serializes `Predictor` via `serialize_to_vec()`
- Generates `embedded_model.rs` containing `EMBEDDED_PREDICTOR_BYTES: Option<&[u8]>`

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `watcher` | yes | Enables file system watcher via `notify` crate |

## Testing Strategy

- Model-optional tests: Try `JapaneseTokenizer::new()`, skip if `Err`
- CI: Set `SHIOTSUCHI_MODEL_PATH` for full tests
- In-memory DB for unit tests
- `tempfile` for disk-based DB tests
