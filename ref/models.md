# Data Models

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

## `SearchResult`

```rust
pub struct SearchResult {
    pub path: String,        // Relative path
    pub title: String,       // Note title
    pub snippet: String,     // Extracted text around match
    pub score: f64,          // BM25 rank (lower = better)
}
```

## `VaultStats`

```rust
pub struct VaultStats {
    pub total_notes: usize,
    pub total_size_bytes: usize,
    pub last_indexed_at: Option<i64>,
    pub db_path: PathBuf,
}
```

## `IndexResult`

Enum representing the outcome of indexing a single file:
- `Inserted` — New file
- `Updated` — Content changed (hash mismatch)
- `Skipped` — Unchanged
- `Error(String)` — Read/tokenize/DB error

## `IndexConfig`

```rust
pub struct IndexConfig {
    pub notes_dir: PathBuf,
    pub include_extensions: Vec<String>,  // ["md", "markdown"]
    pub exclude_patterns: Vec<String>,     // [".git", ".obsidian", "node_modules"]
}
```

## FTS5 Query Format

Tokenized queries are wrapped in quotes and joined with AND:
```
Input: "東京 検索 エンジン"
Output: "東京" AND "検索" AND "エンジン"
```

Quotes inside tokens are escaped as `""`.

## File Hash

SHA-256 of raw file content (before frontmatter extraction or markdown parsing). Used for change detection to skip re-indexing unchanged files.

## Relative Paths

All paths stored in the database use the notes directory as root:
- Forward slashes (`/`) regardless of platform
- No leading `./`
- Examples: `projects/meeting.md`, `daily/2024-04-29.md`
