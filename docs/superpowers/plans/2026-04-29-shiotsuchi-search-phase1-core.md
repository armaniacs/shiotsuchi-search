# Shiotsuchi-Search Phase 1: Core Library Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `obsidian-shiotsuchi-vault-core` crate providing Markdown indexing, Japanese-aware search via Vaporetto+SQLite FTS5, and file watching capabilities.

**Architecture:** A Rust library crate (`core/`) containing modules for database schema management (`db.rs`), file indexing with Vaporetto tokenization (`indexer.rs`), BM25 search with snippet extraction (`search.rs`), filesystem watching (`watcher.rs`), and shared data models (`models.rs`). Uses `rusqlite` with bundled SQLite FTS5, `vaporetto` for Japanese tokenization, `pulldown-cmark` for Markdown parsing, and `notify` for filesystem events.

**Tech Stack:** Rust, rusqlite (bundled, fts5), vaporetto, pulldown-cmark, notify, serde, sha2, hex, thiserror, walkdir

---

## TDD (Test-Driven Development) Approach

All implementation in this plan follows strict TDD cycles:

1. **RED** - Write a failing test for the desired behavior.
2. **RED VERIFY** - Run the test, confirm it fails (feature not yet implemented).
3. **GREEN** - Write minimal code to make the test pass.
4. **GREEN VERIFY** - Run the test, confirm it passes.
5. **REFACTOR** - Clean up code while keeping tests green.
6. Repeat for next behavior.

**Mandatory Rules:**
- Never write production code without a failing test first.
- If code was written before tests, delete it and start over.
- Verify RED before writing GREEN code — if the test passes immediately, the test is wrong.
- Verify GREEN before moving to next cycle.
- RED VERIFY is never skippable: watching the test fail is proof that it tests the right thing.

Each task below is structured as TDD cycles. Steps marked with (RED), (GREEN), (REFACTOR) indicate the phase.

**Exception — Task 1 (Skeleton):** Configuration files and empty module stubs have no testable behavior; TDD does not apply. All other tasks follow strict TDD.

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
rusqlite = { version = "0.31", features = ["bundled"] }
# NOTE: "load_extension" は不要。Rust 側でトークナイズするため。

# sqlite-vaporetto と同一の feature flags を指定する。
# charwise-pma は日本語の文字単位 PMA を有効化し、速度に最も影響する。
vaporetto = { version = "0.6", default-features = false, features = [
    "std",
    "tag-prediction",
    "cache-type-score",
    "fix-weight-length",
    "charwise-pma",
] }
vaporetto_rules = { version = "0.6", default-features = false }
ruzstd = "0.8"  # sqlite-vaporetto と同じ。.model.zst の zstd 解凍に使用。
pulldown-cmark = "0.11"
walkdir = "2"
sha2 = "0.10"
hex = "0.4"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
notify = { version = "6", optional = true }
log = "0.4"

[features]
default = ["watcher"]
watcher = ["dep:notify"]

[dev-dependencies]
tempfile = "3"

[build-dependencies]
# build.rs は std のみ使用。追加依存なし。
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
/models/
*.model.zst
*.model
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

## Task 2: Define Shared Models (TDD)

**Files:**
- Create: `core/src/models.rs`
- Test: `core/src/models.rs` (doc tests / unit tests inline)

- [ ] **(RED) Step 1: Write failing test for NoteMetadata**

Create `core/src/models.rs` with test only (NoteMetadata not defined yet):

```rust
use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_metadata_serde_roundtrip() {
        // This test will FAIL - NoteMetadata not defined yet
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
}
```

- [ ] **(RED VERIFY) Step 2: Run test, confirm it fails**

Run: `cargo test -p obsidian-shiotsuchi-vault-core --lib`
Expected: Compilation error (NoteMetadata not found)

- [ ] **(GREEN) Step 3: Write minimal NoteMetadata struct**

Add NoteMetadata with derive attributes and fields to make test compile:

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
```

- [ ] **(GREEN VERIFY) Step 4: Run test, confirm it passes**

Run: `cargo test -p obsidian-shiotsuchi-vault-core --lib`
Expected: test passes

- [ ] **(REFACTOR) Step 5: Add remaining types (IndexResult, SearchResult, VaultStats, IndexConfig)**

Add the rest of the types from the full spec below. Keep tests green.

Run: `cargo test -p obsidian-shiotsuchi-vault-core --lib`
Expected: 1 test still passes (no regressions)

- [ ] **(RED) Step 6: Write failing test for IndexConfig default**

Add test `default_index_config` to `models.rs` — `IndexConfig` not defined yet, so compilation fails:

```rust
#[test]
fn default_index_config() {
    let config = IndexConfig::default();
    assert_eq!(config.include_extensions, vec!["md", "markdown"]);
    assert!(config.exclude_patterns.contains(&".git".to_string()));
}
```

- [ ] **(RED VERIFY) Step 7: Run test, confirm it fails**

Run: `cargo test -p obsidian-shiotsuchi-vault-core --lib`
Expected: Compilation error — `IndexConfig` not found

- [ ] **(GREEN) Step 8: Implement IndexConfig with Default**

Add IndexConfig struct and Default impl.

- [ ] **(GREEN VERIFY) Step 9: Run all model tests**

Run: `cargo test -p obsidian-shiotsuchi-vault-core --lib`
Expected: 2 tests pass

- [ ] **Step 10: Commit**

```bash
git add core/src/models.rs
git commit -m "feat(core): add shared data models with serde support"
```

### Full models.rs reference (implement in GREEN/REFACTOR phases):

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

---

## Task 3: Database Schema and Operations

**TDD**: For each behavior: (RED) Write failing test → (RED VERIFY) → (GREEN) Minimal code → (GREEN VERIFY) → (REFACTOR)

**Files:**
- Create: `core/src/db.rs`
- Test: `core/src/db.rs` (inline tests)

- [ ] **(RED) Step 1: Write failing tests only in db.rs**

Create `core/src/db.rs` with the test module only — no implementation yet:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_schema() {
        // FAIL: NoteDatabase not defined yet
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

- [ ] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p obsidian-shiotsuchi-vault-core db::`
Expected: Compilation error — `NoteDatabase` not found

- [ ] **(GREEN) Step 3: Write minimal db.rs implementation**

Add the implementation above the test module to make all 4 tests pass:

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
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(
                path UNINDEXED,
                title,
                body,
                tokenize='unicode61 remove_diacritics 0'
            )",
            [],
        )?;

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
                return Ok(false);
            }
            self.conn
                .execute("DELETE FROM notes_fts WHERE path = ?1", [path])?;
        }

        self.conn.execute(
            "INSERT INTO notes_fts (path, title, body) VALUES (?1, ?2, ?3)",
            params![path, title, tokenized_body],
        )?;

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
```

- [ ] **(GREEN VERIFY) Step 4: Run DB tests, confirm all pass**

Run: `cargo test -p obsidian-shiotsuchi-vault-core db::`
Expected: 4 tests pass

- [ ] **Step 5: Commit**

```bash
git add core/src/db.rs
git commit -m "feat(core): add SQLite FTS5 schema and CRUD operations"
```

---

## Task 4: Markdown Parsing and Frontmatter Extraction

**TDD**: Follow RED → RED VERIFY → GREEN → GREEN VERIFY → REFACTOR for each behavior.

**Files:**
- Create: `core/src/indexer.rs` (partial - parsing utilities)
- Create: `tests/fixtures/vault/simple.md`
- Create: `tests/fixtures/vault/frontmatter.md`
- Create: `tests/fixtures/vault/empty.md`

- [ ] **(RED) Step 1: Write failing tests only in indexer.rs**

Create `core/src/indexer.rs` with the test module only — no implementation yet:

```rust
#[cfg(test)]
mod parsing_tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_no_frontmatter() {
        // FAIL: extract_frontmatter not defined yet
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

- [ ] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p obsidian-shiotsuchi-vault-core parsing_tests`
Expected: Compilation error — `extract_frontmatter`, `markdown_to_text`, `title_from_path` not found

- [ ] **(GREEN) Step 3: Write minimal parsing implementation**

Add the implementation above the test module:

```rust
use std::path::Path;

/// Extract YAML frontmatter. Returns (title, body_without_frontmatter).
pub fn extract_frontmatter(content: &str) -> (Option<String>, String) {
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return (None, content.to_string());
    }

    let end_marker = "\n---\n";
    let end_marker_crlf = "\r\n---\r\n";

    if let Some(end_pos) = content.find(end_marker) {
        let frontmatter = &content[4..end_pos];
        let body = &content[end_pos + end_marker.len()..];
        return (parse_yaml_title(frontmatter), body.to_string());
    }

    if let Some(end_pos) = content.find(end_marker_crlf) {
        let frontmatter = &content[4..end_pos];
        let body = &content[end_pos + end_marker_crlf.len()..];
        return (parse_yaml_title(frontmatter), body.to_string());
    }

    (None, content.to_string())
}

fn parse_yaml_title(frontmatter: &str) -> Option<String> {
    for line in frontmatter.lines() {
        if let Some(stripped) = line.trim().strip_prefix("title:") {
            let value = stripped.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Parse markdown to plain text (strips all markup).
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
    text.lines().map(|l| l.trim()).collect::<Vec<_>>().join("\n")
}

/// Derive title from filename stem (hyphens/underscores → spaces).
pub fn title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .replace('-', " ")
        .replace('_', " ")
}
```

- [ ] **(GREEN VERIFY) Step 4: Run parsing tests, confirm all pass**

Run: `cargo test -p obsidian-shiotsuchi-vault-core parsing_tests`
Expected: 4 tests pass

- [ ] **Step 5: Create fixture files**

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

- [ ] **Step 6: Commit**

```bash
git add core/src/indexer.rs tests/fixtures/vault/
git commit -m "feat(core): add markdown parsing and frontmatter extraction"
```

---

## Task 5: Vaporetto Tokenization (TDD)

**TDD**: Follow RED → RED VERIFY → GREEN → GREEN VERIFY → REFACTOR for each behavior.

**Files:**

**Files:**
- Create: `core/build.rs`
- Create: `core/src/tokenizer.rs`
- Create: `scripts/download-model.sh`
- Test: `core/src/tokenizer.rs` (inline tests)

- [ ] **Step 1: Write `core/build.rs`（sqlite-vaporetto の build.rs パターン踏襲）**

`SHIOTSUCHI_EMBED_MODEL` 環境変数が設定されていれば `include_bytes!` でモデルをバイナリに埋め込む。
未設定なら `EMBEDDED_MODEL_BYTES = None`（実行時に `SHIOTSUCHI_MODEL_PATH` 環境変数を参照）。

```rust
use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest = out_dir.join("embedded_model.rs");

    if let Ok(model_path) = env::var("SHIOTSUCHI_EMBED_MODEL") {
        if !model_path.is_empty() {
            println!("cargo:rerun-if-changed={}", model_path);
            fs::write(
                &dest,
                format!(
                    "static EMBEDDED_MODEL_BYTES: Option<&'static [u8]> = Some(include_bytes!({:?}));",
                    model_path
                ),
            )
            .unwrap();
            return;
        }
    }

    fs::write(
        &dest,
        "static EMBEDDED_MODEL_BYTES: Option<&'static [u8]> = None;",
    )
    .unwrap();
    println!("cargo:rerun-if-env-changed=SHIOTSUCHI_EMBED_MODEL");
}
```

- [ ] **(RED) Step 2: Write failing tests only in tokenizer.rs**

Create `core/src/tokenizer.rs` with the test module only — no implementation yet:

```rust
include!(concat!(env!("OUT_DIR"), "/embedded_model.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_tokenize() {
        // FAIL: simple_tokenize not defined yet
        assert_eq!(simple_tokenize("Hello world  test"), "Hello world test");
    }

    #[test]
    fn test_simple_and_query() {
        let q = simple_and_query("東京 検索");
        assert_eq!(q, "\"東京\" AND \"検索\"");
    }
}
```

- [ ] **(RED VERIFY) Step 3: Run tests, confirm they fail**

Run: `cargo test -p obsidian-shiotsuchi-vault-core tokenizer`
Expected: Compilation error — `simple_tokenize`, `simple_and_query` not found

- [ ] **(GREEN) Step 4: Write minimal tokenizer implementation**

Add the implementation above the test module. Start with just `simple_tokenize` and `simple_and_query` to pass the tests, then add `JapaneseTokenizer`:

```rust
include!(concat!(env!("OUT_DIR"), "/embedded_model.rs"));

use std::io::Read;
use thiserror::Error;
use vaporetto::{Model, Predictor, Sentence};

#[derive(Error, Debug)]
pub enum TokenizerError {
    #[error("モデルが見つかりません: SHIOTSUCHI_MODEL_PATH を設定するか、SHIOTSUCHI_EMBED_MODEL 付きで再ビルドしてください")]
    NoModel,
    #[error("モデルロード失敗: {0}")]
    ModelLoad(String),
}

/// sqlite-vaporetto の TokenizerConfig に対応。
#[derive(Debug, Clone)]
pub struct TokenizerConfig {
    pub pos_filter: Option<Vec<String>>,
    pub keep_untagged: bool,
}

impl Default for TokenizerConfig {
    fn default() -> Self {
        Self { pos_filter: None, keep_untagged: true }
    }
}

pub struct JapaneseTokenizer {
    predictor: Predictor,
    config: TokenizerConfig,
}

impl JapaneseTokenizer {
    pub fn new(config: TokenizerConfig) -> Result<Self, TokenizerError> {
        let bytes_owned: Vec<u8>;
        let model_bytes: &[u8] = if let Some(embedded) = EMBEDDED_MODEL_BYTES {
            embedded
        } else if let Ok(path) = std::env::var("SHIOTSUCHI_MODEL_PATH") {
            let raw = std::fs::read(&path)
                .map_err(|e| TokenizerError::ModelLoad(format!("{}: {}", path, e)))?;
            bytes_owned = raw;
            &bytes_owned
        } else {
            return Err(TokenizerError::NoModel);
        };

        let model_data = decompress_if_needed(model_bytes)
            .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;
        let model = Model::read(model_data.as_slice())
            .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;
        let predictor = Predictor::new(model, false)
            .map_err(|e| TokenizerError::ModelLoad(e.to_string()))?;

        Ok(Self { predictor, config })
    }

    /// `vaporetto_split(text, ' ')` と等価。FTS5 body カラムに格納する値を返す。
    pub fn split(&self, text: &str) -> String {
        self.collect_tokens(text).join(" ")
    }

    /// `vaporetto_and_query(text)` と等価。FTS5 MATCH 引数にそのまま渡せる。
    pub fn and_query(&self, text: &str) -> String {
        self.collect_tokens(text)
            .into_iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ")
    }

    pub fn or_query(&self, text: &str) -> String {
        self.collect_tokens(text)
            .into_iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ")
    }

    fn collect_tokens(&self, text: &str) -> Vec<String> {
        let mut tokens = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() { continue; }
            if let Ok(mut sentence) = Sentence::from_raw(line) {
                self.predictor.predict(&mut sentence);
                for token in sentence.iter_tokens() {
                    if self.should_include(&token) {
                        tokens.push(token.surface().to_string());
                    }
                }
            }
        }
        tokens
    }

    fn should_include(&self, token: &vaporetto::Token) -> bool {
        match &self.config.pos_filter {
            None => true,
            Some(prefixes) => {
                let tag = token.tag().unwrap_or("");
                if tag.is_empty() {
                    self.config.keep_untagged
                } else {
                    prefixes.iter().any(|p| tag.starts_with(p.as_str()))
                }
            }
        }
    }
}

fn decompress_if_needed(bytes: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    if bytes.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        let mut decoder = ruzstd::StreamingDecoder::new(bytes)?;
        let mut out = Vec::new();
        decoder.read_to_end(&mut out)?;
        Ok(out)
    } else {
        Ok(bytes.to_vec())
    }
}

/// フォールバック: モデルなし環境でのテスト用（空白分割）。
pub fn simple_tokenize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// フォールバック: simple_tokenize に対応した AND クエリビルダ。
pub fn simple_and_query(text: &str) -> String {
    text.split_whitespace()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}
```

- [ ] **(GREEN VERIFY) Step 5: Run tokenizer tests, confirm they pass**

Run: `cargo test -p obsidian-shiotsuchi-vault-core tokenizer`
Expected: 2 tests pass（モデル未埋め込みでも `simple_tokenize` / `simple_and_query` テストは通る）

- [ ] **Step 6: Write `scripts/download-model.sh`**

sqlite-vaporetto のリリース tarball から同じモデルを抽出する。

```bash
#!/usr/bin/env bash
set -euo pipefail
VERSION="0.4.0"
MODEL="bccwj-suw+unidic_pos+kana.model.zst"
DEST="models/${MODEL}"
mkdir -p models
if [ ! -f "$DEST" ]; then
    echo "Downloading Vaporetto model..."
    curl -sL \
      "https://github.com/hotchpotch/sqlite-vaporetto/releases/download/v${VERSION}/sqlite-vaporetto-v${VERSION}-$(uname -s | tr '[:upper:]' '[:lower:]')-x86_64-with-model.tar.gz" \
      | tar -xz --wildcards "*.model.zst" -O > "$DEST"
    echo "Saved: $DEST"
fi
```

- [ ] **Step 7: Add tokenizer module to lib.rs**

Modify `core/src/lib.rs`:
```rust
pub mod db;
pub mod indexer;
pub mod models;
pub mod search;
pub mod tokenizer;
pub mod watcher;

pub use db::NoteDatabase;
pub use indexer::Indexer;
pub use models::{NoteMetadata, SearchResult};
pub use search::Searcher;
pub use tokenizer::{JapaneseTokenizer, TokenizerConfig};
```

- [ ] **Step 8: Commit**

```bash
git add core/build.rs core/src/tokenizer.rs core/src/lib.rs scripts/download-model.sh .gitignore
git commit -m "feat(core): add Vaporetto tokenizer with build.rs model embedding"
```

---

## Task 6: File Walker and Indexer (TDD)

**TDD**: Follow RED → RED VERIFY → GREEN → GREEN VERIFY → REFACTOR for each behavior.

**Files:**
- Modify: `core/src/indexer.rs` (complete implementation)
- Test: `core/src/indexer.rs` (integration tests)

- [ ] **(RED) Step 1: Add failing tests for index_directory and cleanup_deleted**

Append to the existing test module in `indexer.rs` — `index_directory` and `cleanup_deleted` are not yet defined:

```rust
// テスト戦略: SHIOTSUCHI_MODEL_PATH が設定されていない場合は panic する。
// CI での実行: SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst cargo test

#[test]
fn test_index_directory() {
    // FAIL: index_directory not defined yet
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();

    let mut f1 = fs::File::create(vault.join("note1.md")).unwrap();
    writeln!(f1, "# Hello\n\nWorld content").unwrap();
    let mut f2 = fs::File::create(vault.join("note2.md")).unwrap();
    writeln!(f2, "---\ntitle: Special\n---\n\nUnique text here").unwrap();

    let db = NoteDatabase::open_in_memory().unwrap();
    let tokenizer = JapaneseTokenizer::new(Default::default())
        .unwrap_or_else(|_| panic!("SHIOTSUCHI_MODEL_PATH を設定してください"));
    let config = IndexConfig { notes_dir: vault.clone(), ..Default::default() };

    let results = index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(results.len(), 2);
    assert_eq!(db.stats().unwrap().total_notes, 2);
}

#[test]
fn test_cleanup_deleted() {
    // FAIL: cleanup_deleted not defined yet
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();

    let mut f = fs::File::create(vault.join("old.md")).unwrap();
    writeln!(f, "content").unwrap();

    let db = NoteDatabase::open_in_memory().unwrap();
    let tokenizer = JapaneseTokenizer::new(Default::default())
        .unwrap_or_else(|_| panic!("SHIOTSUCHI_MODEL_PATH を設定してください"));
    let config = IndexConfig { notes_dir: vault.clone(), ..Default::default() };
    index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(db.stats().unwrap().total_notes, 1);

    fs::remove_file(vault.join("old.md")).unwrap();
    let removed = cleanup_deleted(&db, &config).unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(db.stats().unwrap().total_notes, 0);
}
```

Also add required imports to the test module:
```rust
use crate::{db::NoteDatabase, tokenizer::{JapaneseTokenizer, TokenizerConfig}};
use std::io::Write;
use tempfile::TempDir;
```

- [ ] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p obsidian-shiotsuchi-vault-core indexer`
Expected: Compilation error — `index_directory`, `cleanup_deleted` not found

- [ ] **(GREEN) Step 3: Complete indexer.rs implementation**

Add `index_file`, `index_directory`, `cleanup_deleted`, and `compute_hash` to `indexer.rs`:

```rust
use crate::{
    db::{DbError, NoteDatabase},
    models::{IndexConfig, IndexResult},
    tokenizer::JapaneseTokenizer,
};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::SystemTime};
use walkdir::WalkDir;

/// Index a single file into the database.
/// `tokenizer` は呼び出し側が一度だけ初期化して渡す（モデルロードコストを1回に抑える）。
pub fn index_file(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    file_path: &Path,
    relative_path: &str,
    _config: &IndexConfig,
) -> IndexResult {
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return IndexResult::Error(format!("Read error: {}", e)),
    };
    let hash = compute_hash(&content);
    let mtime = fs::metadata(file_path)
        .and_then(|m| m.modified())
        .map(|t| t.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
        .unwrap_or(0);

    let (frontmatter_title, body) = extract_frontmatter(&content);
    let title = frontmatter_title.unwrap_or_else(|| title_from_path(file_path));
    let plain_text = markdown_to_text(&body);
    let tokenized = tokenizer.split(&plain_text);

    match db.upsert_note(relative_path, &title, &tokenized, &hash, mtime) {
        Ok(true) => IndexResult::Inserted,
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
    tokenizer: &JapaneseTokenizer,
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
        if !path.is_file() { continue; }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !config.include_extensions.iter().any(|e| e == ext) { continue; }
        let relative = path.strip_prefix(notes_dir).unwrap_or(path);
        let rel_str = relative.to_string_lossy();
        if config.exclude_patterns.iter().any(|pat| rel_str.contains(pat)) { continue; }
        let result = index_file(db, tokenizer, path, &rel_str, config);
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
```

- [ ] **(GREEN VERIFY) Step 4: Run indexer tests, confirm all pass**

Run: `cargo test -p obsidian-shiotsuchi-vault-core indexer`
Expected: 6 tests pass (4 parsing + 2 new indexer tests)

- [ ] **Step 3: Commit**

```bash
git add core/src/indexer.rs
git commit -m "feat(core): implement file walker and indexer with hash tracking"
```

---

## Task 7: Search and Snippet Extraction (TDD)

**TDD**: Follow RED → RED VERIFY → GREEN → GREEN VERIFY → REFACTOR for each behavior.

**Files:**
- Create: `core/src/search.rs`
- Modify: `core/src/db.rs` (add `search()` method)
- Test: `core/src/search.rs` (inline tests)

- [ ] **(RED) Step 1: Write failing tests only in search.rs**

Create `core/src/search.rs` with the test module only — no implementation yet:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_snippet_found() {
        // FAIL: extract_snippet not defined yet
        let text = "Line one\nLine two\nLine three\nLine four\nLine five";
        let snippet = extract_snippet(text, "three", 1);
        assert!(snippet.contains("three"));
    }

    #[test]
    fn test_extract_snippet_multiline() {
        let text = "A\nB\nC\nD\nE\nF\nG";
        let snippet = extract_snippet(text, "E", 1);
        assert!(snippet.contains("E"));
        assert!(snippet.contains("D") || snippet.contains("F"));
    }
}
```

- [ ] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p obsidian-shiotsuchi-vault-core search`
Expected: Compilation error — `extract_snippet` not found

- [ ] **(GREEN) Step 3: Write minimal search.rs implementation**

Add implementation above the test module:

```rust
use crate::{
    db::{DbError, NoteDatabase},
    models::SearchResult,
    tokenizer::{simple_and_query, JapaneseTokenizer},
};
use std::{fs, path::Path};

/// 検索のメインエントリポイント。tokenizer.and_query() → db.search() → extract_snippet()。
pub fn search(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    notes_dir: &Path,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchResult>, DbError> {
    let fts5_query = tokenizer.and_query(query);
    let fts5_query = if fts5_query.is_empty() { simple_and_query(query) } else { fts5_query };
    if fts5_query.is_empty() { return Ok(vec![]); }

    let mut results = db.search(&fts5_query, limit)?;

    for result in &mut results {
        let file_path = notes_dir.join(&result.path);
        if let Ok(content) = fs::read_to_string(&file_path) {
            result.snippet = extract_snippet(&content, query, 3);
        }
    }

    Ok(results)
}

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
            if i == 0 { idx = 0; }
        }
        idx
    };

    let snippet_text = &text[start..];
    let lines: Vec<&str> = snippet_text.lines().take(max_lines * 2 + 1).collect();
    let result = lines.join("\n");

    if result.len() > 500 { result.chars().take(500).collect::<String>() + "…" } else { result }
}
```

- [ ] **(GREEN VERIFY) Step 4: Run search tests, confirm they pass**

Run: `cargo test -p obsidian-shiotsuchi-vault-core search`
Expected: 2 tests pass

- [ ] **(RED) Step 5: Write failing test for NoteDatabase::search() in db.rs**

Add to `db.rs` test module — `NoteDatabase::search()` not yet defined:

```rust
#[test]
fn test_search_returns_results() {
    // FAIL: NoteDatabase::search not defined yet
    let db = NoteDatabase::open_in_memory().unwrap();
    db.upsert_note("note.md", "My Note", "hello world content", "hash1", 1000).unwrap();
    let results = db.search("\"hello\"", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, "note.md");
}
```

- [ ] **(RED VERIFY) Step 6: Run test, confirm it fails**

Run: `cargo test -p obsidian-shiotsuchi-vault-core db::tests::test_search`
Expected: Compilation error — `NoteDatabase::search` not found

- [ ] **(GREEN) Step 7: Add search() method to NoteDatabase in db.rs**

Modify `core/src/db.rs` to add:

```rust
/// Search notes using tokenized query. Returns results ordered by BM25 relevance.
/// `fts5_query` は呼び出し側で `tokenizer.and_query()` を使って構築すること。
pub fn search(&self, fts5_query: &str, limit: usize) -> Result<Vec<SearchResult>, DbError> {
    let sql = format!(
        "SELECT path, title, rank
         FROM notes_fts
         WHERE notes_fts MATCH ?1
         ORDER BY rank
         LIMIT {}",
        limit
    );

    let mut stmt = self.conn.prepare(&sql)?;
    let rows = stmt.query_map(params![fts5_query], |row| {
        Ok(SearchResult {
            path: row.get(0)?,
            title: row.get(1)?,
            snippet: String::new(),
            score: row.get(2)?,
        })
    })?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(DbError::Sqlite)
}
```

- [ ] **(GREEN VERIFY) Step 8: Run all db and search tests**

Run: `cargo test -p obsidian-shiotsuchi-vault-core`
Expected: All tests pass (db: 5 tests, search: 2 tests)

- [ ] **Step 9: Commit**

```bash
git add core/src/db.rs core/src/search.rs
git commit -m "feat(core): add BM25 search and snippet extraction"
```

---

## Task 8: File System Watcher (TDD)

**TDD**: Follow RED → RED VERIFY → GREEN → GREEN VERIFY → REFACTOR for each behavior.

**Files:**
- Create: `core/src/watcher.rs`
- Test: `core/src/watcher.rs` (inline tests)

Note: `VaultWatcher::new()` does not fail (watcher is created inside `watch()`), so the test simply verifies that construction succeeds — a minimal but meaningful assertion about the API contract.

- [ ] **(RED) Step 1: Write failing test only in watcher.rs**

Create `core/src/watcher.rs` with the test module only — no implementation yet:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::NoteDatabase, tokenizer::{JapaneseTokenizer, TokenizerConfig}};
    use tempfile::TempDir;

    #[test]
    fn test_watcher_setup() {
        // FAIL: VaultWatcher not defined yet
        let temp = TempDir::new().unwrap();
        let db = Arc::new(Mutex::new(NoteDatabase::open_in_memory().unwrap()));
        let tokenizer = Arc::new(
            JapaneseTokenizer::new(TokenizerConfig::default())
                .unwrap_or_else(|_| panic!("SHIOTSUCHI_MODEL_PATH を設定してください"))
        );
        let config = IndexConfig {
            notes_dir: temp.path().to_path_buf(),
            ..Default::default()
        };

        // VaultWatcher は new() で失敗しない（ウォッチャー生成は watch() 内）
        let _watcher = VaultWatcher::new(db, tokenizer, config);
    }
}
```

- [ ] **(RED VERIFY) Step 2: Run test, confirm it fails**

Run: `cargo test -p obsidian-shiotsuchi-vault-core watcher`
Expected: Compilation error — `VaultWatcher` not found

- [ ] **(GREEN) Step 3: Write minimal watcher.rs implementation**

Add implementation above the test module:

```rust
use crate::{
    db::NoteDatabase,
    indexer::index_file,
    models::IndexConfig,
    tokenizer::JapaneseTokenizer,
};
use notify::{Event, RecursiveMode, Watcher};
use std::sync::{mpsc::channel, Arc, Mutex};

/// Watch a directory for changes and incrementally re-index.
pub struct VaultWatcher {
    db: Arc<Mutex<NoteDatabase>>,
    tokenizer: Arc<JapaneseTokenizer>,
    config: IndexConfig,
}

impl VaultWatcher {
    pub fn new(
        db: Arc<Mutex<NoteDatabase>>,
        tokenizer: Arc<JapaneseTokenizer>,
        config: IndexConfig,
    ) -> Self {
        Self { db, tokenizer, config }
    }

    /// ファイル監視ループを開始する（Ctrl+C まで継続）。ウォッチャーはここで一度だけ生成する。
    pub fn watch(&self) -> Result<(), Box<dyn std::error::Error>> {
        let (tx, rx) = channel();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })?;

        watcher.watch(&self.config.notes_dir, RecursiveMode::Recursive)?;
        eprintln!("Watching {} for changes...", self.config.notes_dir.display());

        loop {
            match rx.recv() {
                Ok(event) => self.handle_event(&event)?,
                Err(e) => {
                    eprintln!("Watch error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    fn handle_event(&self, event: &Event) -> Result<(), Box<dyn std::error::Error>> {
        use notify::event::{EventKind, ModifyKind, RenameMode};

        match event.kind {
            EventKind::Modify(ModifyKind::Data(_)) | EventKind::Create(_) => {
                for path in &event.paths {
                    if let Ok(rel) = path.strip_prefix(&self.config.notes_dir) {
                        let rel_str = rel.to_string_lossy();
                        let db = self.db.lock().unwrap();
                        let _ = index_file(&db, &self.tokenizer, path, &rel_str, &self.config);
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in &event.paths {
                    if let Ok(rel) = path.strip_prefix(&self.config.notes_dir) {
                        let db = self.db.lock().unwrap();
                        let _ = db.delete_note(&rel.to_string_lossy());
                    }
                }
            }
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                if event.paths.len() == 2 {
                    let old = &event.paths[0];
                    let new = &event.paths[1];
                    if let Ok(old_rel) = old.strip_prefix(&self.config.notes_dir) {
                        let db = self.db.lock().unwrap();
                        let _ = db.delete_note(&old_rel.to_string_lossy());
                    }
                    if let Ok(new_rel) = new.strip_prefix(&self.config.notes_dir) {
                        let db = self.db.lock().unwrap();
                        let _ = index_file(&db, &self.tokenizer, new, &new_rel.to_string_lossy(), &self.config);
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }
}
```

- [ ] **(GREEN VERIFY) Step 4: Run watcher test, confirm it passes**

Run: `cargo test -p obsidian-shiotsuchi-vault-core watcher`
Expected: 1 test passes

- [ ] **Step 5: Commit**

```bash
git add core/src/watcher.rs
git commit -m "feat(core): add filesystem watcher with incremental re-indexing"
```

---

## Task 9: Integration Test - End-to-End Index and Search (TDD)

**TDD**: Follow RED → RED VERIFY → GREEN → GREEN VERIFY → REFACTOR for each behavior.

**Files:**
- Create: `tests/integration_test.rs`

テスト実行前提: `SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst`
（または `make release-embedded` でモデル埋め込みビルド後）

- [ ] **(RED) Step 1: Write integration test**

```rust
use obsidian_shiotsuchi_vault_core::{
    db::NoteDatabase,
    indexer::{index_directory, cleanup_deleted},
    models::IndexConfig,
    search::{search, extract_snippet},
    tokenizer::{JapaneseTokenizer, TokenizerConfig, simple_and_query},
};
use std::fs;
use tempfile::TempDir;

fn make_tokenizer() -> JapaneseTokenizer {
    JapaneseTokenizer::new(TokenizerConfig::default())
        .unwrap_or_else(|_| panic!("SHIOTSUCHI_MODEL_PATH を設定してください"))
}

#[test]
fn test_end_to_end_index_and_search() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();

    fs::write(
        vault.join("project.md"),
        "# Project Plan\n\nThis project is about building a search engine.",
    ).unwrap();

    fs::write(
        vault.join("meeting.md"),
        "---\ntitle: Team Meeting\n---\n\nWe discussed the search feature and timeline.",
    ).unwrap();

    fs::write(
        vault.join("japanese.md"),
        "# 日本語ノート\n\n形態素解析は非常に便利です。",
    ).unwrap();

    // Index: tokenizer を index_directory に渡す
    let db = NoteDatabase::open_in_memory().unwrap();
    let tokenizer = make_tokenizer();
    let config = IndexConfig { notes_dir: vault.clone(), ..Default::default() };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(results.len(), 3);

    // Search: tokenizer.and_query() で FTS5 AND クエリを構築してから db.search() に渡す
    let fts5_query = tokenizer.and_query("search engine");
    let search_results = db.search(&fts5_query, 10).unwrap();
    assert!(!search_results.is_empty());
    assert!(search_results[0].path.contains("project"));

    // Search 日本語: 同様に and_query() を経由する
    let ja_query = tokenizer.and_query("形態素");
    let ja_results = db.search(&ja_query, 10).unwrap();
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
    let snippet = extract_snippet(text, "keyword", 1);
    assert!(snippet.contains("keyword"));
}
```

- [ ] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test --test integration_test`
Expected: Compilation error — some symbols not yet exported from `lib.rs`, or test fails due to missing behavior

- [ ] **(GREEN) Step 3: Fix any compilation errors**

If the integration test fails to compile, fix the exports in `core/src/lib.rs`. Do NOT change the test assertions — fix the production code only.

- [ ] **(GREEN VERIFY) Step 4: Run integration tests, confirm all pass**

Run: `cargo test --test integration_test`
Expected: 2 tests pass

- [ ] **Step 5: Commit**

```bash
git add tests/integration_test.rs
git commit -m "test: add end-to-end integration tests"
```

---

## Self-Review

### 1. Spec Coverage Check

| Spec Requirement | Plan Task |
|------------------|-----------|
| SQLite FTS5 schema（`content=''` なし） | Task 3 |
| Hash + mtime tracking | Task 3, 6 |
| `tokenizer.rs` 独立モジュール（`split()` / `and_query()`） | Task 5 |
| `build.rs` モデル埋め込み | Task 5 |
| Frontmatter extraction | Task 4, 6 |
| Markdown→plain text | Task 4, 6 |
| File walker with exclusions | Task 6 |
| BM25 search（`notes_fts MATCH`、全カラム） | Task 7 |
| 3-line snippet extraction | Task 7 |
| Filesystem watcher（ダブルウォッチャーバグなし） | Task 8 |
| Config model | Task 2 |
| Error handling (thiserror) | Task 3 |
| Unit + integration tests | All tasks |

### 2. TDD Cycle Compliance

- ✅ Task 1: TDD不適用（設定ファイル・空モジュールスタブ）と明示
- ✅ Task 2: RED → RED VERIFY → GREEN → GREEN VERIFY → REFACTOR → 次のRED → RED VERIFY → GREEN → GREEN VERIFY
- ✅ Task 3: テストのみのStep（RED）→ VERIFY → 実装のStep（GREEN）→ VERIFY に分解
- ✅ Task 4: 同様にREDフェーズとGREENフェーズを明確分離
- ✅ Task 5: テストのみStep → RED VERIFY → 実装Step → GREEN VERIFY
- ✅ Task 6: 新機能（index_directory / cleanup_deleted）のみをREDに追加 → VERIFY → GREEN → VERIFY
- ✅ Task 7: extract_snippet と NoteDatabase::search を別サイクルに分離
- ✅ Task 8: VaultWatcher のテストのみStep → RED VERIFY → 実装Step → GREEN VERIFY
- ✅ Task 9: RED → RED VERIFY → GREEN（コンパイルエラー修正）→ GREEN VERIFY

### 3. Placeholder Scan

- ✅ `// ... (same as Task 4)` プレースホルダは Task 6 に完全実装を展開済み
- ✅ `todo!()` は削除済み
- ✅ `body MATCH` → `notes_fts MATCH` に修正済み
- ✅ `index_directory` / `index_file` / `handle_event` すべてに `tokenizer` 引数あり
- ✅ 統合テストが `tokenizer.and_query()` を経由して `db.search()` を呼ぶ

### 4. Type Consistency

- ✅ `NoteMetadata` fields match between `models.rs` and `db.rs`
- ✅ `IndexResult` used consistently in `indexer.rs`
- ✅ `SearchResult` used in `search.rs` and `db.rs`
- ✅ `IndexConfig` used in `indexer.rs` and `watcher.rs`
- ✅ `VaultWatcher::new()` のシグネチャに `tokenizer: Arc<JapaneseTokenizer>` あり

### 5. テスト実行前提

```bash
# モデルをダウンロードしてからテスト
./scripts/download-model.sh
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
    cargo test -p obsidian-shiotsuchi-vault-core

# またはモデル埋め込みビルドでテスト
SHIOTSUCHI_EMBED_MODEL=$(pwd)/models/bccwj-suw+unidic_pos+kana.model.zst \
    cargo test -p obsidian-shiotsuchi-vault-core
```

---

## Next Steps (Post-Core)

After completing this plan, the following phases are ready for implementation:

1. **Phase 2: CLI** - `cli/` crate with `shiotsuchi` binary（`dive`, `chart`, `tide`, `scan`, `log` コマンド）
   - `drift` コマンドは廃止。MCP は独立バイナリ `shiotsuchi-mcp`。
   - `dive --json` フラグ（compact JSON 出力）
   - `config.toml` 読み込み対応（Phase 2 で同時実装）
2. **Phase 3: Skill** - `skill/` crate with Kilo skill protocol（Phase 3 でプロトコル調査）
3. **Phase 4: MCP** - `mcp/` crate with separate `shiotsuchi-mcp` binary（独立バイナリ）
4. **Phase 5: Polish** - watcher `scan` コマンド、benchmarks、README
