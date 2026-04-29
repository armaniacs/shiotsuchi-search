# Shiotsuchi-Search Design Specification

**Project**: shiotsuchi-search  
**Tagline**: "Guiding your path through the data tide." — As the deity Shiotsuchi guides travelers through misty seas, Shiotsuchi-Search guides you through the sea of your notes.  
**Date**: 2026-04-29  
**Status**: Draft

---

## 1. Vision & Purpose

**Shiotsuchi** (塩椎) is a high-performance Rust tool that indexes Markdown note directories (including Obsidian vaults), enabling AI assistants (via MCP) and developers to instantly find relevant context across thousands of notes.

**Core Problem Solved**:  
AI assistants like Claude Desktop lack knowledge of user's personal notes. Shiotsuchi bridges this gap by providing a blazingly fast, Japanese-aware search engine that surfaces the right snippet at the right time.

**Key Differentiator**:  
Vaporetto (Japanese tokenizer) × SQLite FTS5 = sub-second search across 10,000+ notes.

---

## 2. System Architecture

### 2.1 High-Level Overview

```
┌─────────────────────────────────────────────────────────────┐
│                        User Interface Layer                 │
├───────────────┬───────────────┬─────────────────────────────┤
│    Standalone │  Kilo Skill   │     MCP Server              │
│      CLI      │  (local)      │  (AI integration)           │
└───────┬───────┴───────┬───────┴───────────────┬─────────────┘
        │               │                       │
        └───────────────┼───────────────────────┘
                        │
              ┌─────────▼─────────┐
              │   Core Library    │  ←  shared logic
              │  (lib obsidian-  │     - Indexing
              │   shiotsuchi-vault │     - Search
              │      -core)      │     - DB ops
              └─────────┬─────────┘
                        │
              ┌─────────▼─────────┐
              │   External Crates │
              │  - vaporetto      │
              │  - rusqlite       │
              │  - pulldown-cmark │
              │  - notify         │
              └───────────────────┘
```

### 2.2 Multi-Interface Strategy

**Approach**: 3 independent frontends sharing a single core library

| Frontend | Purpose | Invocation |
|----------|---------|------------|
| **CLI** (`shiotsuchi`) | Direct terminal use, scripting | `shiotsuchi dive "query"` |
| **Killo Skill** | Integrated into Kilo workflow | `kilo search-vault` |
| **MCP Server** | Claude Desktop / Codex integration | stdio transport |

**Rationale**: Maximum flexibility; each interface optimized for its use case while code stays DRY.

---

## 3. Core Library Design (`obsidian-shiotsuchi-vault-core`)

### 3.1 Modules & Responsibilities

```rust
core/
├── lib.rs              # Public API exports
├── models.rs           # Shared data structures
├── db.rs               # SQLite FTS5 schema, hash tracking
├── indexer.rs          # Vaporetto tokenization + DB insertion
├── search.rs           # BM25 ranking + snippet extraction
└── watcher.rs          # File system notifications (optional feature)
```

### 3.2 Database Schema

```sql
-- Main FTS5 table for search
-- content='' (contentless) は削除。通常コンテンツテーブルとして扱う。
-- 理由: contentless table は DELETE に特殊構文が必要で実装が複雑になるため。
CREATE VIRTUAL TABLE notes_fts USING fts5(
    path UNINDEXED,        -- normalized relative path (e.g., "project/meeting.md")
    title,                 -- frontmatter title (if present) or filename
    body,                  -- space-separated Vaporetto tokens
    tokenize='unicode61 remove_diacritics 0'
);

-- Metadata table (not FTS)
CREATE TABLE notes_meta (
    path TEXT PRIMARY KEY,         -- unique identifier
    hash TEXT NOT NULL,            -- SHA-256 of file content
    mtime INTEGER NOT NULL,        -- last modified timestamp
    indexed_at INTEGER NOT NULL,   -- when this record was last updated
    title TEXT                     -- cached title for quick access
);
```

**Design Decisions**:
- **Hash field**: SHA-256 hex string (64 chars). Enables content-based change detection.
- **mtime field**: Unix timestamp (seconds). Used for quick pre-filter before hashing.
- **Path normalization**: All paths stored relative to vault root using forward slashes.

### 3.3 Indexing Algorithm

```
For each markdown file in directory tree:
  1. Read file content as UTF-8 (skip non-UTF-8 with warning)
  2. Extract YAML frontmatter (if any), strip from body
  3. Extract title: frontmatter.title || filename stem
  4. Parse Markdown → plain text via pulldown-cmark
  5. Normalize whitespace, collapse newlines
  6. Tokenize with Vaporetto (sentence mode + user dict)
  7. Join tokens with single space (UTF-8 spaces, not ASCII-only)
  8. Compute SHA-256 hash of original content
  9. Get file mtime
  10. Upsert into DB:
      - If path exists: compare hashes → skip if identical, update if different
      - If new: insert both notes_fts and notes_meta
  11. Log progress (files/sec, errors)
```

**Edge Cases Handled**:
- Binary/non-text files → skip with warning
- Frontmatter without title → fallback to filename
- Empty body → store empty string (still indexable by title)
- Unicode normalization → stored as-is; search uses same tokenizer

### 3.4 Search Algorithm

```
Input: query_string (Japanese or mixed)
1. Tokenize query with Vaporetto (same settings as indexer)
2. Build FTS5 AND query: "token1" AND "token2" (各トークンを "" で囲んで AND 結合)
   ※ スペース区切りはフレーズ検索になるため使わない
3. Execute: SELECT path, title, rank FROM notes_fts WHERE notes_fts MATCH ? ORDER BY rank
   ※ body のみでなく title + body 全カラムを対象にする
4. For top N results:
   a. Fetch full content from disk (lazy, on-demand)
   b. Extract 3-line snippet around first matched token
   c. Return path + snippet + score
```

**BM25 Scoring**: SQLite FTS5 built-in `bm25()` function. Tuned via `matchinfo` if needed.

**Snippet Extraction**:
- Locate first occurrence of any query token in original text
- Walk backward to previous newline (snippet start)
- Walk forward capturing up to ~3 logical lines (split by `\n\n` or heading markers)
- Truncate to ~500 chars max, append "…" if truncated
- Highlight matched tokens with `**` Markdown bold (optional flag)

---

## 4. Interface Layer Designs

### 4.1 CLI (`obsidian-shiotsuchi-vault-cli`)

**Command Structure**:

```bash
shiotsuchi [GLOBAL_OPTS] <COMMAND> [ARGS]

Commands:
  dive <query>          Search notes (航海:潜る="dive") [alias: search]
  chart                 Build/rebuild index (海図作成)
  scan                  Watch for changes (見張り)
  tide                  Show vault status (潮況) [alias: status]
  log                   Show statistics (航海日誌)

# NOTE: drift コマンドは廃止。MCP サーバは shiotsuchi-mcp として独立バイナリ。

Global Options:
  --notes-dir <PATH>    Root directory (required, default: $PWD)
  --db-path <PATH>      SQLite DB location (default: ~/.shiotsuchi/db.sqlite3)
  --verbose             Debug output
  --version             Show version and tagline
```

**Command Details**:

#### `dive <query>`
- **Action**: Search and print results as JSON array
- **Output format**: Pretty-printed JSON (デフォルト)。`--json` フラグで compact JSON（改行なし）
- **Search mode**: AND 検索デフォルト（全トークンがマッチするノートを返す）
- **Search target**: title + body 全カラム（タイトルのみのノートも拾える）
- **Example**:
  ```bash
  $ shiotsuchi dive "プロジェクト計画"
  [
    {
      "path": "projects/2024/plan.md",
      "title": "プロジェクト計画書",
      "snippet": "## プロジェクト計画\n\n**背景**: 市場の…",
      "score": 0.87
    },
    …
  ]

  # compact 出力（パイプ処理向け）
  $ shiotsuchi dive "プロジェクト計画" --json
  [{"path":"projects/2024/plan.md","title":"...","snippet":"...","score":0.87}]
  ```

#### `chart`
- **Action**: Walk directory, index all markdown files
- **Flags**:
  - `--force`: Re-hash all files regardless of mtime/hash
  - `--quiet`: Only show errors
- **Output**: Summary lines `Indexed 342 files (3 skipped, 2 errors)`

#### `tide`
- **Action**: Query DB stats (total files, last indexed, DB size)
- **Output**: Table format

#### `scan`
- **Action**: Start persistent file watcher (requires `notify` feature)
- **Behavior**: On file create/modify/delete, incrementally re-index
- **Flags**: `--debounce <ms>` (default: 500)

#### `log`
- **Action**: Show indexing history and search analytics (future)

---

### 4.2 Kilo Skill (`shiotsuchi-skill`)

**Skill Registration**:  
User adds to `~/.config/killo/agents/skills/shiotsuchi-search.md` or via `killo agent enable shiotsuchi-search`.

**Commands exposed**:

```yaml
commands:
  - name: search-vault
    description: Search your Markdown notes for relevant context
    params:
      - name: query
        type: string
        required: true
    handler: shiotsuchi-skill::search
  - name: read-note
    description: Read full content of a specific note
    params:
      - name: path
        type: string
        required: true
    handler: shiotsuchi-skill::read
  - name: vault-status
    description: Show vault indexing status
    handler: shiotsuchi-skill::status
```

**Integration**:  
Skill binary (`shiotsuchi-skill`) is a thin wrapper around core library, communicating via Kilo's JSON-RPC stdio protocol.

---

### 4.3 MCP Server (`shiotsuchi-mcp` 独立バイナリ)

**Binary**: `shiotsuchi-mcp`（`mcp/` クレート）。`shiotsuchi drift` コマンドは廃止。

**Claude Desktop 設定例**:
```json
{
  "mcpServers": {
    "shiotsuchi": {
      "command": "/usr/local/bin/shiotsuchi-mcp",
      "env": {
        "SHIOTSUCHI_NOTES_DIR": "/Users/name/Notes",
        "SHIOTSUCHI_DB_PATH": "/Users/name/.shiotsuchi/db.sqlite3"
      }
    }
  }
}
```

**Transport**: Standard Input/Output (stdio)

**Tools Defined**:

```json
{
  "tools": [
    {
      "name": "search_vault",
      "description": "Search the user's Markdown vault for notes matching a query. Returns paths, snippets, and relevance scores.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "query": { "type": "string", "description": "Japanese or English search query" }
        },
        "required": ["query"]
      }
    },
    {
      "name": "read_full_note",
      "description": "Read the complete Markdown content of a specific note by its relative path within the vault.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "path": { "type": "string", "description": "Relative path inside vault (e.g., 'projects/meeting.md')" }
        },
        "required": ["path"]
      }
    },
    {
      "name": "vault_status",
      "description": "Get vault indexing statistics: total notes, last updated, database size.",
      "inputSchema": { "type": "object", "properties": {} }
    }
  ]
}
```

**Security Policy**:
- `path` parameter: Must be a relative path (no `/` prefix, no `..` segments)
- Validate: All resolved paths must be inside configured `--notes-dir`
- Reject with error: Absolute paths, paths containing `..`, symlink escapes

**Message Flow**:
```
Claude → MCP request → shiotsuchi drift → core.search() → JSON response → Claude
```

---

## 5. Global Configuration & Environment

### 5.1 Configuration File

**Path**: `~/.shiotsuchi/config.toml` (or XDG compliant)

```toml
[vault]
notes_dir = "/Users/name/Documents/Notes"  # default vault root
db_path = "/Users/name/.local/share/shiotsuchi/db.sqlite3"

[indexing]
tokenizer = "vaporetto"          # or "simple" for testing
snippet_lines = 3                # configurable snippet length
include_extensions = ["md", "markdown"]
exclude_patterns = [".obsidian/", "node_modules/", ".git/"]

[watcher]
debounce_ms = 500
enabled = true

[mcp]
enabled = true
transport = "stdio"
```

CLI flags override config file values.

### 5.2 Environment Variables

- `SHIOTSUCHI_NOTES_DIR`: Override vault root
- `SHIOTSUCHI_DB_PATH`: Override database path
- `SHIOTSUCHI_CONFIG`: Custom config file location
- `SHIOTSUCHI_VERBOSE`: Set to `1` for debug logging

---

## 6. Error Handling & Resilience

### 6.1 Indexing Errors

| Error Type | Action |
|-----------|--------|
| File not found (deleted during scan) | Log warning, continue |
| Permission denied | Log error, skip file, continue |
| Non-UTF-8 content | Skip file, log warning with path |
| Corrupt frontmatter | Treat as no frontmatter, continue |
| DB write error | Abort index run, return error code 1 |

### 6.2 Search Errors

| Error | Response |
|-------|---------|
| DB not found | Return error: "Run `shiotsuchi chart` first" |
| Query tokenization fails | Return empty results + warning log |
| File deleted between index and read | Return error: "Note not found" |

### 6.3 MCP Protocol Errors

- Invalid JSON → error response, continue
- Missing required param → error with usage hint
- Unexpected panic → log to stderr, return generic error to client

---

## 7. Testing Strategy

### 7.1 Unit Tests (per crate)

**core**:
- `db.rs`: schema creation, hash comparison logic, upsert correctness
- `indexer.rs`: frontmatter extraction, tokenization pipeline, upsert flow
- `search.rs`: snippet extraction edge cases (start/end of file, multiple matches)
- `models.rs`: serde round-trips, path normalization

**cli**:
- Argument parsing (clap tests)
- Command dispatch (each subcommand)

**skill**:
- Skill command registration
- JSON-RPC message handling

### 7.2 Integration Tests

- Full indexing + search on fixture vault (sample ~50 notes)
- Concurrency: watcher + manual index race conditions
- MCP server: simulate Claude request/response cycle

### 7.3 Fixtures

`tests/fixtures/vault/` contains:
- Simple note
- Note with YAML frontmatter
- Japanese + English mixed
- Headings, code blocks, blockquotes
- Empty body
- Non-UTF-8 file (should skip)

---

## 8. Performance Targets

| Metric | Target |
|--------|--------|
| Indexing throughput | ≥ 100 files/sec (SSD) |
| Search latency (first page, 1000 notes) | ≤ 50ms |
| DB size overhead | ≤ 2× raw text size (tokenization expands) |
| Memory during indexing | ≤ 100MB (streaming, no full load) |
| Watcher latency | ≤ 1s from file save to indexed |

---

## 9. Implementation Phases

### Phase 1: Core Library
- [ ] Define `models.rs` (NoteMetadata, IndexResult, SearchResult)
- [ ] Implement `db.rs` (schema, upsert, hash tracking)
- [ ] Implement `indexer.rs` (file walk, Vaporetto, DB insert)
- [ ] Basic search in `search.rs` (BM25)
- [ ] Unit tests + integration test on fixture vault

### Phase 2: CLI
- [ ] Project setup (Cargo.toml, clap dependency)
- [ ] `main.rs` with subcommand structure
- [ ] Implement `chart` command (wrap core::index)
- [ ] Implement `dive` command (wrap core::search)
  - `--json` フラグ: compact JSON 出力
  - デフォルト: pretty-printed JSON
- [ ] Implement `tide` command (stats)
- [ ] config.toml 読み込み対応（`~/.shiotsuchi/config.toml`）
- [ ] Manual testing on sample vault

### Phase 3: Kilo Skill
- [ ] Skill manifest (`skill.yaml` or `skill.json`)
- [ ] Skill binary (`main.rs`) that loads config and runs commands
- [ ] JSON-RPC handler (KPeer protocol)
- [ ] Register as Kilo skill
- [ ] Test via `killo agent run shiotsuchi-search`

### Phase 4: MCP Server
- [ ] MCP protocol layer (stdio read/write, JSON-RPC 2.0)
- [ ] Tool definitions (`search_vault`, `read_full_note`, `vault_status`)
- [ ] Transport + tool dispatcher
- [ ] Test with `mcp` CLI or Claude Desktop

### Phase 5: Polishing
- [ ] Watcher (`scan` command) using `notify` crate
- [ ] Version command with tagline
- [ ] Error message UX improvements
- [ ] Benchmark suite (criterion)

---

## 10. Open Questions & Decisions

| Question | Decision | Rationale |
|----------|----------|-----------|
| Obsidian-specific features? | No (generic Markdown) | Maximizes reusability; Obsidian files are just Markdown |
| Frontmatter required? | Optional | Many Markdown files have none |
| Include attachments? | No (future) | Focus on text search; images/PDFs out of scope |
| Encrypted vaults? | Not supported | Shiotsuchi expects plaintext files |
| Windows support? | Yes (Rust cross-platform) | `notify` works on Windows/macOS/Linux |
| Concurrent indexing? | No (single-threaded) | SSD sequential read is fastest; simpler correctness |
| Search result limit | Configurable (default 20) | Prevent overwhelming output |

---

## 11. Success Criteria

- ✅ Index 10,000 notes in ≤ 2 minutes
- ✅ Search 3-character Japanese query returns in ≤ 100ms
- ✅ `shiotsuchi dive` outputs valid JSON
- ✅ Kilo skill loads and responds to `search-vault`
- ✅ Claude Desktop can call `search_vault` via MCP and get useful snippets
- ✅ Cross-platform: macOS, Linux, Windows

---

## 12. Future Enhancements (Post-V1)

- [ ] Tag-based filtering (`#tag` syntax)
- [ ] Fuzzy search (trigram, levenshtein)
- [ ] Graph relationship extraction (wikilinks)
- [ ] AI-powered query expansion
- [ ] Web UI for browsing
- [ ] Incremental indexing with `mtime`+`hash` (already planned)
- [ ] Export search results to various formats (CSV, Org, HTML)

---

## Appendix: Technology Choices

| Component | Crate | Why |
|-----------|-------|-----|
| Japanese Tokenizer | `vaporetto` | MeCab-compatible, fast, pre-trained model |
| SQLite | `rusqlite` (bundled) | Zero-config, FTS5 built-in |
| Markdown Parsing | `pulldown-cmark` | Fast, lossless, zero-copy where possible |
| CLI Parser | `clap` | De facto standard, derive macros |
| Config | `config` crate | TOML support, env override |
| File Watching | `notify` | Cross-platform, debounce built-in |
| Serialization | `serde` + `serde_json` | Standard in Rust ecosystem |
| MCP | Custom (JSON-RPC over stdio) | No existing crate; simple to implement |

---

**Document Version**: 1.0-draft  
**Next Review**: After Phase 1 core implementation
