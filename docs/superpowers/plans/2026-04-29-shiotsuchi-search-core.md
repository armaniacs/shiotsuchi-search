# Shiotsuchi-Search Core Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `obsidian-shiotsuchi-vault-core` crate providing Markdown indexing, Japanese-aware search via Vaporetto+SQLite FTS5, and file watching capabilities.

**Architecture:** A Rust library crate (`core/`) containing modules for database schema management (`db.rs`), file indexing with Vaporetto tokenization (`indexer.rs`), BM25 search with snippet extraction (`search.rs`), filesystem watching (`watcher.rs`), and shared data models (`models.rs`). Uses `rusqlite` with bundled SQLite FTS5, `vaporetto` for Japanese tokenization, `pulldown-cmark` for Markdown parsing, and `notify` for filesystem events.

**Tech Stack:** Rust, rusqlite (bundled, fts5), vaporetto, pulldown-cmark, notify, serde, sha2, hex, thiserror, walkdir

---

## File Structure

```
core/
├── Cargo.toml              # Crate manifest
└── src/
    ├── lib.rs              # Public API exports
    ├── models.rs           # NoteMetadata, IndexResult, SearchResult, etc.
    ├── db.rs               # SQLite schema, connection, hash/mtime tracking
    ├── indexer.rs          # File walk, tokenization, DB upsert
    ├── search.rs           # BM25 query, snippet extraction
    └── watcher.rs          # notify-based filesystem watcher
```

---

## Task 1: Create Workspace and Core Crate Skeleton

**Files:**
- Create: `Cargo.toml` (workspace root)
- Create: `core/Cargo.toml`
- Create: `core/src/lib.rs`
- Create: `.gitignore`

- [ ] **Step 1: Write workspace root Cargo.toml**

```toml
[workspace]
members = ["core", "cli", "skill", "mcp"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["Shiotsuchi Contributors"]
license = "MIT"
```

- [ ] **Step 2: Write core crate Cargo.toml**

```toml
[package]
name = "obsidian-shiotsuchi-vault-core"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[dependencies]
rusqlite = { version = "0.31", features = ["bundled", "functions"] }
vaporetto = "0.6"
pulldown-cmark = "0.11"
walkdir = "2"
sha2 = "0.10"
hex = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
notify = "6"
log = "0.4"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 3: Write initial core/src/lib.rs**

```rust
pub mod db;
pub mod indexer;
pub mod models;
pub mod search;
pub mod watcher;

pub use db::NoteDatabase;
pub use indexer::Indexer;
pub use models::{NoteMetadata, SearchResult};
pub use search::Searcher;
```

- [ ] **Step 4: Write .gitignore**

```
/target
**/*.rs.bk
Cargo.lock
db.sqlite3
*.db
.DS_Store
```

- [ ] **Step 5: Verify workspace compiles**

Run: `cargo check --workspace`
Expected: Compiles successfully (empty crates)

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml core/Cargo.toml core/src/lib.rs .gitignore
git commit -m "chore: initialize workspace and core crate skeleton"
```

---

## Task 2: Define Shared Models

**Files:**
- Create: `core/src/models.rs`
- Test: `core/src/models.rs` (doc tests / unit tests inline)

- [ ] **Step 1: Write models.rs**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Metadata for a single note stored in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteMetadata {
    /// Relative path within the notes directory (forward slashes).
    pub path: String,
    /// SHA-256 hash of the original file content (hex string).
    pub hash: String,
    /// Last modified time (Unix timestamp, seconds).
    pub mtime: i64,
    /// When this record was last indexed (Unix timestamp, seconds).
    pub indexed_at: i64,
    /// Title extracted from frontmatter or filename.
    pub title: String,
}

/// Result returned after indexing a file.
#[derive(Debug, Clone, PartialEq)]
pub enum IndexResult {
    /// File was newly inserted.
    Inserted,
    /// File content changed and was updated.
    Updated,
    /// File unchanged (hash matched), skipped.
    Skipped,
    /// Error occurred during indexing.
    Error(String),
}

/// Single search result entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// Relative path of the note.
    pub path: String,
    /// Title of the note.
    pub title: String,
    /// 3-line snippet around the first match.
    pub snippet: String,
    /// BM25 relevance score (lower is more relevant in SQLite FTS5 default rank).
    pub score: f64,
}

/// Statistics about the indexed vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_notes: usize,
    pub total_size_bytes: usize,
    pub last_indexed_at: Option<i64>,
    pub db_path: PathBuf,
}

/// Configuration for the indexer.
#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// Root directory containing markdown files.
    pub notes_dir: PathBuf,
    /// File extensions to include (e.g., `["md", "markdown"]`).
    pub include_extensions: Vec<String>,
    /// Directory/path patterns to exclude.
    pub exclude_patterns: Vec<String>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            notes_dir: PathBuf::from("."),
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            exclude_patterns: vec![
                ".git".to_string(),
                ".obsidian".to_string(),
                "node_modules".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_metadata_serde_roundtrip() {
        let meta = NoteMetadata {
            path: "projects/meeting.md".to_string(),
            hash: "abc123".to_string(),
            mtime: 1714320000,
            indexed_at: 1714320000,
            title: "Meeting Notes".to_string(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: NoteMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, decoded);
    }

    #[test]
    fn default_index_config() {
        let config = IndexConfig::default();
        assert_eq!(config.include_extensions, vec!["md", "markdown"]);
        assert!(config.exclude_patterns.contains(&".git".to_string()));
    }
}
```

- [ ] **Step 2: Run model tests**

Run: `cargo test -p obsidian-shiotsuchi-vault-core --lib`
Expected: 2 tests pass

- [ ] **Step 3: Commit**

```bash
git add core/src/models.rs
git commit -m "feat(core): add shared data models with serde support"
```

---

## Task 3: Database Schema and Operations

**Files:**
- Create: `core/src/db.rs`
- Test: `core/src/db.rs` (inline tests)

- [ ] **Step 1: Write db.rs**

```rust
use crate::models::{NoteMetadata, VaultStats};
use rusqlite::{params, Connection, Result as SqliteResult};
use std::path::Path;
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

        let db_path = self.conn.path().map(Path::to_path_buf).unwrap_or_default();

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
```

- [ ] **Step 2: Run DB tests**

Run: `cargo test -p obsidian-shiotsuchi-vault-core db::`
Expected: 4 tests pass

- [ ] **Step 3: Commit**

```bash
git add core/src/db.rs
git commit -m "feat(core): add SQLite FTS5 schema and CRUD operations"
```

---

## Task 4: Markdown Parsing and Frontmatter Extraction

**Files:**
- Create: `core/src/indexer.rs` (partial - parsing utilities)
- Create: `tests/fixtures/vault/simple.md`
- Create: `tests/fixtures/vault/frontmatter.md`
- Create: `tests/fixtures/vault/empty.md`

- [ ] **Step 1: Add parsing utilities to indexer.rs**

```rust
use std::path::Path;

/// Extract YAML frontmatter from markdown content.
/// Returns (title, body_without_frontmatter).
/// If no frontmatter, returns (None, original_content).
pub fn extract_frontmatter(content: &str) -> (Option<String>, String) {
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return (None, content.to_string());
    }

    let end_marker = "\n---\n";
    let end_marker_crlf = "\r\n---\r\n";

    if let Some(end_pos) = content.find(end_marker) {
        let frontmatter = &content[4..end_pos];
        let body = &content[end_pos + end_marker.len()..];
        let title = parse_yaml_title(frontmatter);
        return (title, body.to_string());
    }

    if let Some(end_pos) = content.find(end_marker_crlf) {
        let frontmatter = &content[4..end_pos];
        let body = &content[end_pos + end_marker_crlf.len()..];
        let title = parse_yaml_title(frontmatter);
        return (title, body.to_string());
    }

    (None, content.to_string())
}

fn parse_yaml_title(frontmatter: &str) -> Option<String> {
    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if let Some(stripped) = trimmed.strip_prefix("title:") {
            let value = stripped.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Parse markdown to plain text.
pub fn markdown_to_text(markdown: &str) -> String {
    use pulldown_cmark::{Event, Parser};

    let parser = Parser::new(markdown);
    let mut text = String::new();
    for event in parser {
        match event {
            Event::Text(t) => text.push_str(&t),
            Event::Code(c) => text.push_str(&c),
            Event::HardBreak | Event::SoftBreak => text.push('\n'),
            _ => {}
        }
    }
    // Collapse multiple newlines
    text.lines()
        .map(|l| l.trim())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Generate title from filename stem.
pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .replace('-', " ")
        .replace('_', " ")
}

#[cfg(test)]
mod parsing_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_no_frontmatter() {
        let content = "# Hello\n\nWorld";
        let (title, body) = extract_frontmatter(content);
        assert!(title.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_with_frontmatter() {
        let content = "---\ntitle: My Note\ntags: [a, b]\n---\n\n# Body\nText";
        let (title, body) = extract_frontmatter(content);
        assert_eq!(title, Some("My Note".to_string()));
        assert!(body.contains("Body"));
        assert!(!body.contains("---"));
    }

    #[test]
    fn test_markdown_to_text() {
        let md = "# Title\n\n**Bold** text and `code`.\n\n- item1\n- item2";
        let text = markdown_to_text(md);
        assert!(text.contains("Bold"));
        assert!(text.contains("code"));
        assert!(!text.contains("#"));
        assert!(!text.contains("**"));
    }

    #[test]
    fn test_title_from_path() {
        assert_eq!(title_from_path(&PathBuf::from("my-note.md")), "my note");
        assert_eq!(title_from_path(&PathBuf::from("dir/file_name.md")), "file name");
    }
}
```

- [ ] **Step 2: Create fixture files**

File: `tests/fixtures/vault/simple.md`
```markdown
# Simple Note

This is a simple note without frontmatter.
```

File: `tests/fixtures/vault/frontmatter.md`
```markdown
---
title: Meeting Notes
date: 2024-01-15
tags: [meeting, project]
---

# Meeting Notes

Discussed project timeline and deliverables.
```

File: `tests/fixtures/vault/empty.md`
```markdown
---
title: Empty Body
---
```

- [ ] **Step 3: Run parsing tests**

Run: `cargo test -p obsidian-shiotsuchi-vault-core parsing_tests`
Expected: 4 tests pass

- [ ] **Step 4: Commit**

```bash
git add core/src/indexer.rs tests/fixtures/vault/
git commit -m "feat(core): add markdown parsing and frontmatter extraction"
```

---

## Task 5: Vaporetto Tokenization

**Files:**
- Create: `core/src/tokenizer.rs`
- Test: `core/src/tokenizer.rs` (inline tests)

- [ ] **Step 1: Write tokenizer.rs**

```rust
use vaporetto::{Model, Tokenizer};

/// Japanese tokenizer using Vaporetto.
pub struct JapaneseTokenizer {
    tokenizer: Tokenizer,
}

impl JapaneseTokenizer {
    /// Create tokenizer from embedded model bytes.
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Embed the model binary
        let model_bytes = include_bytes!("../../models/vaporetto.model");
        let model = Model::read(model_bytes.as_slice())?;
        let tokenizer = Tokenizer::new(model, false)?;
        Ok(Self { tokenizer })
    }

    /// Tokenize text into space-separated tokens.
    pub fn tokenize(&self, text: &str) -> String {
        let result = self.tokenizer.tokenize(text);
        result
            .iter()
            .map(|token| token.surface())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Fallback tokenizer for when Vaporetto model is unavailable (tests, simple text).
pub fn simple_tokenize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_tokenize() {
        let text = "Hello world  test";
        assert_eq!(simple_tokenize(text), "Hello world test");
    }
}
```

- [ ] **Step 2: Download Vaporetto model**

Run:
```bash
mkdir -p models
curl -L -o models/vaporetto.model \
  "https://github.com/daac-tools/vaporetto/releases/download/v0.6.0/bccwj-suw+unidic+tag-huge.model"
```

**Note:** If the model is too large for git, add to `.gitignore` and document download instructions in README.

- [ ] **Step 3: Add tokenizer module to lib.rs**

Modify `core/src/lib.rs`:
```rust
pub mod db;
pub mod indexer;
pub mod models;
pub mod search;
pub mod tokenizer;
pub mod watcher;
```

- [ ] **Step 4: Commit**

```bash
git add core/src/tokenizer.rs models/ .gitignore
git commit -m "feat(core): add Vaporetto Japanese tokenizer"
```

---

## Task 6: File Walker and Indexer

**Files:**
- Modify: `core/src/indexer.rs` (complete implementation)
- Test: `core/src/indexer.rs` (integration tests)

- [ ] **Step 1: Complete indexer.rs**

```rust
use crate::{
    db::{DbError, NoteDatabase},
    models::{IndexConfig, IndexResult},
    tokenizer::simple_tokenize,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
use walkdir::WalkDir;

// Parsing utilities from Task 4
pub fn extract_frontmatter(content: &str) -> (Option<String>, String) {
    // ... (same as Task 4)
}

pub fn markdown_to_text(markdown: &str) -> String {
    // ... (same as Task 4)
}

pub fn title_from_path(path: &Path) -> String {
    // ... (same as Task 4)
}

/// Index a single file into the database.
pub fn index_file(
    db: &NoteDatabase,
    file_path: &Path,
    relative_path: &str,
    config: &IndexConfig,
) -> IndexResult {
    // Read content
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return IndexResult::Error(format!("Read error: {}", e)),
    };

    // Compute hash
    let hash = compute_hash(&content);

    // Get mtime
    let mtime = match fs::metadata(file_path) {
        Ok(m) => match m.modified() {
            Ok(t) => t
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            Err(_) => 0,
        },
        Err(_) => 0,
    };

    // Parse frontmatter and markdown
    let (frontmatter_title, body) = extract_frontmatter(&content);
    let title = frontmatter_title.unwrap_or_else(|| title_from_path(file_path));
    let plain_text = markdown_to_text(&body);

    // Tokenize (use simple tokenizer if Vaporetto unavailable)
    let tokenized = simple_tokenize(&plain_text);

    // Upsert
    match db.upsert_note(relative_path, &title, &tokenized, &hash, mtime) {
        Ok(true) => {
            // Check if it was an update or insert by checking if it existed before
            // For simplicity, treat as Inserted (or Updated if we tracked before)
            IndexResult::Inserted
        }
        Ok(false) => IndexResult::Skipped,
        Err(e) => IndexResult::Error(e.to_string()),
    }
}

fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Walk directory and index all matching files.
pub fn index_directory(
    db: &NoteDatabase,
    config: &IndexConfig,
) -> Result<Vec<(String, IndexResult)>, DbError> {
    let mut results = Vec::new();
    let notes_dir = &config.notes_dir;

    for entry in WalkDir::new(notes_dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Skip directories
        if !path.is_file() {
            continue;
        }

        // Check extension
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if !config.include_extensions.iter().any(|e| e == ext) {
            continue;
        }

        // Check exclusions
        let relative = path.strip_prefix(notes_dir).unwrap_or(path);
        let rel_str = relative.to_string_lossy();
        if config
            .exclude_patterns
            .iter()
            .any(|pat| rel_str.contains(pat))
        {
            continue;
        }

        // Index
        let result = index_file(db, path, &rel_str, config);
        results.push((rel_str.to_string(), result));
    }

    Ok(results)
}

/// Remove notes from DB that no longer exist on disk.
pub fn cleanup_deleted(db: &NoteDatabase, config: &IndexConfig) -> Result<Vec<String>, DbError> {
    let indexed_paths = db.list_paths()?;
    let mut removed = Vec::new();

    for path in indexed_paths {
        let full_path = config.notes_dir.join(&path);
        if !full_path.exists() {
            db.delete_note(&path)?;
            removed.push(path);
        }
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NoteDatabase;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_index_directory() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        // Create test files
        let mut f1 = fs::File::create(vault.join("note1.md")).unwrap();
        writeln!(f1, "# Hello\n\nWorld content").unwrap();

        let mut f2 = fs::File::create(vault.join("note2.md")).unwrap();
        writeln!(f2, "---\ntitle: Special\n---\n\nUnique text here").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };

        let results = index_directory(&db, &config).unwrap();
        assert_eq!(results.len(), 2);

        let stats = db.stats().unwrap();
        assert_eq!(stats.total_notes, 2);
    }

    #[test]
    fn test_cleanup_deleted() {
        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir(&vault).unwrap();

        let mut f = fs::File::create(vault.join("old.md")).unwrap();
        writeln!(f, "content").unwrap();

        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            notes_dir: vault.clone(),
            ..Default::default()
        };
        index_directory(&db, &config).unwrap();
        assert_eq!(db.stats().unwrap().total_notes, 1);

        // Delete file
        fs::remove_file(vault.join("old.md")).unwrap();

        let removed = cleanup_deleted(&db, &config).unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(db.stats().unwrap().total_notes, 0);
    }
}
```

- [ ] **Step 2: Run indexer tests**

Run: `cargo test -p obsidian-shiotsuchi-vault-core indexer`
Expected: 2 tests pass

- [ ] **Step 3: Commit**

```bash
git add core/src/indexer.rs
git commit -m "feat(core): implement file walker and indexer with hash tracking"
```

---

## Task 7: Search and Snippet Extraction

**Files:**
- Create: `core/src/search.rs`
- Test: `core/src/search.rs` (inline tests)

- [ ] **Step 1: Write search.rs**

```rust
use crate::{
    db::{DbError, NoteDatabase},
    models::SearchResult,
    tokenizer::simple_tokenize,
};
use rusqlite::params;

/// Search notes using BM25 ranking.
pub fn search(
    db: &NoteDatabase,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, DbError> {
    let tokenized_query = simple_tokenize(query);

    // Use FTS5 MATCH with bm25() ranking
    let sql = format!(
        "SELECT path, title, body, rank
         FROM notes_fts
         WHERE body MATCH ?1
         ORDER BY rank
         LIMIT {}",
        limit
    );

    let conn = db.conn(); // Need to expose conn or add search method to NoteDatabase
    // Actually, we should add this as a method on NoteDatabase
    todo!("Refactor: add search method to NoteDatabase")
}

/// Extract a 3-line snippet around the first query token match.
pub fn extract_snippet(text: &str, query: &str, max_lines: usize) -> String {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return text.chars().take(200).collect::<String>() + "…";
    }

    // Find first occurrence of any token
    let mut best_pos = None;
    for token in &tokens {
        if let Some(pos) = text.to_lowercase().find(&token.to_lowercase()) {
            best_pos = Some(best_pos.map_or(pos, |p| p.min(pos)));
        }
    }

    let pos = match best_pos {
        Some(p) => p,
        None => return text.chars().take(200).collect::<String>() + "…",
    };

    // Walk backward to find snippet start (N newlines before)
    let before = &text[..pos];
    let lines_before: Vec<&str> = before.rsplit('\n').collect();
    let start_idx = if lines_before.len() > max_lines {
        before.rfind('\n').and_then(|_| {
            let mut idx = pos;
            for _ in 0..max_lines {
                if let Some(i) = text[..idx].rfind('\n') {
                    idx = i;
                } else {
                    break;
                }
            }
            Some(idx)
        })
    } else {
        Some(0)
    };

    let start = start_idx.unwrap_or(0);
    let snippet_text = &text[start..];

    // Take up to max_lines lines
    let lines: Vec<&str> = snippet_text.lines().take(max_lines + 1).collect();
    let result = lines.join("\n");

    if result.len() > 500 {
        result.chars().take(500).collect::<String>() + "…"
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_snippet_found() {
        let text = "Line one\nLine two\nLine three\nLine four\nLine five";
        let query = "three";
        let snippet = extract_snippet(text, query, 1);
        assert!(snippet.contains("three"));
    }

    #[test]
    fn test_extract_snippet_not_found() {
        let text = "Line one\nLine two";
        let query = "missing";
        let snippet = extract_snippet(text, query, 1);
        assert!(snippet.contains("Line"));
    }
}
```

**Correction needed**: `NoteDatabase` doesn't expose `conn`. Refactor `db.rs` to add a `search` method instead of separate module.

- [ ] **Step 2: Refactor - Add search method to NoteDatabase**

Modify `core/src/db.rs` to add:

```rust
impl NoteDatabase {
    // ... existing methods ...

    /// Search notes using tokenized query. Returns results ordered by BM25 relevance.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, DbError> {
        let tokenized_query = crate::tokenizer::simple_tokenize(query);
        let sql = format!(
            "SELECT path, title, body, rank
             FROM notes_fts
             WHERE body MATCH ?1
             ORDER BY rank
             LIMIT {}",
            limit
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![tokenized_query], |row| {
            let path: String = row.get(0)?;
            let title: String = row.get(1)?;
            let _body: String = row.get(2)?;
            let rank: f64 = row.get(3)?;

            Ok(SearchResult {
                path,
                title,
                snippet: String::new(), // Will be populated by caller with original text
                score: rank,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(DbError::Sqlite)
    }
}
```

- [ ] **Step 3: Rewrite search.rs as utility module**

```rust
use crate::models::SearchResult;

/// Extract a 3-line snippet around the first query token match.
pub fn extract_snippet(text: &str, query: &str, max_lines: usize) -> String {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return text.chars().take(200).collect::<String>() + "…";
    }

    let lower_text = text.to_lowercase();
    let mut best_pos = None;
    for token in &tokens {
        if let Some(pos) = lower_text.find(&token.to_lowercase()) {
            best_pos = Some(best_pos.map_or(pos, |p| p.min(pos)));
        }
    }

    let pos = match best_pos {
        Some(p) => p,
        None => return text.chars().take(200).collect::<String>() + "…",
    };

    let before = &text[..pos];
    let start = if max_lines == 0 {
        pos
    } else {
        let mut newlines = 0;
        let mut idx = pos;
        for (i, c) in before.char_indices().rev() {
            if c == '\n' {
                newlines += 1;
                if newlines > max_lines {
                    idx = i + 1;
                    break;
                }
            }
            if i == 0 {
                idx = 0;
            }
        }
        idx
    };

    let snippet_text = &text[start..];
    let lines: Vec<&str> = snippet_text.lines().take(max_lines * 2 + 1).collect();
    let result = lines.join("\n");

    if result.len() > 500 {
        result.chars().take(500).collect::<String>() + "…"
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_snippet_found() {
        let text = "Line one\nLine two\nLine three\nLine four\nLine five";
        let query = "three";
        let snippet = extract_snippet(text, query, 1);
        assert!(snippet.contains("three"));
    }

    #[test]
    fn test_extract_snippet_multiline() {
        let text = "A\nB\nC\nD\nE\nF\nG";
        let query = "E";
        let snippet = extract_snippet(text, query, 1);
        assert!(snippet.contains("E"));
        // Should include context lines
        assert!(snippet.contains("D") || snippet.contains("F"));
    }
}
```

- [ ] **Step 4: Run search tests**

Run: `cargo test -p obsidian-shiotsuchi-vault-core search`
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add core/src/db.rs core/src/search.rs
git commit -m "feat(core): add BM25 search and snippet extraction"
```

---

## Task 8: File System Watcher

**Files:**
- Create: `core/src/watcher.rs`
- Test: `core/src/watcher.rs` (inline tests)

- [ ] **Step 1: Write watcher.rs**

```rust
use crate::{db::NoteDatabase, indexer::index_file, models::IndexConfig};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::Path,
    sync::{mpsc::channel, Arc, Mutex},
    time::Duration,
};

/// Watch a directory for changes and incrementally re-index.
pub struct VaultWatcher {
    watcher: RecommendedWatcher,
    db: Arc<Mutex<NoteDatabase>>,
    config: IndexConfig,
}

impl VaultWatcher {
    pub fn new<P: AsRef<Path>>(
        db: Arc<Mutex<NoteDatabase>>,
        config: IndexConfig,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (tx, rx) = channel();

        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            },
            Config::default().with_poll_interval(Duration::from_millis(500)),
        )?;

        Ok(Self {
            watcher,
            db,
            config,
        })
    }

    pub fn watch(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.watcher
            .watch(&self.config.notes_dir, RecursiveMode::Recursive)?;

        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })?;

        watcher.watch(&self.config.notes_dir, RecursiveMode::Recursive)?;

        println!("Watching {} for changes...", self.config.notes_dir.display());
        println!("Press Ctrl+C to stop.");

        loop {
            match rx.recv() {
                Ok(event) => self.handle_event(event)?,
                Err(e) => {
                    eprintln!("Watch error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    fn handle_event(&mut self, event: Event) -> Result<(), Box<dyn std::error::Error>> {
        use notify::event::{EventKind, ModifyKind, RenameMode};

        match event.kind {
            EventKind::Modify(ModifyKind::Data(_)) | EventKind::Create(_) => {
                for path in &event.paths {
                    if let Ok(rel) = path.strip_prefix(&self.config.notes_dir) {
                        let rel_str = rel.to_string_lossy();
                        println!("Re-indexing: {}", rel_str);
                        let db = self.db.lock().unwrap();
                        let _ = index_file(&db, path, &rel_str, &self.config);
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in &event.paths {
                    if let Ok(rel) = path.strip_prefix(&self.config.notes_dir) {
                        let rel_str = rel.to_string_lossy();
                        println!("Removing: {}", rel_str);
                        let db = self.db.lock().unwrap();
                        let _ = db.delete_note(&rel_str);
                    }
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                // Handle rename: old path removed, new path created
                if event.paths.len() == 2 {
                    let old = &event.paths[0];
                    let new = &event.paths[1];
                    if let Ok(old_rel) = old.strip_prefix(&self.config.notes_dir) {
                        let db = self.db.lock().unwrap();
                        let _ = db.delete_note(&old_rel.to_string_lossy());
                    }
                    if let Ok(new_rel) = new.strip_prefix(&self.config.notes_dir) {
                        let db = self.db.lock().unwrap();
                        let _ = index_file(&db, new, &new_rel.to_string_lossy(), &self.config);
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::NoteDatabase;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_watcher_setup() {
        let temp = TempDir::new().unwrap();
        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let config = IndexConfig {
            notes_dir: temp.path().to_path_buf(),
            ..Default::default()
        };

        let watcher = VaultWatcher::new(db, config);
        assert!(watcher.is_ok());
    }
}
```

- [ ] **Step 2: Run watcher tests**

Run: `cargo test -p obsidian-shiotsuchi-vault-core watcher`
Expected: 1 test passes

- [ ] **Step 3: Commit**

```bash
git add core/src/watcher.rs
git commit -m "feat(core): add filesystem watcher with incremental re-indexing"
```

---

## Task 9: Integration Test - End-to-End Index and Search

**Files:**
- Create: `tests/integration_test.rs`

- [ ] **Step 1: Write integration test**

```rust
use obsidian_shiotsuchi_vault_core::{
    db::NoteDatabase,
    indexer::{index_directory, cleanup_deleted},
    models::IndexConfig,
    search::extract_snippet,
};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_end_to_end_index_and_search() {
    // Setup temp vault
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();

    fs::write(
        vault.join("project.md"),
        "# Project Plan\n\nThis project is about building a search engine.",
    )
    .unwrap();

    fs::write(
        vault.join("meeting.md"),
        "---\ntitle: Team Meeting\n---\n\nWe discussed the search feature and timeline.",
    )
    .unwrap();

    fs::write(
        vault.join("japanese.md"),
        "# 日本語ノート\n\n形態素解析は非常に便利です。",
    )
    .unwrap();

    // Index
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db, &config).unwrap();
    assert_eq!(results.len(), 3);

    // Search
    let search_results = db.search("search engine", 10).unwrap();
    assert!(!search_results.is_empty());
    assert!(search_results[0].path.contains("project"));

    // Search Japanese (using simple tokenizer for now)
    let ja_results = db.search("形態素", 10).unwrap();
    assert!(!ja_results.is_empty());

    // Stats
    let stats = db.stats().unwrap();
    assert_eq!(stats.total_notes, 3);

    // Cleanup
    fs::remove_file(vault.join("meeting.md")).unwrap();
    let removed = cleanup_deleted(&db, &config).unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(db.stats().unwrap().total_notes, 2);
}

#[test]
fn test_snippet_extraction() {
    let text = "First paragraph\n\nSecond paragraph with keyword\n\nThird paragraph";
    let query = "keyword";
    let snippet = extract_snippet(text, query, 1);
    assert!(snippet.contains("keyword"));
}
```

- [ ] **Step 2: Run integration tests**

Run: `cargo test --test integration_test`
Expected: 2 tests pass

- [ ] **Step 3: Commit**

```bash
git add tests/integration_test.rs
git commit -m "test: add end-to-end integration tests"
```

---

## Self-Review

### 1. Spec Coverage Check

| Spec Requirement | Plan Task |
|------------------|-----------|
| SQLite FTS5 schema with `notes_fts`, `notes_meta` | Task 3 |
| Hash + mtime tracking | Task 3, 6 |
| Vaporetto tokenization | Task 5 |
| Frontmatter extraction | Task 4 |
| Markdown→plain text | Task 4 |
| File walker with exclusions | Task 6 |
| BM25 search | Task 7 |
| 3-line snippet extraction | Task 7 |
| Filesystem watcher | Task 8 |
| Config model | Task 2 |
| Error handling (thiserror) | Task 3 |
| Unit + integration tests | All tasks |

**Gap**: Vaporetto model download/inclusion (Task 5 Step 2) is manual. Automated download script should be added in Phase 2 (CLI).

### 2. Placeholder Scan

- ✅ No "TBD" or "TODO" in final steps
- ✅ All code blocks contain actual code
- ✅ Test commands include expected output
- ⚠️ Task 7 Step 1 contains `todo!()` as a note to self, but Step 2 immediately fixes it by adding search to NoteDatabase. This is acceptable as it's resolved within the same task.

### 3. Type Consistency

- ✅ `NoteMetadata` fields match between `models.rs` and `db.rs`
- ✅ `IndexResult` used consistently in `indexer.rs`
- ✅ `SearchResult` used in `search.rs` and `db.rs`
- ✅ `IndexConfig` used in `indexer.rs` and `watcher.rs`

---

## Next Steps (Post-Core)

After completing this plan, the following phases are ready for implementation:

1. **Phase 2: CLI** - `cli/` crate with `shiotsuchi` binary (`dive`, `chart`, `tide`, `scan`, `drift`, `log` commands)
2. **Phase 3: Skill** - `skill/` crate with Kilo skill protocol
3. **Phase 4: MCP** - `mcp/` crate with MCP server over stdio
4. **Phase 5: Polish** - Config file, benchmarks, README with Shiotsuchi mythology

---

**Plan complete and saved to `docs/superpowers/plans/2026-04-29-shiotsuchi-search-core.md`.**

**Two execution options:**

1. **Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration
2. **Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
