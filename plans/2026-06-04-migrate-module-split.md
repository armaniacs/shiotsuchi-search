# migrate() Module Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Split the 245-line `migrate()` method in `core/src/db.rs` into per-version files under `core/src/migration/`.

**Architecture:** A dispatcher `run()` function reads `PRAGMA user_version` once, cleans up orphans, then calls version-specific functions sequentially. Each version function lives in its own file and contains the exact DDL/DML from the original `migrate()` block. No logic changes.

**Tech Stack:** Rust, rusqlite

**Spec:** `plans/2026-06-04-migrate-module-split-design.md`

---

## File Structure

| File | Purpose |
|------|---------|
| `core/src/migration/mod.rs` | `run()` dispatcher + `create_schema()` free function |
| `core/src/migration/v02.rs` | v1→v2: DROP old tables, create_schema, bump version |
| `core/src/migration/v03.rs` | v2→v3: vault_name column + file_cache_v3 restructure |
| `core/src/migration/v04.rs` | v3→v4: recreate vec_chunks with FLOAT[1024] |
| `core/src/migration/v05.rs` | v4→v5: add file_size to file_cache |
| `core/src/migration/v06.rs` | v5→v6: add tags, frontmatter_date, title to chunks |
| `core/src/migration/v07.rs` | v6→v7: create tasks table + self-heal v6 columns |
| `core/src/migration/v08.rs` | v7→v8: add emphasized_text to chunks |
| `core/src/migration/v09.rs` | v8→v9: add note_links table + backlink_count |
| `core/src/migration/v10.rs` | v9→v10: add char_count + tag_counts table |
| `core/src/migration/v11.rs` | v10→v11: add vlm_hash to file_cache |
| `core/src/db.rs` | Remove `migrate()` body + `create_schema()`, call `crate::migration::run()` |
| `core/src/lib.rs` | Add `pub mod migration;` |

---

## Task 1: Create migration module skeleton with dispatcher and create_schema

**Files:**
- Create: `core/src/migration/mod.rs`
- Modify: `core/src/lib.rs:9` (add module declaration)

- [ ] **Step 1: Create `core/src/migration/mod.rs` with dispatcher and create_schema**

```rust
// core/src/migration/mod.rs
use rusqlite::Connection;

mod v02;
mod v03;
mod v04;
mod v05;
mod v06;
mod v07;
mod v08;
mod v09;
mod v10;
mod v11;

/// Run all pending schema migrations.
pub fn run(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Clean up orphaned file_cache_v3 from a previous crash (runs every migration)
    conn.execute_batch("DROP TABLE IF EXISTS file_cache_v3")?;

    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if version < 2  { v02::migrate(conn)?; }
    if version < 3  { v03::migrate(conn)?; }
    if version < 4  { v04::migrate(conn)?; }
    if version < 5  { v05::migrate(conn)?; }
    if version < 6  { v06::migrate(conn)?; }
    if version < 7  { v07::migrate(conn)?; }
    if version < 8  { v08::migrate(conn)?; }
    if version < 9  { v09::migrate(conn)?; }
    if version < 10 { v10::migrate(conn)?; }
    if version < 11 { v11::migrate(conn)?; }

    Ok(())
}

/// Create the full v11 schema from scratch.
/// Called by v02 migration after dropping old tables.
pub(crate) fn create_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS file_cache (
            vault_name      TEXT NOT NULL,
            path            TEXT NOT NULL,
            hash            TEXT NOT NULL,
            mtime           INTEGER NOT NULL,
            model_id        TEXT NOT NULL,
            file_size       INTEGER NOT NULL DEFAULT 0,
            backlink_count  INTEGER NOT NULL DEFAULT 0,
            char_count      INTEGER NOT NULL DEFAULT 0,
            vlm_hash        TEXT,
            PRIMARY KEY (vault_name, path)
        );

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

        CREATE TABLE IF NOT EXISTS tasks (
            id          INTEGER PRIMARY KEY,
            vault_name  TEXT NOT NULL,
            file_path   TEXT NOT NULL,
            content     TEXT NOT NULL,
            checked     INTEGER NOT NULL DEFAULT 0,
            line_number INTEGER NOT NULL DEFAULT 0,
            indexed_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS note_links (
            source_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            vault_name  TEXT NOT NULL,
            PRIMARY KEY (source_path, target_path, vault_name)
        );
        CREATE INDEX IF NOT EXISTS idx_note_links_target
            ON note_links(target_path, vault_name);

        CREATE TABLE IF NOT EXISTS tag_counts (
            tag        TEXT NOT NULL,
            vault_name TEXT NOT NULL,
            count      INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (tag, vault_name)
        ) WITHOUT ROWID;

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
```

- [ ] **Step 2: Add module declaration to lib.rs**

In `core/src/lib.rs`, add `pub mod migration;` after `pub mod db;` (line 9):

```rust
pub mod db;
pub mod migration;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles with warnings about unused modules (v02-v11 not yet created)

- [ ] **Step 4: Commit**

```bash
git add core/src/migration/mod.rs core/src/lib.rs
git commit -m "refactor(migration): add module skeleton with dispatcher and create_schema"
```

---

## Task 2: Create v02.rs — DROP old tables + create_schema

**Files:**
- Create: `core/src/migration/v02.rs`

- [ ] **Step 1: Create `core/src/migration/v02.rs`**

```rust
// core/src/migration/v02.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Wrap v1→v2 migration in a transaction for crash safety.
    // DROP + schema creation + version bump must be atomic.
    conn.execute_batch("BEGIN TRANSACTION")?;
    conn.execute_batch("
        DROP TABLE IF EXISTS notes_fts;
        DROP TABLE IF EXISTS notes_meta;
    ")?;
    super::create_schema(conn)?;
    conn.execute_batch("PRAGMA user_version = 2")?;
    conn.execute_batch("COMMIT")?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles (v02 imported, others still missing)

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v02.rs
git commit -m "refactor(migration): extract v02 (DROP old tables + create_schema)"
```

---

## Task 3: Create v03.rs — vault_name + file_cache restructure

**Files:**
- Create: `core/src/migration/v03.rs`

- [ ] **Step 1: Create `core/src/migration/v03.rs`**

```rust
// core/src/migration/v03.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // Check if vault_name column already exists (crash recovery)
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(chunks)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let has_vault_name = cols.iter().any(|c| c == "vault_name");

    if !has_vault_name {
        conn.execute_batch("BEGIN TRANSACTION")?;
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN vault_name TEXT NOT NULL DEFAULT 'default'")?;
        conn.execute_batch("DROP INDEX IF EXISTS idx_chunks_file_path")?;
        conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_chunks_file_path ON chunks(vault_name, file_path)")?;
        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS file_cache_v3 (
                vault_name TEXT NOT NULL,
                path TEXT NOT NULL,
                hash TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                model_id TEXT NOT NULL,
                file_size INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (vault_name, path)
            )
        ")?;
        // file_size may or may not exist in the source file_cache
        // depending on whether create_schema already included it.
        let fc_cols: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(file_cache)")?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if fc_cols.iter().any(|c| c == "file_size") {
            conn.execute_batch("
                INSERT INTO file_cache_v3 (vault_name, path, hash, mtime, model_id, file_size)
                SELECT 'default', path, hash, mtime, model_id, file_size FROM file_cache
            ")?;
        } else {
            conn.execute_batch("
                INSERT INTO file_cache_v3 (vault_name, path, hash, mtime, model_id, file_size)
                SELECT 'default', path, hash, mtime, model_id, 0 FROM file_cache
            ")?;
        }
        conn.execute_batch("DROP TABLE file_cache")?;
        conn.execute_batch("ALTER TABLE file_cache_v3 RENAME TO file_cache")?;
        conn.execute_batch("PRAGMA user_version = 3")?;
        conn.execute_batch("COMMIT")?;
    } else {
        // Already partially/fully migrated — just ensure user_version is correct
        conn.execute_batch("PRAGMA user_version = 3")?;
    }
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v03.rs
git commit -m "refactor(migration): extract v03 (vault_name + file_cache restructure)"
```

---

## Task 4: Create v04.rs — vec_chunks FLOAT[1024]

**Files:**
- Create: `core/src/migration/v04.rs`

- [ ] **Step 1: Create `core/src/migration/v04.rs`**

```rust
// core/src/migration/v04.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v3→v4: recreate vec_chunks to ensure FLOAT type.
    // (sqlite-vec 0.1.x does not support FLOAT2/FLOAT4_BINARY.)
    // vec0 is a virtual table, so we must DROP and recreate.
    conn.execute_batch("DROP TABLE IF EXISTS vec_chunks")?;
    conn.execute_batch("
        CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(
            chunk_id  INTEGER PRIMARY KEY,
            embedding FLOAT[1024]
        )
    ")?;
    conn.execute_batch("PRAGMA user_version = 4")?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v04.rs
git commit -m "refactor(migration): extract v04 (vec_chunks FLOAT[1024])"
```

---

## Task 5: Create v05.rs — file_size column

**Files:**
- Create: `core/src/migration/v05.rs`

- [ ] **Step 1: Create `core/src/migration/v05.rs`**

```rust
// core/src/migration/v05.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v4→v5: add file_size column to file_cache for two-stage skip (mtime+size).
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(file_cache)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !cols.iter().any(|c| c == "file_size") {
        conn.execute_batch(
            "ALTER TABLE file_cache ADD COLUMN file_size INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    conn.execute_batch("PRAGMA user_version = 5")?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v05.rs
git commit -m "refactor(migration): extract v05 (file_size column)"
```

---

## Task 6: Create v06.rs — tags, frontmatter_date, title

**Files:**
- Create: `core/src/migration/v06.rs`

- [ ] **Step 1: Create `core/src/migration/v06.rs`**

```rust
// core/src/migration/v06.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v5→v6: add tags, frontmatter_date, title columns to chunks table
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(chunks)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !cols.iter().any(|c| c == "tags") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN tags TEXT NOT NULL DEFAULT ''")?;
    }
    if !cols.iter().any(|c| c == "frontmatter_date") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN frontmatter_date TEXT NOT NULL DEFAULT ''")?;
    }
    if !cols.iter().any(|c| c == "title") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN title TEXT NOT NULL DEFAULT ''")?;
    }
    conn.execute_batch("PRAGMA user_version = 6")?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v06.rs
git commit -m "refactor(migration): extract v06 (tags, frontmatter_date, title)"
```

---

## Task 7: Create v07.rs — tasks table + self-heal

**Files:**
- Create: `core/src/migration/v07.rs`

- [ ] **Step 1: Create `core/src/migration/v07.rs`**

```rust
// core/src/migration/v07.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v6→v7: create tasks table (runs AFTER v6 to avoid column-loss on crash).
    // Defensively check for v6 columns — if missing, add them before proceeding.
    // This self-heals any database that was bumped to a version >= 6 via the
    // old (buggy) migration ordering where v7 ran before v6.
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(chunks)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !cols.iter().any(|c| c == "tags") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN tags TEXT NOT NULL DEFAULT ''")?;
    }
    if !cols.iter().any(|c| c == "frontmatter_date") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN frontmatter_date TEXT NOT NULL DEFAULT ''")?;
    }
    if !cols.iter().any(|c| c == "title") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN title TEXT NOT NULL DEFAULT ''")?;
    }
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS tasks (
            id INTEGER PRIMARY KEY,
            vault_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            content TEXT NOT NULL,
            checked INTEGER NOT NULL DEFAULT 0,
            line_number INTEGER NOT NULL DEFAULT 0,
            indexed_at TEXT NOT NULL DEFAULT (datetime('now'))
        )
    ")?;
    conn.execute_batch("PRAGMA user_version = 7")?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v07.rs
git commit -m "refactor(migration): extract v07 (tasks table + self-heal)"
```

---

## Task 8: Create v08.rs — emphasized_text

**Files:**
- Create: `core/src/migration/v08.rs`

- [ ] **Step 1: Create `core/src/migration/v08.rs`**

```rust
// core/src/migration/v08.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v7→v8: add emphasized_text column to chunks table
    let cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(chunks)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !cols.iter().any(|c| c == "emphasized_text") {
        conn.execute_batch("ALTER TABLE chunks ADD COLUMN emphasized_text TEXT NOT NULL DEFAULT ''")?;
    }
    conn.execute_batch("PRAGMA user_version = 8")?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v08.rs
git commit -m "refactor(migration): extract v08 (emphasized_text)"
```

---

## Task 9: Create v09.rs — note_links + backlink_count (transaction)

**Files:**
- Create: `core/src/migration/v09.rs`

- [ ] **Step 1: Create `core/src/migration/v09.rs`**

```rust
// core/src/migration/v09.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v8→v9: add note_links table and backlink_count column to file_cache
    // Wrap multi-statement migration in a transaction for crash safety.
    conn.execute_batch("BEGIN TRANSACTION")?;
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS note_links (
            source_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            vault_name  TEXT NOT NULL,
            PRIMARY KEY (source_path, target_path, vault_name)
        )
    ")?;
    conn.execute_batch("
        CREATE INDEX IF NOT EXISTS idx_note_links_target
        ON note_links(target_path, vault_name)
    ")?;
    let fc_cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(file_cache)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !fc_cols.iter().any(|c| c == "backlink_count") {
        conn.execute_batch(
            "ALTER TABLE file_cache ADD COLUMN backlink_count INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    conn.execute_batch("PRAGMA user_version = 9")?;
    conn.execute_batch("COMMIT")?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v09.rs
git commit -m "refactor(migration): extract v09 (note_links + backlink_count)"
```

---

## Task 10: Create v10.rs — char_count + tag_counts (transaction)

**Files:**
- Create: `core/src/migration/v10.rs`

- [ ] **Step 1: Create `core/src/migration/v10.rs`**

```rust
// core/src/migration/v10.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v9→v10: add char_count to file_cache, create tag_counts table.
    // Multi-statement migration: wrap in transaction for crash safety.
    conn.execute_batch("BEGIN TRANSACTION")?;
    let fc_cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(file_cache)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !fc_cols.iter().any(|c| c == "char_count") {
        conn.execute_batch(
            "ALTER TABLE file_cache ADD COLUMN char_count INTEGER NOT NULL DEFAULT 0",
        )?;
    }
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS tag_counts (
            tag        TEXT NOT NULL,
            vault_name TEXT NOT NULL,
            count      INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (tag, vault_name)
        ) WITHOUT ROWID
    ")?;
    // NOTE: char_count is intentionally NOT backfilled here. SQLite LENGTH()
    // returns UTF-8 byte count, not Unicode character count, which would
    // inflate values for non-ASCII text. char_count is computed correctly
    // via .chars().count() in reindex_file(), so upgraded databases get
    // accurate values on the next re-index — same design as tag_counts.
    conn.execute_batch("PRAGMA user_version = 10")?;
    conn.execute_batch("COMMIT")?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v10.rs
git commit -m "refactor(migration): extract v10 (char_count + tag_counts)"
```

---

## Task 11: Create v11.rs — vlm_hash

**Files:**
- Create: `core/src/migration/v11.rs`

- [ ] **Step 1: Create `core/src/migration/v11.rs`**

```rust
// core/src/migration/v11.rs
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // v10→v11: add vlm_hash column to file_cache for VLM extraction caching.
    let fc_cols: Vec<String> = {
        let mut stmt = conn.prepare("PRAGMA table_info(file_cache)")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(1))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    if !fc_cols.iter().any(|c| c == "vlm_hash") {
        conn.execute_batch(
            "ALTER TABLE file_cache ADD COLUMN vlm_hash TEXT",
        )?;
    }
    conn.execute_batch("PRAGMA user_version = 11")?;
    Ok(())
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles (all modules now exist)

- [ ] **Step 3: Commit**

```bash
git add core/src/migration/v11.rs
git commit -m "refactor(migration): extract v11 (vlm_hash)"
```

---

## Task 12: Replace migrate() in db.rs + remove create_schema()

**Files:**
- Modify: `core/src/db.rs:119-434` (replace migrate body, remove create_schema)

- [ ] **Step 1: Replace migrate() body in db.rs**

Replace lines 119-363 (`fn migrate` through its closing brace) with:

```rust
    fn migrate(&self) -> Result<(), DbError> {
        let conn = self.write_conn.borrow();
        crate::migration::run(&conn)
    }
```

- [ ] **Step 2: Remove create_schema() method**

Delete lines 365-434 (the entire `fn create_schema` method).

- [ ] **Step 3: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles without warnings

- [ ] **Step 4: Run all tests**

Run: `cargo test -p shiotsuchi-core`
Expected: All tests pass (including `core/tests/migration.rs`)

- [ ] **Step 5: Commit**

```bash
git add core/src/db.rs
git commit -m "refactor(migration): replace migrate() with migration::run(), remove create_schema()"
```

---

## Task 13: Final verification

- [ ] **Step 1: Full workspace build**

Run: `cargo build --workspace`
Expected: Builds successfully

- [ ] **Step 2: Full workspace tests**

Run: `cargo test --workspace`
Expected: All tests pass

- [ ] **Step 3: Verify no leftover references**

Run: `rg "fn create_schema" core/src/`
Expected: Only `core/src/migration/mod.rs` matches

Run: `rg "fn migrate\b" core/src/`
Expected: Only `core/src/db.rs` (the one-liner wrapper) and `core/src/migration/mod.rs` (the dispatcher) match

- [ ] **Step 4: Final commit if needed**

If any cleanup was needed, commit it.
