# FTS5 — SQLite Full-Text Search Engine

> **FTS5** (Full-Text Search version 5) is the built-in full-text search engine of SQLite. It is the core technology that powers search in shiotsuchi-search.

---

## What is FTS5?

FTS5 is a **virtual table module** for SQLite that provides:

- **Full-text indexing** — inverted index for fast keyword lookups
- **BM25 ranking** — built-in relevance scoring (Okapi BM25 algorithm)
- **Prefix, phrase, and NEAR queries** — flexible query syntax
- **Incremental updates** — add/delete rows without rebuilding the index
- **No external dependencies** — ships with SQLite itself

FTS5 is the successor to FTS4 and FTS3, and has been included in SQLite since version 3.9.0 (October 2015).

```sql
-- Create an FTS5 virtual table
CREATE VIRTUAL TABLE notes_fts USING fts5(
    title,
    body,
    tokenize='unicode61 remove_diacritics 0'
);

-- Search with BM25 ranking
SELECT title, rank FROM notes_fts
WHERE body MATCH '"project" AND "planning"'
ORDER BY rank;
```

---

## Why FTS5 for Shiotsuchi Search?

| Requirement | How FTS5 addresses it |
|-------------|----------------------|
| Sub-second search across 10,000+ notes | Inverted index with BM25 ranking |
| No external database server | SQLite is embedded — zero configuration, single file |
| Concurrent CLI + MCP access | WAL mode (Write-Ahead Logging) allows concurrent reads during writes |
| Portable across platforms | SQLite runs everywhere — macOS, Linux, Windows |
| Incremental indexing | Add/delete rows without full re-index |
| Privacy-first | Everything is local — no cloud, no network calls |

### Alternative approaches considered

| Approach | Why not chosen |
|----------|----------------|
| **Elasticsearch / Meilisearch** | Requires running a separate server process; overkill for a local note vault |
| **Apache Lucene / Tantivy** | Pure Rust alternatives, but introduce a larger dependency; SQLite is already in the stack |
| **Custom inverted index** | Re-inventing the wheel; FTS5 is battle-tested and well-documented |
| **ripgrep / grep** | No incremental indexing; full scan every time |

**Verdict**: FTS5 hits the sweet spot of being lightweight, zero-admin, and fast enough for vaults of 100,000+ notes.

---

## Architecture: Vaporetto × FTS5

FTS5's built-in tokenizers (`unicode61`, `porter`, etc.) do not handle Japanese word segmentation. Japanese has no spaces between words, so the standard approach of splitting on whitespace fails.

Shiotsuchi Search solves this with a **two-stage pipeline**:

```
User's query: "プロジェクト計画"
                    │
                    ▼
         ┌─────────────────────┐
         │  Vaporetto          │  Japanese tokenizer (Rust)
         │  (in-process)       │  splits into words
         └─────────┬───────────┘
                    │
                    ▼
         Tokens: "プロジェクト 計画"
                    │
                    ▼
         ┌─────────────────────┐
         │  FTS5 MATCH query   │  '"プロジェクト" AND "計画"'
         │  with BM25 ranking  │
         └─────────┬───────────┘
                    │
                    ▼
         ┌─────────────────────┐
         │  SQLite result set  │  path + title + snippet + score
         └─────────────────────┘
```

This design means:

- **Vaporetto handles Japanese segmentation** in Rust (not as a SQLite extension)
- **FTS5 handles the inverted index and ranking** using its built-in `unicode61` tokenizer on the already-segmented text
- The system avoids platform-dependent `.so`/`.dylib` distribution issues that would arise from a custom FTS5 C extension

### Why not use FTS5's tokenizer extensibility?

FTS5 supports loading custom tokenizers as C extensions. However, distributing a Vaporetto-based SQLite extension would require platform-specific shared libraries (`.so` on Linux, `.dylib` on macOS). By tokenizing in Rust and storing space-separated tokens in FTS5's `body` column, the entire system compiles to a single portable binary.

---

## Database Schema

Shiotsuchi Search uses a **three-table design**:

```sql
-- FTS5 virtual table (external content → chunks)
CREATE VIRTUAL TABLE fts_chunks USING fts5(
    tokenized_content,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 0'
);

-- Main chunk storage
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_path TEXT,
    chunk_index INTEGER,
    parent_header TEXT,
    content TEXT,
    tokenized_content TEXT,
    vault_name TEXT,
    tags TEXT,
    frontmatter_date TEXT,
    title TEXT,
    emphasized_text TEXT
);

-- Incremental index cache
CREATE TABLE file_cache (
    vault_name TEXT,
    path TEXT,
    hash TEXT,
    mtime INTEGER,
    model_id TEXT,
    file_size INTEGER DEFAULT 0,
    backlink_count INTEGER DEFAULT 0,
    char_count INTEGER DEFAULT 0,
    PRIMARY KEY (vault_name, path)
);
```

**Why three tables?** The `chunks` table stores the actual content, split into manageable pieces. `fts_chunks` is an FTS5 virtual table with `content='chunks'` (external content mode) to avoid redundant storage. `file_cache` tracks per-file hashes and metadata for incremental indexing.

---

## Query Format

User queries are processed through Vaporetto tokenization and converted to FTS5 MATCH syntax:

| User input | Tokenized | FTS5 query |
|-----------|-----------|------------|
| `東京 検索` | `東京 検索` | `"東京" AND "検索"` |
| `プロジェクト計画` | `プロジェクト 計画` | `"プロジェクト" AND "計画"` |
| `明日の天気` | `明日 の 天気` | `"明日" AND "の" AND "天気"` |

Each token is wrapped in double quotes and joined with `AND`. Double quotes inside tokens are escaped as `""`.

```rust
// Pseudocode: how tokenizer builds an AND query
fn and_query(text: &str) -> String {
    let tokens: Vec<&str> = tokenizer.split(text).collect();
    tokens.iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}
```

---

## BM25 Ranking

FTS5 uses the **Okapi BM25** algorithm to rank search results. The `rank` column (lower = better match) is calculated based on:

- **Term frequency (TF)** — how often a term appears in a document
- **Inverse document frequency (IDF)** — how rare a term is across the corpus
- **Document length normalization** — shorter documents get a boost

FTS5's BM25 implementation is tuned for general text and works well with Japanese tokenized content out of the box.

---

## WAL Mode for Concurrent Access

Shiotsuchi Search enables SQLite's **WAL (Write-Ahead Logging)** mode on database open:

```rust
conn.execute_batch("PRAGMA journal_mode=wal;")?;
```

This allows:

- **Concurrent reads** — the CLI and MCP server can search simultaneously
- **Non-blocking writes** — indexing does not block ongoing searches
- **Better performance** — reduced fsync overhead

---

## Frequently Asked Questions

### Is FTS5 the same as the search in SQLite?

FTS5 is a module *for* SQLite. Plain SQLite supports basic `LIKE` and `GLOB` pattern matching, but not full-text indexing. FTS5 is an optional module that provides the `CREATE VIRTUAL TABLE ... USING fts5` syntax.

### Does FTS5 support fuzzy search?

FTS5 itself does not support fuzzy/edit-distance search. However, shiotsuchi supports fuzzy matching at the application level via `--fuzzy` flag, which applies Unicode NFKC normalization and ASCII lowercasing before tokenization.

### Is the FTS5 index stored separately from the database?

FTS5 stores its index internally within the SQLite database file (the same `.sqlite3` file). No separate files are created.

### Can I query the database directly?

Yes. The SQLite database is stored at `~/.cache/shiotsuchi/db.sqlite3` (or your configured `db_path`). You can inspect it with:

```sh
sqlite3 ~/.cache/shiotsuchi/db.sqlite3
```

Then run queries like:

```sql
-- See indexed chunks
SELECT file_path, title FROM chunks ORDER BY rowid DESC LIMIT 10;

-- Search directly
SELECT file_path, title, rank FROM fts_chunks
WHERE fts_chunks MATCH '"検索" AND "エンジン"'
ORDER BY rank;
```

---

## References

- [SQLite FTS5 Documentation](https://www.sqlite.org/fts5.html)
- [Okapi BM25 on Wikipedia](https://en.wikipedia.org/wiki/Okapi_BM25)
- [Vaporetto — Rust Japanese tokenizer](https://github.com/daac-tools/vaporetto)
- [Architecture overview](../ref/architecture.md)
- [Core library reference](../ref/core.md)
