# Shiotsuchi Search — Architecture Overview

## Project Description

High-performance Japanese-aware search engine for Markdown note vaults (Obsidian, etc.).
Powered by Vaporetto × SQLite FTS5, with optional vector (semantic) search via ONNX + sqlite-vec.

## Workspace Structure

```
shiotsuchi-search/
├── Cargo.toml          # Workspace definition
├── core/               # Core library (shiotsuchi-core)
│   ├── src/
│   │   ├── lib.rs      # Module exports
│   │   ├── db.rs       # SQLite + FTS5 + vec0 database operations
│   │   ├── tokenizer.rs # Japanese tokenizer (Vaporetto)
│   │   ├── chunker.rs  # Markdown → chunks splitter (RAG)
│   │   ├── embedder.rs # ONNX embedding inference (RAG)
│   │   ├── indexer.rs  # File walking + indexing (chunk-aware)
│   │   ├── search.rs   # Search (FTS / Vec / Hybrid) + snippet extraction
│   │   ├── models.rs   # Data structures (Chunk, SearchMode, VaultStats, etc.)
│   │   ├── watcher.rs  # File change watcher
│   │   ├── build_info.rs # Compile-time constants (embedded hash, features)
│   │   ├── paths.rs    # XDG path resolution
│   │   └── constants.rs # Build-time constants (embedded model hash)
│   ├── benches/        # Criterion benchmarks
│   └── build.rs        # Model embedding at compile time
├── cli/                # CLI binary (shiotsuchi)
│   └── src/
│       ├── main.rs     # Entry point (clap)
│       ├── config.rs   # Config file loading
│       ├── build_info.rs # Dynamic version/build info
│       ├── util.rs     # Shared CLI utilities
│       └── commands/   # Subcommand implementations
├── mcp/                # MCP server binary (shiotsuchi-mcp)
│   └── src/
│       ├── main.rs     # JSON-RPC stdio loop, tokio runtime
│       ├── protocol.rs # MCP request/response types
│       ├── tools.rs    # Tool definitions (JSON Schema)
│       └── handler.rs  # Tool call handlers
└── integration/        # TypeScript integration tests (Vitest)
```

## Key Design Decisions

1. **Rust tokenizer instead of FTS5 extension**: Vaporetto runs in-process rather than as a SQLite loadable extension. This avoids platform-dependent `.so`/`.dylib` distribution issues.
2. **Tokenized body stored in FTS5**: `tokenizer.split()` produces space-separated tokens stored in the `tokenized_content` column. FTS5 `unicode61` tokenizer then treats each as a word.
3. **SHA-256 hash tracking**: Only re-indexes files whose content changed. Uses `file_cache` table.
4. **Chunk-based schema**: Files are split into chunks (by headers/paragraphs) for RAG retrieval. Each chunk has its own FTS5 entry and optional vector embedding.
5. **Dual retrieval**: FTS5 BM25 for keyword search + sqlite-vec `vec0` for semantic search, combinable via Hybrid RRF.
6. **WAL mode**: Enabled on database open for concurrent read/write between CLI and MCP server.
7. **tokio runtime in MCP**: Enables async MCP progress notifications during background `rebuild_index`.
8. **Vaporetto model embedding at build time**: Tokenizer model can be embedded via `build.rs` for zero-runtime-dependency deployment.
9. **Multi-vault support**: Single SQLite database can serve multiple notes directories. Each chunk and file_cache entry carries a `vault_name` column to distinguish origins. Config uses `[vaults.xxx]` sections (see config format below).
10. **Crash-safe migration**: Schema upgrades (v2→v3) are wrapped in transactions with pre-checks to handle mid-migration crashes.

## Data Flow

```
Markdown files
    │
    ▼
index_directory() / index_file()
    │
    ├── extract_frontmatter() ──► (title, body)
    │
    ├── split_into_chunks()
    │   ├── Level 1: split on headers (#/##/###)
    │   └── Level 2: split long sections on blank lines
    │
    ├── JapaneseTokenizer.split() per chunk
    │       │
    │       ▼
    ├── db.insert_chunks()
    │   ├── chunks table (id, file_path, chunk_index, content, tokenized_content)
    │   ├── fts_chunks (FTS5 virtual table over tokenized_content)
    │   └── file_cache (hash/mtime tracking)
    │
    └── Embedder (optional)
        └── db.insert_embeddings()
            └── vec_chunks (vec0 virtual table for KNN search)

Search flow:
    query
     │
     ▼
    search(db, tokenizer, query, limit, mode, embedder?, min_score?)
     │
     ├── Mode::Fts → search_fts() → fts_chunks MATCH (BM25 ranking)
     ├── Mode::Vec → search_vec() → embed → vec_chunks KNN (cosine distance)
     └── Mode::Hybrid → search_hybrid() → RRF merge of FTS + Vec results
     │
     ▼
    extract_snippet() ──► ChunkSearchResult[]
     │
     ▼
    Structured Markdown output with context delimiters
```

## Entry Points

| Binary | File | Purpose |
|--------|------|---------|
| `shiotsuchi` | `cli/src/main.rs` | CLI tool (chart, clean, config-migrate, dive, tide, scan, log, init, setup, dredge, delete) |
| `shiotsuchi-mcp` | `mcp/src/main.rs` | MCP server for Claude Desktop (tokio async) |

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
| Config file | `vaults.*.notes_dir` | `~/.config/shiotsuchi/config.toml` → `[vaults.default] notes_dir` |
| Config file | `database.db_path` | `~/.config/shiotsuchi/config.toml` → `[database] db_path` |
| Config file | `embedder.provider` + `embedder.path` | `[embedder] provider = "onnx-file"` / `path = "/path/to/model.onnx"` |
| Config file (legacy) | `vault.notes_dir` / `vault.db_path` | Pre-v0.3.7 format, auto-detected with migration hint |
| Env var | `SHIOTSUCHI_MODEL_PATH` | `models/bccwj-suw+unidic_pos+kana.model.zst` |
| Env var | `SHIOTSUCHI_EMBED_MODEL_PATH` | `/path/to/model.onnx` (runtime model resolution) |
| Env var | `SHIOTSUCHI_NOTES_DIR` | `/Users/name/Notes` (overrides first vault's notes_dir) |
| Env var | `SHIOTSUCHI_DB_PATH` | `~/.cache/shiotsuchi/db.sqlite3` |

## Feature Flags (core)

| Feature | Default | Description |
|---------|---------|-------------|
| `watcher` | yes | File system watcher via `notify` crate |
| `async-index` | yes | Parallel indexing via `rayon` |
