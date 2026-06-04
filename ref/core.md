# Core Library (shiotsuchi-core)

Crate path: `core/`
Published name: `shiotsuchi-core`

## Modules

### `db.rs` — Database Operations

**Type**: `NoteDatabase { write_conn: RefCell<Connection> }`

**Schema** (v11, created by `create_schema()` + migrations):

```sql
-- File cache for incremental indexing (hash + mtime + size + backlink tracking)
CREATE TABLE IF NOT EXISTS file_cache (
    vault_name      TEXT NOT NULL,
    path            TEXT NOT NULL,
    hash            TEXT NOT NULL,
    mtime           INTEGER NOT NULL,
    model_id        TEXT NOT NULL,
    file_size       INTEGER NOT NULL DEFAULT 0,
    backlink_count  INTEGER NOT NULL DEFAULT 0,
    char_count      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (vault_name, path)
);

-- Chunk storage
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

-- Task checkboxes extracted from notes (- [ ] / - [x])
CREATE TABLE IF NOT EXISTS tasks (
    id          INTEGER PRIMARY KEY,
    vault_name  TEXT NOT NULL,
    file_path   TEXT NOT NULL,
    content     TEXT NOT NULL,
    checked     INTEGER NOT NULL DEFAULT 0,
    line_number INTEGER NOT NULL DEFAULT 0,
    indexed_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Note-to-note links (wikilinks [[...]] resolving to file paths)
CREATE TABLE IF NOT EXISTS note_links (
    source_path TEXT NOT NULL,
    target_path TEXT NOT NULL,
    vault_name  TEXT NOT NULL,
    PRIMARY KEY (source_path, target_path, vault_name)
);
CREATE INDEX IF NOT EXISTS idx_note_links_target ON note_links(target_path, vault_name);

-- Tag frequency cache for O(1) stats (populated incrementally during indexing)
CREATE TABLE IF NOT EXISTS tag_counts (
    tag        TEXT NOT NULL,
    vault_name TEXT NOT NULL,
    count      INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (tag, vault_name)
) WITHOUT ROWID;

-- FTS5 virtual table for keyword search (external content table)
CREATE VIRTUAL TABLE IF NOT EXISTS fts_chunks USING fts5(
    tokenized_content,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 0'
);

-- Vec0 virtual table for vector KNN search
CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
    chunk_id  INTEGER PRIMARY KEY,
    embedding FLOAT[1024]
);
```

**Key Methods**:
- `open(path)` / `open_in_memory()` — Open SQLite DB, enable WAL, register sqlite-vec extension, run migrations
- `open_readonly(path)` — Read-only connection (for MCP search handlers)
- `reindex_file(p: &ReindexParams)` — Delete old chunks and insert new ones in a single atomic transaction, including tasks, note_links, tag_counts, and char_count
- `insert_chunks(chunks)` — Insert chunk batch in transaction, returns assigned IDs (reads `vault_name` from each chunk)
- `insert_embeddings(pairs)` — Insert (chunk_id, embedding) pairs for vector search
- `delete_file_fully(vault_name, path)` — Atomically remove all data for a file: tag_counts, chunks, FTS/vec, tasks, file_cache, note_links — all in one transaction
- `delete_chunks_for_file(vault_name, file_path)` — Remove all chunks/FTS/vec/tasks entries for a file in a specific vault
- `delete_file_cache(vault_name, path)` / `delete_note_links_for_source(path, vault)` — Targeted cleanup
- `fts_search(fts5_query, limit)` — Execute FTS5 MATCH with BM25 ranking (results joined with chunks for vault_name)
- `vec_search(embedding, limit, vault_filter, include_embeddings)` — Execute vec0 KNN search with cosine distance
- `get_chunks_by_ids(ids)` — Fetch chunks by IDs, preserving order (includes vault_name)
- `get_surrounding_chunks(chunk_id, window)` — Fetch chunks before/after a given chunk (for context, includes vault_name)
- `get_chunk_vault_name(chunk_id)` — Return vault_name for a chunk (used for MCP vault auth check)
- `cached_hash(vault_name, path)` / `upsert_file_cache(vault_name, ..., char_count)` / `delete_file_cache(vault_name, path)` — Per-vault incremental index tracking (char_count must be provided)
- `list_cached_paths(vault_name)` — Indexed file paths for a specific vault
- `get_dominant_model_id()` — Returns the most common non-"none" `model_id` stored in `file_cache`
- `stats()` — Vault statistics (total_chunks, total_files, vec_indexed_chunks, db_path, total_chars from file_cache.char_count, top_tags from tag_counts, etc.)
- `tag_stats(limit)` — Returns top N tags by frequency (reads from tag_counts table, not chunks)
- `insert_tasks(vault_name, file_path, tasks)` / `delete_tasks_for_file(vault_name, file_path)` — Task list management
- `query_tasks(keyword, include_checked)` — Search tasks with optional keyword filter
- `get_tags_for_file(vault_name, path)` / `decrement_tag_count(vault_name, tag)` — Tag counts maintenance helpers
- `update_backlink_counts_for_vault(vault_name)` — Recalculate backlink counts for all files in a vault
- `migrate()` — Schema migration (v1→v11, crash-safe with versioned blocks)

**Error Type**: `DbError { Sqlite(rusqlite::Error), NotFound(String), Io(std::io::Error), Other(String) }`

### `tokenizer.rs` — Japanese Tokenization

**Type**: `JapaneseTokenizer { predictor: Predictor, config: TokenizerConfig }`

**Construction**:
1. `EMBEDDED_PREDICTOR_BYTES` (compile-time embedded) → `deserialize_from_slice_unchecked`
2. `SHIOTSUCHI_MODEL_PATH` env var → decompress → `Model::read` → `Predictor::new`
3. None available → `TokenizerError::NoModel`

**Key Methods**:
- `split(text)` → space-separated tokenized string (for FTS5 body column)
- `tokenize_content(text, is_code)` → Whitespace-split if `is_code=true`, else Vaporetto-based
- `and_query(text)` → `"東京" AND "検索" AND "エンジン"` (**deprecated**: use `collect_tokens` + `expand_synonyms`)
- `collect_tokens(text)` → Collect unique tokens from text (replaces `and_query`)
- `or_query(text)` → `"東京" OR "検索"` (for OR search)

**Global Cache**:
- `static TOKENIZER: OnceLock<Arc<JapaneseTokenizer>>`
- `get_tokenizer()` — Returns cached instance (avoids ~500ms init cost)

**Fallbacks** (for testing without model):
- `simple_tokenize(text)` — whitespace split
- `simple_and_query(text)` — simple FTS5 AND query

### `chunker.rs` — Markdown Chunking (RAG)

**Type**: Free functions only.

**Key Function**:
- `split_into_chunks(markdown, tokenizer, file_path, vault_name)` → `Vec<Chunk>`

**Algorithm**:
1. **Level 1**: Split on Markdown headers (`#`, `##`, `###`). Builds a hierarchy stack of parent headers.
2. **Level 2**: Sections exceeding 1000 chars are further split on blank-line boundaries (paragraphs).
3. Fenced code blocks (` ``` `) are never split internally.
4. Each chunk receives `parent_header` set to the ancestor heading path (e.g. `"Section 1 > Subsection A"`).
5. Each chunk receives `vault_name` for multi-vault tracking.

### `embedder.rs` — ONNX Embedding Inference (RAG)

**Type**: `Embedder { session: RefCell<Session>, tokenizer: Tokenizer, model_id: String }`

**Construction**:
- `Embedder::load(model_path)` — Load ONNX model + HuggingFace tokenizer from `model.onnx` / `tokenizer.json`
- Expects `tokenizer.json` alongside the ONNX model
- Computes SHA-256 hash of model file as `model_id`

**Key Methods**:
- `embed(texts)` → `Vec<Vec<f32>>` — Batched embedding with mean pooling + L2 normalization
- `flush()` — Clear embedding cache (reclaims memory)
- `model_id()` — Returns the SHA-256 hex string of the loaded ONNX file

**Output Handling**:
- If model outputs `sentence_embedding`: used directly (already pooled)
- If model outputs `last_hidden_state`: applies mean pooling + L2 normalization

**Error Type**: `EmbedderError`

**Status**: `EmbedderStatus::Ready` or `EmbedderStatus::Unavailable(reason)`

### `indexer.rs` — File Indexing (Chunk-aware)

**Key Functions**:
- `index_directory(db, tokenizer, config, embedder, progress)` → Walk all configured vaults, index all matching files. Builds `path_map` once per vault for O(1) wikilink resolution. Calls `update_backlink_counts_for_vault` after indexing. Progress is cumulative across vaults.
- `index_file_with_embedder(p: &IndexParams)` → Index single file: read → split into chunks → FTS insert → optional embedding insert → tasks + note_links + tag_counts
- `index_file(db, tokenizer, file_path, vault_name, relative_path, config)` → Convenience wrapper (embedder=None)
- `cleanup_deleted(db, config)` → Remove DB entries for deleted files across all vaults using `delete_file_fully` (atomic)
- `build_path_map(vault_paths)` → Build `HashMap<String, String>` mapping lowercase stem → shortest path for O(1) wikilink resolution
- `extract_frontmatter(content)` → Parse YAML frontmatter, extract title/tags/date
- `extract_tasks(content)` → Scan for `- [ ]` / `- [x]` task markers and extract task text
- `extract_emphasized(content)` → Extract text from `==highlight==` and `**bold**` markers
- `extract_wikilinks(content)` → Extract wikilink targets (`[[...]]`) from content
- `load_shiotsuchiignore(vault_dir)` → Read `.shiotsuchiignore` from vault root, return pattern list
- `check_ignore(relative_path, patterns)` → Verify if a path matches any exclude pattern, returns `Err(pattern)` if excluded
- `markdown_to_text(markdown)` → Strip markup to plain text
- `title_from_path(path)` → Derive title from filename
- `sha256_hex(content)` → SHA-256 of raw file content

**Flow**:
```
For each (vault_name, notes_dir) in config.vaults:
  WalkDir(notes_dir) → filter extensions/excludes
  For each file:
    Read → Extract frontmatter → Split into chunks (with vault_name)
    → Delete old chunks for vault+path → Insert new chunks → Insert embeddings
```

**Progress type**: `IndexProgress = Box<dyn Fn(usize, Option<usize>) + Send + 'static>`
Progress is cumulative: `(processed_so_far, total_across_all_vaults)`.
`total` is `None` when total file count is unknown (pre-count walk was removed for efficiency).

### `search.rs` — Search Engine

**Key Function**:
- `search(db, tokenizer, query, limit, mode, embedder, min_score, vault_filter, tag_filter, since_date, user_dictionary, synonyms, fuzzy, alpha, mmr, lambda, backlink_scoring)` → `Result<Vec<ChunkSearchResult>>`

**Modes** (`SearchMode` enum):
- `Fts` — Keyword search via FTS5 BM25 (works without model). Lower score = more relevant.
- `Vec` — Semantic search via embedding + vec0 KNN (requires model). Lower distance = more relevant.
- `Hybrid` — Reciprocal Rank Fusion (RRF) merge of FTS + Vec results. Supports alpha-weighted blending via `--alpha`.

**Parameters**:
- `query`: Raw search query text (tokenized internally for FTS)
- `limit`: Max results (1–50 recommended for MCP)
- `mode`: Search strategy
- `embedder`: Optional ONNX embedder (required for Vec/Hybrid modes)
- `min_score`: Optional threshold — FTS/Vec excludes `score > min_score`, Hybrid excludes `score < min_score`
- `vault_filter`: Optional vault name to restrict results to a single vault (`None` = all vaults)
- `tag_filter`: Optional comma-separated tag string to filter results (empty/none = no filter)
- `since_date`: Optional ISO 8601 date string for minimum frontmatter date filter
- `user_dictionary`: Custom dictionary entries for Vaporetto tokenization during query analysis
- `synonyms`: Synonym/thesaurus map for FTS5 query OR-expansion
- `fuzzy`: When true, applies Unicode NFKC normalization + ASCII lowercasing to the query
- `alpha`: Optional hybrid blend ratio (0.0 = vec only, 1.0 = FTS only, None = RRF)
- `mmr`: When true, applies Maximal Marginal Relevance diversity re-ranking
- `lambda`: MMR trade-off (0.0 = max diversity, 1.0 = pure relevance)

**Snippet Extraction** (`extract_snippet`):
- Finds first matching token position
- Extracts `max_lines * 2 + 1` lines around match
- Falls back to first `max_chars` chars if no match
- Truncates at `max_chars` chars (configurable, default 1000, clamped 128–65535)

**Security**: Path traversal protection via `canonicalize` + `starts_with` vault check

### `models.rs` — Data Structures

| Type | Fields |
|------|--------|
| `Chunk` | id, vault_name, file_path, chunk_index, parent_header, content, tokenized_content, tags, frontmatter_date, title, emphasized_text |
| `ChunkSearchResult` | vault_name, chunk_id, file_path, parent_header, content, score, search_mode, tags, frontmatter_date, title, emphasized_text |
| `SearchMode` | `Fts` / `Vec` / `Hybrid` (default) |
| `EmbedderStatus` | `Ready` / `Unavailable(String)` |
| `NoteMetadata` | path, hash, mtime, indexed_at, title |
| `Task` | id, vault_name, file_path, content, checked (bool), line_number |
| `VaultStats` | total_chunks, total_files, total_size_bytes, last_indexed_at, db_path, vec_indexed_chunks, embedder_status, total_chars, top_tags |
| `SearchConfig` | max_snippet_chars (128–65535, default 1000) |
| `IndexConfig` | vaults, include_extensions, exclude_dirs, auto_exclude_hidden, follow_links, dynamic_threshold, backlink_scoring, enable_pdf_extraction, vlm_config, thesaurus_path, user_dictionary_path, hybrid_alpha |
| `IndexResult` | `Inserted` / `Updated` / `Skipped` / `Error(String)` |
| `ReindexParams` | vault_name, relative_path, hash, mtime, model_id, chunks, embeddings, file_size, tasks, note_link_targets (params for `reindex_file`) |
| `IndexParams` | db, tokenizer, embedder, file_path, vault_name, relative_path, config, path_map (params for `index_file_with_embedder`) |
| `Config` | synonyms: HashMap, vault_default: Option\<String\>, hybrid_alpha: Option\<f64\>, semantic_threshold: Option\<f64\>, embedder: EmbedderConfig |
| `EmbedderConfig` | `BuiltIn` (default) / `OnnxFile { path: PathBuf }` / `Api { endpoint, model, api_key }` — embedding model provider; see `[embedder]` config section |

### `watcher.rs` — File System Watcher

**Type**: `VaultWatcher { db, tokenizer, config: IndexConfig, embedder, watchers }`

**Behavior**:
- Creates one `notify` watcher per configured vault
- Events carry vault_name for correct routing to DB operations
- Handles: Create, Modify (data), Remove, Rename
- Incremental re-indexing on change by calling `index_file()`
- `resolve_vault_for_path(path)` → finds which vault a path belongs to (symlink-safe)
- Requires `watcher` feature flag (enabled by default)

### `build_info.rs` — Compile-time Build Information

**Constants**:
- `HAS_MODEL_EMBEDDED: bool` — Vaporetto model embedded at build time
- `EMBEDDED_MODEL_HASH: &str` — SHA-256 hex of embedded predictor
- `FEATURE_WATCHER: bool` / `FEATURE_ASYNC_INDEX: bool` — Feature flags
- `DEP_*` constants — Dependency configurations (bundled SQLite, vaporetto features, etc.)

## Build-time Model Embedding

`core/build.rs`:
- Checks `SHIOTSUCHI_EMBED_MODEL` env var at build time
- Reads `.model.zst` file, decompresses if needed
- Serializes `Predictor` via `serialize_to_vec()`
- Generates `embedded_model.rs` containing `EMBEDDED_PREDICTOR_BYTES: Option<&[u8]>` and `EMBEDDED_PREDICTOR_HASH`

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `default` | yes | Enables all default features (watcher, async-index, semantic, pdf) |
| `watcher` | yes | Enables file system watcher via `notify` crate |
| `async-index` | yes | Enables parallel indexing via `rayon` |
| `semantic` | yes | Enables ONNX embedding/vector search via `ort` and `tokenizers` crates |
| `pdf` | yes | Enables PDF text extraction via `pdfium-render` and `pdfium-auto` |
| `vlm` | no | Enables VLM-based PDF markdown extraction via `edgequake-pdf2md` |

## Schema Migrations

| Version | Change |
|---------|--------|
| v1 | Original schema with `notes_meta` + `notes_fts` tables (dropped) |
| v2 | Current chunk-based schema: `chunks`, `file_cache`, `fts_chunks`, `vec_chunks` |
| v3 | Added `vault_name` column to `chunks` and `file_cache`; composite PK on `file_cache(vault_name, path)` |
| v4 | Added `file_size` column to `file_cache` for two-stage mtime+size skip |
| v5 | Added `tags`, `frontmatter_date`, `title` columns to `chunks` for frontmatter metadata |
| v6 | Added `emphasized_text` column to `chunks` for highlighted/bold text detection |
| v7 | Added `tasks` table for task checkbox extraction (`- [ ]` / `- [x]`) |
| v8 | Defensive column adds (consolidated) |
| v9 | Added `note_links` table and `backlink_count` column to `file_cache` |
| v10 | Added `char_count` column to `file_cache` and `tag_counts` table for O(1) stats |
| v11 | Added `vlm_hash` column to `file_cache` for VLM extraction caching |

The v2→v3 migration is crash-safe: it checks for the column before adding it, and wraps the full migration in a transaction. Migrations v4→v4 (vec_chunks recreation), v8→v9, v9→v10, and v10→v11 are also wrapped in transactions.

## Virtual Table Integrity Management

FTS5 and vec0 virtual tables do not support SQLite foreign key constraints. This means there is no automatic cascading delete when rows are removed from the backing `chunks` table. The application layer must manually manage consistency across all three tables (`chunks`, `fts_chunks`, `vec_chunks`).

`delete_file_fully()` in `core/src/db.rs` is the canonical method for atomic multi-table deletion. It:

1. Decrements `tag_counts` for the file's tags
2. Collects chunk IDs, then deletes corresponding rows from `fts_chunks` and `vec_chunks`
3. Deletes from `chunks`, `tasks`, `file_cache`, and `note_links`
4. All within a single SQLite transaction

The v04 migration (which drops and recreates `vec_chunks` to switch to `FLOAT` type) is wrapped in a transaction to ensure crash consistency — if the process is killed mid-migration, neither the DROP nor the CREATE will persist.

## Testing Strategy

- Model-optional tests: Try `JapaneseTokenizer::new()`, skip if `Err`
- CI: Set `SHIOTSUCHI_MODEL_PATH` for full tests
- In-memory DB for unit tests
- `tempfile` for disk-based DB tests
- 554+ tests across core (388), CLI (142), MCP (33)
