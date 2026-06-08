# CLAUDE.md — Shiotsuchi Search

Think in English, interact with the user in Japanese.
This file is the quick reference for working with this codebase.


## Project

High-performance Japanese-aware search engine for Markdown note vaults, powered by Vaporetto × SQLite FTS5.

## Reference Documentation

| Document | Purpose |
|----------|---------|
| [ref/architecture.md](ref/architecture.md) | Workspace structure, data flow, design decisions |
| [ref/core.md](ref/core.md) | Core library: DB, tokenizer, indexer, search, watcher, sensitive masking, rate limiter |
| [ref/cli.md](ref/cli.md) | CLI commands, config, entry points |
| [ref/mcp.md](ref/mcp.md) | MCP server protocol, tools, Claude Desktop setup |
| [ref/models.md](ref/models.md) | Data models, FTS5 query format, file hash |

## Key Files

| File | What it does |
|------|-------------|
| [Cargo.toml](Cargo.toml) | Workspace definition |
| [core/src/lib.rs](core/src/lib.rs) | Core crate exports |
| [core/src/db.rs](core/src/db.rs) | SQLite + FTS5 operations |
| [core/src/tokenizer.rs](core/src/tokenizer.rs) | Vaporetto Japanese tokenizer |
| [core/src/indexer.rs](core/src/indexer.rs) | File walking + indexing |
| [core/src/search.rs](core/src/search.rs) | Search + snippet extraction |
| [cli/src/main.rs](cli/src/main.rs) | CLI entry point |
| [cli/src/config.rs](cli/src/config.rs) | Config loading (XDG dirs) |
| [core/src/server/handlers.rs](core/src/server/handlers.rs) | HTTP API server handlers |
| [core/src/server/ui.html](core/src/server/ui.html) | Browser-based search UI |
| [mcp/src/main.rs](mcp/src/main.rs) | MCP server stdio loop |
| [mcp/src/handler/mod.rs](mcp/src/handler/mod.rs) | MCP tool handlers |

## Quick Commands

```bash
# Build
make build

# Test (all workspace)
make test

# Test core only
cargo test -p shiotsuchi-core

# Benchmark
cargo bench -p shiotsuchi-core

# Index vault
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  shiotsuchi index --notes-dir ~/Notes

# Search
shiotsuchi search "project plan"

# Watch
shiotsuchi watch --notes-dir ~/Notes

# HTTP server
shiotsuchi serve --port 7171
```

## Linear CLI

Linear CLI は **常に `npx` を使って呼び出すこと**。

```bash
# 正しい使い方
npx @schpet/linear-cli issue list
npx @schpet/linear-cli issue create --title "..." --team DEV

# 間違い（使わないこと）
linear issue list
```

グローバルインストールしない。`npx` 経由で必ず呼ぶ。

## Important Context

- Uses **Vaporetto** for Japanese tokenization (not SQLite extension)
- Tokenized body stored as space-separated tokens in FTS5 `tokenized_content` column
- Chunk-based schema: `chunks` + `fts_chunks` (FTS5) + `vec_chunks` (vec0) + `file_cache` + `tasks` + `note_links` + `tag_counts`
- SHA-256 hash tracking for incremental indexing (`file_cache` table)
- WAL mode allows concurrent CLI + MCP + HTTP server access
- `NoteDatabase` has dual connections: `write_conn` (indexing) + `read_conn` (search, SQLITE_OPEN_READ_ONLY). 22 read methods use `get_read_conn()`.
- `ReadOnlyDb` type: lightweight read-only wrapper opened per-request in HTTP server (no Mutex serialization). Each handler creates its own connection via `ReadOnlyDb::open()`.
- `search()` and `build_results()` in `search.rs` take `&Connection` (not `&NoteDatabase`), enabling use by both `NoteDatabase` and `ReadOnlyDb`.
- Transactions wrap FTS + vec + meta updates (`reindex_file` is fully atomic)
- Path traversal protection on search snippets and MCP/HTTP read endpoints
- HTTP server (`shiotsuchi serve`) provides REST API + browser UI at `/ui` (rate-limited 30 req/s). **No shared `Mutex`** — handlers open `ReadOnlyDb` per request.
- Sensitive data masking on MCP and HTTP API outputs (enabled by default via `sensitive.rs`)
- Model embedding at compile time via `core/build.rs` and `SHIOTSUCHI_MODEL_PATH`
