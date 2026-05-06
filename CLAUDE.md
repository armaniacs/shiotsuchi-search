# CLAUDE.md — Shiotsuchi Search

Quick reference for working with this codebase.

## Project

High-performance Japanese-aware search engine for Markdown note vaults, powered by Vaporetto × SQLite FTS5.

## Reference Documentation

| Document | Purpose |
|----------|---------|
| [ref/architecture.md](ref/architecture.md) | Workspace structure, data flow, design decisions |
| [ref/core.md](ref/core.md) | Core library: DB, tokenizer, indexer, search, watcher |
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
| [mcp/src/main.rs](mcp/src/main.rs) | MCP server stdio loop |
| [mcp/src/handler.rs](mcp/src/handler.rs) | MCP tool handlers |

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
  shiotsuchi chart --notes-dir ~/Notes

# Search
shiotsuchi dive "project plan"

# Watch
shiotsuchi scan --notes-dir ~/Notes
```

## Important Context

- Uses **Vaporetto** for Japanese tokenization (not SQLite extension)
- Tokenized body stored as space-separated tokens in FTS5 `body` column
- Two-table design: `notes_fts` (FTS5 virtual) + `notes_meta` (metadata)
- SHA-256 hash tracking for incremental indexing
- WAL mode enabled for concurrent CLI + MCP access
- Transactions wrap FTS + meta updates
- Path traversal protection on both search snippets and MCP `read_full_note`
- Model embedding at compile time via `core/build.rs` and `SHIOTSUCHI_MODEL_PATH`
