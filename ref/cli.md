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
| `init` | `[--notes-dir]` `[--db-path]` `[--force]` `[--yes]` | Create config file with interactive exclusion selection |
| `config detect-noise` | `[--notes-dir]` | Scan vault for exclusion candidates (read-only) |

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
exclude_dirs = ["node_modules"]
auto_exclude_hidden = true
follow_links = false
dynamic_threshold = 5

[watcher]
enabled = true
```

## Configuration Fields

### `[indexing]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `snippet_lines` | integer | 3 | Context lines to show around each search match |
| `include_extensions` | string array | `["md", "markdown"]` | File extensions to include when indexing |
| `exclude_dirs` | string array | `["node_modules"]` | Directory names to exclude (gitignore-style component matching). Renamed from `exclude_patterns` in v0.2.9. |
| `auto_exclude_hidden` | bool | `true` | Skip directories starting with `.` (`.git`, `.obsidian`, etc.) |
| `follow_links` | bool | `false` | Follow symbolic links when walking the vault (with vault boundary protection) |
| `dynamic_threshold` | integer | 5 | Minimum number of matching files for a directory to be dynamically flagged as noise during `init` scan |

### `[watcher]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Enable the file watcher |

## Config Migration (v0.2.9)

In v0.2.9, the `exclude_patterns` field was renamed to `exclude_dirs` to accurately reflect that it matches directory names (not arbitrary file patterns). If your existing config uses `exclude_patterns`, you will see a deserialization error with a message like:

```
unknown field `exclude_patterns`, expected one of `snippet_lines`, `include_extensions`, `exclude_dirs`, ...
```

**Fix:** Rename the key to `exclude_dirs` in your config file:

```toml
# Before (v0.2.8 and earlier)
exclude_patterns = ["node_modules", "templates"]

# After (v0.2.9+)
exclude_dirs = ["node_modules", "templates"]
```

## Implementation Files

- `cli/src/main.rs` — Entry point, argument parsing with `clap`
- `cli/src/config.rs` — Config types (`ShiotsuchiConfig`, `VaultConfig`, `IndexingConfig`, `WatcherConfig`)
- `cli/src/commands/chart.rs` — Full vault indexing
- `cli/src/commands/dive.rs` — Search with JSON output
- `cli/src/commands/tide.rs` — Statistics display
- `cli/src/commands/scan.rs` — File watcher setup
- `cli/src/commands/log.rs` — Metadata listing
- `cli/src/commands/init.rs` — Config file creation with interactive exclusion selection
- `cli/src/commands/noise.rs` — Vault scanning logic for exclusion candidate detection
- `cli/src/commands/config.rs` — Config subcommands (`detect-noise`)

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
| `chart` | Human-readable progress (indexed/skipped/errors, invalid patterns if any) |
| `dive` | Pretty JSON (or raw JSON with `--json`) |
| `tide` | Human-readable statistics |
| `scan` | Watcher logs |
| `log` | Table with columns |
| `init` | Human-readable config creation summary |
| `config detect-noise` | Human-readable exclusion candidate list |
