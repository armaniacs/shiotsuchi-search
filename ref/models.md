# Data Models

## `Chunk`

```rust
pub struct Chunk {
    pub id: Option<i64>,                // Set by DB after INSERT; None before persisting
    pub file_path: String,              // Relative path (forward slashes)
    pub chunk_index: i64,               // 0-based position within the file
    pub parent_header: Option<String>,  // Ancestor heading path, e.g. "大見出し > 中見出し"
    pub content: String,                // Raw Markdown content (for snippets/display)
    pub tokenized_content: String,      // Vaporetto-tokenized, space-separated (for FTS5)
}
```

## `ChunkSearchResult`

```rust
pub struct ChunkSearchResult {
    pub chunk_id: i64,
    pub file_path: String,
    pub parent_header: Option<String>,
    pub content: String,
    pub score: f64,              // Lower = more relevant for FTS; higher = more relevant for Hybrid
    pub search_mode: SearchMode,
}
```

## `SearchMode`

```rust
pub enum SearchMode {
    Fts,       // Keyword BM25 (no model needed)
    Vec,       // Semantic vector KNN (requires ONNX model)
    Hybrid,    // RRF fusion of FTS + Vec (default)
}
```

## `EmbedderStatus`

```rust
pub enum EmbedderStatus {
    Ready,                           // Model loaded and ready
    Unavailable(String),             // Model file not found — FTS-only mode
}
```

## `NoteMetadata`

```rust
pub struct NoteMetadata {
    pub path: String,        // Relative path (forward slashes)
    pub hash: String,        // SHA-256 hex string
    pub mtime: i64,          // Unix timestamp (seconds)
    pub indexed_at: i64,     // Unix timestamp (seconds)
    pub title: String,       // From frontmatter or filename
}
```

## `VaultStats`

```rust
pub struct VaultStats {
    pub total_chunks: usize,
    pub total_files: usize,
    pub total_size_bytes: usize,
    pub last_indexed_at: Option<i64>,
    pub db_path: PathBuf,
    pub vec_indexed_chunks: usize,
    pub embedder_status: String,
}
```

## `SearchConfig`

```rust
pub struct SearchConfig {
    pub max_snippet_chars: usize,  // Clamped 128–65535, default 1000
}
```

## `IndexConfig`

```rust
pub struct IndexConfig {
    pub notes_dir: PathBuf,
    pub include_extensions: Vec<String>,  // ["md", "markdown"]
    pub exclude_dirs: Vec<String>,         // ["node_modules"]
    pub auto_exclude_hidden: bool,         // true
    pub follow_links: bool,                // false
    pub dynamic_threshold: usize,          // 5
}
```

## `IndexResult`

Enum representing the outcome of indexing a single file:
- `Inserted` — New file
- `Updated` — Content changed (hash mismatch)
- `Skipped` — Unchanged
- `Error(String)` — Read/tokenize/DB error

## `BuildInfo` (compile-time constants in `build_info.rs`)

| Constant | Type | Description |
|----------|------|-------------|
| `HAS_MODEL_EMBEDDED` | `bool` | Vaporetto model embedded at build time |
| `EMBEDDED_MODEL_HASH` | `&str` | SHA-256 hex of embedded predictor |
| `FEATURE_WATCHER` | `bool` | `watcher` feature enabled |
| `FEATURE_ASYNC_INDEX` | `bool` | `async-index` feature enabled |
| `DEP_RUSQLITE_BUNDLED` | `bool` | rusqlite uses bundled SQLite |

## `IndexProgress` (callback type)

```rust
pub type IndexProgress = Box<dyn Fn(usize, usize) + Send + 'static>;
```

Used by `index_directory()` for per-file progress reporting. Arguments are (current, total) where current is 1-based.

## FTS5 Query Format

Tokenized queries are wrapped in quotes and joined with AND:
```
Input: "東京 検索 エンジン"
Output: "東京" AND "検索" AND "エンジン"
```

Quotes inside tokens are escaped as `""`. For models without a tokenizer, `simple_and_query()` provides a whitespace-based fallback.

## File Hash

SHA-256 of raw file content (before frontmatter extraction or markdown parsing). Used for change detection to skip re-indexing unchanged files.

## Relative Paths

All paths stored in the database use the notes directory as root:
- Forward slashes (`/`) regardless of platform
- No leading `./`
- Examples: `projects/meeting.md`, `daily/2024-04-29.md`
