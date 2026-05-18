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
shiotsuchi chart --notes-dir ~/Notes
```

Restart Claude Desktop and ask: "Search my notes for project"

## Security & Privacy

- The database (`db.sqlite3`) stores **plaintext** of your note bodies (tokenized for search). If your vault contains sensitive data, ensure appropriate file permissions (e.g., `chmod 600`) on the database file.
- The MCP server exposes read-only access to your vault. Only connect to trusted MCP clients.

## Configuration

`~/.config/shiotsuchi/config.toml` (`$XDG_CONFIG_HOME/shiotsuchi/config.toml` if set):

```toml
[database]
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[vaults.default]
notes_dir = "/home/name/Notes"

[indexing]
snippet_lines = 3
max_snippet_chars = 1000
include_extensions = ["md", "markdown"]
exclude_dirs = ["node_modules"]
```

Multiple vaults can share a single database:

```toml
[database]
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[vaults.personal]
notes_dir = "/home/name/Documents/Personal"

[vaults.work]
notes_dir = "/home/name/Documents/Work"
```

> **Legacy format:** Pre-v0.3.7 configs use `[vault] notes_dir` / `[vault] db_path` and are still readable.
> Run `shiotsuchi config-migrate` to upgrade to the new format.

## Performance

| Metric | Target | Notes |
|--------|--------|-------|
| Indexing | ≥ 100 files/sec | SSD |
| Search (1,000 notes) | ≤ 50ms | AND query |
| Memory during indexing | ≤ 100MB | Streaming |

Run benchmarks:

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  cargo bench -p shiotsuchi-core
```

## License

Apache License 2.0

The release binaries embed the Vaporetto model `bccwj-suw+unidic_pos+kana.model.zst`,
which is licensed under BSD-3-Clause. See [docs/MODEL_LICENSES.md](docs/MODEL_LICENSES.md) for details.
