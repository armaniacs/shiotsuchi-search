# Shiotsuchi-Search

[Japanese](README.ja.md)

> *Guiding your path through the data tide.*

High-performance Japanese-aware search engine for Markdown note vaults (Obsidian, etc.).
Powered by [Vaporetto](https://github.com/daac-tools/vaporetto) × SQLite FTS5.

## Features

- **Sub-second search** across 10,000+ notes
- **Japanese-aware tokenization** via Vaporetto
- **Multiple interfaces**: CLI, MCP (Claude Desktop)
- **Incremental indexing**: only re-indexes changed files (SHA-256 hash tracking)

> **Note:** All command output and error messages are currently in English only. Japanese localization may be added in a future release.

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

DB is stored at `~/.cache/shiotsuchi/db.sqlite3` by default (`$XDG_CACHE_HOME/shiotsuchi/db.sqlite3` if set).

### 3. Search

```bash
shiotsuchi dive "project plan"
```

## Commands

| Command | Description |
|---------|-------------|
| `chart` | Index/re-index all Markdown files |
| `dive <query>` | Search notes (AND search, JSON output) |
| `tide` | Show vault statistics |
| `scan` | Watch for file changes and auto-re-index |
| `log` | Show indexing history |
| `delete <path>` | Remove a note from the index (does not delete the file) |

## Claude Desktop Integration (MCP)

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "shiotsuchi": {
      "command": "/usr/local/bin/shiotsuchi-mcp",
      "env": {
        "SHIOTSUCHI_NOTES_DIR": "/Users/name/Notes",
        "SHIOTSUCHI_DB_PATH": "/home/name/.cache/shiotsuchi/db.sqlite3"
      }
    }
  }
}
```

Then index your vault first:

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  shiotsuchi chart --notes-dir ~/Notes
```

Restart Claude Desktop and ask: "Search my notes for project"

## Security & Privacy

- The database (`db.sqlite3`) stores **plaintext** of your note bodies (tokenized for search). If your vault contains sensitive data, ensure appropriate file permissions (e.g., `chmod 600`) on the database file.
- The MCP server exposes read-only access to your vault. Only connect to trusted MCP clients.

## Configuration

`~/.config/shiotsuchi/config.toml` (`$XDG_CONFIG_HOME/shiotsuchi/config.toml` if set):

```toml
[vault]
notes_dir = "/home/name/Notes"
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[indexing]
snippet_lines = 3
include_extensions = ["md", "markdown"]
exclude_patterns = [".obsidian", ".git", "node_modules"]
```

## Building from Source

```bash
git clone https://github.com/your-org/shiotsuchi-search
cd shiotsuchi-search
make build
```

`make build` downloads the tokenizer model automatically if needed, then builds release binaries with the model embedded.

Binaries are placed in `target/release/`:
- `shiotsuchi` — CLI
- `shiotsuchi-mcp` — Claude Desktop MCP server

### Install

```bash
make install                        # installs to /usr/local/bin
make install PREFIX=~/.local        # installs to ~/.local/bin
```

### Common make targets

| Target | Description |
|--------|-------------|
| `make build` | Build release binaries (embeds tokenizer model) |
| `make test` | Run all workspace tests |
| `make bench` | Run criterion benchmarks |
| `make install` | Install binaries to `$(PREFIX)/bin` |
| `make uninstall` | Remove installed binaries |
| `make clean` | Remove build artifacts |

## Running Tests

```bash
make test
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

Apache License 2.0

The release binaries embed the Vaporetto model `bccwj-suw+unidic_pos+kana.model.zst`,
which is licensed under BSD-3-Clause. See [MODEL_LICENSES.md](MODEL_LICENSES.md) for details.
