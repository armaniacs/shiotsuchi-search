# CLI (shiotsuchi)

Binary name: `shiotsuchi`
Crate path: `cli/`

## Commands

| Command | Args | Description |
|---------|------|-------------|
| `chart` | `[--notes-dir]` `[--db-path]` | Index/re-index all Markdown files |
| `dive <query>` | `[--notes-dir]` `[--db-path]` `[--limit]` `[--json]` | Search notes (AND search) |
| `tide` | `[--db-path]` | Show vault statistics |
| `scan` | `[--notes-dir]` `[--db-path]` | Watch for file changes and auto-re-index |
| `log` | `[--db-path]` | Show indexing history |

## Global Options

All commands accept (via CLI flag or environment variable):
- `--notes-dir` / `SHIOTSUCHI_NOTES_DIR` — Vault root directory
- `--db-path` / `SHIOTSUCHI_DB_PATH` — SQLite database path
- `--verbose` — Enable logging

## Configuration File

Path: `~/.config/shiotsuchi/config.toml` (or `$XDG_CONFIG_HOME/shiotsuchi/config.toml`)

```toml
[vault]
notes_dir = "/home/name/Notes"
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[indexing]
snippet_lines = 3
include_extensions = ["md", "markdown"]
exclude_patterns = [".obsidian", ".git", "node_modules"]

[watcher]
debounce_ms = 500
enabled = true
```

## Implementation Files

- `cli/src/main.rs` — Entry point, argument parsing with `clap`
- `cli/src/config.rs` — Config types (`ShiotsuchiConfig`, `VaultConfig`, `IndexingConfig`, `WatcherConfig`)
- `cli/src/commands/chart.rs` — Full vault indexing
- `cli/src/commands/dive.rs` — Search with JSON output
- `cli/src/commands/tide.rs` — Statistics display
- `cli/src/commands/scan.rs` — File watcher setup
- `cli/src/commands/log.rs` — Metadata listing

## DB Path Resolution

Default DB path: `~/.cache/shiotsuchi/db.sqlite3`
Resolution order:
1. `XDG_CACHE_HOME/shiotsuchi/db.sqlite3` (if env var set)
2. `~/.cache/shiotsuchi/db.sqlite3` (fallback)
3. Current directory `./.cache/shiotsuchi/db.sqlite3` (if home dir unavailable)

## Error Handling

- `main()` returns `Result<(), Box<dyn std::error::Error>>`
- `dive` checks `db_path.exists()` before opening and shows a helpful message if not found
- Config parse errors are logged as warnings (not silently ignored)

## Outputs

| Command | Output Format |
|---------|--------------|
| `chart` | Human-readable progress |
| `dive` | Pretty JSON (or raw JSON with `--json`) |
| `tide` | Human-readable statistics |
| `scan` | Watcher logs |
| `log` | Table with columns |
