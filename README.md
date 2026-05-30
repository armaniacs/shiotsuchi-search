# Shiotsuchi-Search

[Japanese](README.ja.md)

> *Guiding your path through the data tide.*

High-performance Japanese-aware search engine for Markdown note vaults (Obsidian, etc.).
Powered by [Vaporetto](https://github.com/daac-tools/vaporetto) × SQLite FTS5.

> **Note:** This search engine is optimized for Japanese text. Search quality for other languages is not guaranteed.

## Features

- **Sub-second search** across 10,000+ notes
- **Japanese-aware tokenization** via Vaporetto
- **Multiple interfaces**: CLI, MCP (Claude Desktop)
- **Incremental indexing**: only re-indexes changed files (SHA-256 hash tracking)

> **Note:** CLI output and help text are in Japanese by default. English usage docs are available in [docs/CLI-USE.md](docs/CLI-USE.md).

## Commands

| Command | Description |
|---------|-------------|
| `index` / `chart` | Index/re-index all Markdown files |
| `check-ignore <path>` | Check if a path matches exclude rules |
| `clean` | Backup database, delete, and re-index from scratch |
| `config` | Manage indexing settings (detect-noise) |
| `config-migrate` | Upgrade config from legacy `[vault]` to new format |
| `delete <path>` | Remove a note from the index (does not delete the file) |
| `search <query>` / `dive <query>` | Search notes (fts/vec/hybrid modes, filters, MMR) |
| `doctor` | Environment health check with interactive repair |
| `prune` / `dredge` | Chunk migration for pre-v0.3.3 vaults |
| `init` | Create config file with interactive exclusion selection |
| `list` / `log` | Show indexing history |
| `watch` / `scan` | Watch for file changes and auto-re-index |
| `setup` | Download/check ONNX embedding model |
| `synonym` | Manage thesaurus entries (add/remove/list) |
| `tasks` | Cross-vault task checkbox search |
| `stats` / `tide` | Show vault statistics (files, chunks, tags, --json) |
| `support` | Show build info and dependency versions |

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
shiotsuchi index --notes-dir ~/Notes
```

Restart Claude Desktop and ask: "Search my notes for project"

## Security & Privacy

- The database (`db.sqlite3`) stores **plaintext** of your note bodies (tokenized for search). If your vault contains sensitive data, ensure appropriate file permissions (e.g., `chmod 600`) on the database file.
- The MCP server exposes read-only access to your vault. Only connect to trusted MCP clients.

## Configuration

`~/.config/shiotsuchi/config.toml` (`$XDG_CONFIG_HOME/shiotsuchi/config.toml` if set):

```toml
[database]
db_path = "~/.cache/shiotsuchi/db.sqlite3"
vault_default = "personal"

[vaults.personal]
notes_dir = "/Users/name/Documents/Personal"

[vaults.work]
notes_dir = "/Users/name/Documents/Work"

[indexing]
snippet_lines       = 3
max_snippet_chars   = 1000
include_extensions  = ["md", "markdown"]
exclude_dirs        = ["node_modules"]
user_dictionary     = ["Vaporetto", "shiotsuchi"]  # custom tokenization words
hybrid_alpha        = 0.5                           # FTS ↔ vec blend ratio
semantic_threshold  = 0.75                          # minimum score threshold
```

Multiple vaults can share a single database. Use `--vault <name>` to restrict operations to one vault. Set `vault_default = "..."` to always use a specific vault when `--vault` is omitted.

## Building from Source

See [docs/INSTALL.md](docs/INSTALL.md) for build options including lightweight builds (`cargo build --no-default-features`).

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

## Further reading

- [docs/INSTALL.md](docs/INSTALL.md) — Install via `cargo install` or build from source
- [docs/CLI-USE.md](docs/CLI-USE.md) — Detailed CLI command reference
- [docs/MCP-SETUP.md](docs/MCP-SETUP.md) — Multi-vault MCP setup guide
- [docs/FTS5.md](docs/FTS5.md) — FTS5 query syntax and tips
- [CHANGELOG.md](CHANGELOG.md) — Release history
- [ref/architecture.md](ref/architecture.md) — Design and data model

## License

Apache License 2.0

The release binaries embed the Vaporetto model `bccwj-suw+unidic_pos+kana.model.zst`,
which is licensed under BSD-3-Clause. See [docs/MODEL_LICENSES.md](docs/MODEL_LICENSES.md) for details.
