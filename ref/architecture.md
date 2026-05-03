# Shiotsuchi Search — Architecture Overview

## Project Description

High-performance Japanese-aware search engine for Markdown note vaults (Obsidian, etc.).
Powered by Vaporetto × SQLite FTS5.

## Workspace Structure

```
shiotsuchi-search/
├── Cargo.toml          # Workspace definition
├── core/               # Core library (obsidian-shiotsuchi-vault-core)
│   ├── src/
│   │   ├── lib.rs      # Module exports
│   │   ├── db.rs       # SQLite + FTS5 database operations
│   │   ├── tokenizer.rs # Japanese tokenizer (Vaporetto)
│   │   ├── indexer.rs  # File walking + indexing
│   │   ├── search.rs   # Search + snippet extraction
│   │   ├── models.rs   # Data structures
│   │   └── watcher.rs  # File change watcher
│   └── build.rs        # Model embedding at compile time
├── cli/                # CLI binary (shiotsuchi)
│   └── src/
│       ├── main.rs     # Entry point
│       ├── config.rs   # Config file loading
│       └── commands/   # Subcommand implementations
├── mcp/                # MCP server binary (shiotsuchi-mcp)
│   └── src/
│       ├── main.rs     # JSON-RPC stdio loop
│       ├── protocol.rs # MCP request/response types
│       ├── tools.rs    # Tool definitions
│       └── handler.rs  # Tool call handlers
└── integration/        # TypeScript integration tests (Vitest)
```

## Key Design Decisions

1. **Rust tokenizer instead of FTS5 extension**: Vaporetto runs in-process rather than as a SQLite loadable extension. This avoids platform-dependent `.so`/`.dylib` distribution issues.
2. **Tokenized body stored in FTS5**: `tokenizer.split()` produces space-separated tokens stored in the `body` column. FTS5 `unicode61` tokenizer then treats each as a word.
3. **SHA-256 hash tracking**: Only re-indexes files whose content changed.
4. **Two-table design**: `notes_fts` (FTS5 virtual table for search) + `notes_meta` (ordinary table for metadata/hash tracking).
5. **WAL mode**: Enabled on database open for concurrent read/write between CLI and MCP server.
6. **Transaction safety**: `upsert_note` and `delete_note` wrap FTS + meta operations in transactions.

## Data Flow

```
Markdown files
    │
    ▼
index_file() ──► extract_frontmatter() ──► markdown_to_text()
                                              │
                                              ▼
                                   JapaneseTokenizer.split()
                                              │
                                              ▼
                              db.upsert_note() ──► notes_fts + notes_meta
                                              │
                                              ▼
                                   db.search() ──► extract_snippet()
```

## Entry Points

| Binary | File | Purpose |
|--------|------|---------|
| `shiotsuchi` | `cli/src/main.rs` | CLI tool (chart, dive, tide, scan, log) |
| `shiotsuchi-mcp` | `mcp/src/main.rs` | MCP server for Claude Desktop |

## Crate Dependencies

```
cli ──► core
mcp ──► core
```

- `core` has no reverse dependencies on `cli` or `mcp`
- `cli` has a dev-dependency on `mcp` for E2E tests (noted as architectural concern)

## Configuration Sources (precedence: CLI args > env vars > config file > defaults)

| Source | Key | Example |
|--------|-----|---------|
| Config file | `vault.notes_dir` | `~/.config/shiotsuchi/config.toml` |
| Config file | `vault.db_path` | `~/.cache/shiotsuchi/db.sqlite3` |
| Env var | `SHIOTSUCHI_MODEL_PATH` | `models/bccwj-suw+unidic_pos+kana.model.zst` |
| Env var | `SHIOTSUCHI_NOTES_DIR` | `/Users/name/Notes` |
| Env var | `SHIOTSUCHI_DB_PATH` | `~/.cache/shiotsuchi/db.sqlite3` |
