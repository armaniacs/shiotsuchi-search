# Shiotsuchi-Search

> *Guiding your path through the data tide.*

High-performance Japanese-aware search engine for Markdown note vaults (Obsidian, etc.).
Powered by [Vaporetto](https://github.com/daac-tools/vaporetto) × SQLite FTS5.

## Features

- **Sub-second search** across 10,000+ notes
- **Japanese-aware tokenization** via Vaporetto
- **Multiple interfaces**: CLI, Kilo Skill, Claude Desktop (MCP)
- **Incremental indexing**: only re-indexes changed files (SHA-256 hash tracking)

## Quick Start

### 1. Download tokenizer model

```bash
./scripts/download-model.sh
```

### 2. Index your vault

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  shiotsuchi chart --notes-dir ~/Notes
```

### 3. Search

```bash
shiotsuchi dive "プロジェクト計画"
```

## Commands

| Command | Description |
|---------|-------------|
| `chart` | Index/re-index all Markdown files |
| `dive <query>` | Search notes (AND search, JSON output) |
| `tide` | Show vault statistics |
| `scan` | Watch for file changes and auto-re-index |
| `log` | Show indexing history |

## Claude Desktop Integration (MCP)

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

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

Then index your vault first:

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  shiotsuchi chart --notes-dir ~/Notes --db-path ~/.shiotsuchi/db.sqlite3
```

Restart Claude Desktop and ask: "Search my notes for プロジェクト"

## Configuration

`~/.shiotsuchi/config.toml`:

```toml
[vault]
notes_dir = "/Users/name/Notes"
db_path = "/Users/name/.shiotsuchi/db.sqlite3"

[indexing]
snippet_lines = 3
include_extensions = ["md", "markdown"]
exclude_patterns = [".obsidian", ".git", "node_modules"]
```

## Building from Source

```bash
git clone https://github.com/your-org/shiotsuchi-search
cd shiotsuchi-search
./scripts/download-model.sh
SHIOTSUCHI_EMBED_MODEL=$(pwd)/models/bccwj-suw+unidic_pos+kana.model.zst \
  cargo build --release
```

Binaries are placed in `target/release/`:
- `shiotsuchi` — CLI
- `shiotsuchi-skill` — Kilo Skill server
- `shiotsuchi-mcp` — Claude Desktop MCP server

## Running Tests

```bash
./scripts/download-model.sh
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  cargo test --workspace
```

## Performance

| Metric | Target | Notes |
|--------|--------|-------|
| Indexing | ≥ 100 files/sec | SSD |
| Search (1,000 notes) | ≤ 50ms | AND query |
| Memory during indexing | ≤ 100MB | Streaming |

Run benchmarks:

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  cargo bench -p obsidian-shiotsuchi-vault-core
```

## License

MIT
