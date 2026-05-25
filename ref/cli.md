# CLI (shiotsuchi)

Binary name: `shiotsuchi`
Crate path: `cli/`

## Commands

| Command | Args | Description |
|---------|------|-------------|
| `chart` | `[--notes-dir]` `[--db-path]` | Index/re-index all Markdown files in all configured vaults (chunk-based) |
| `clean` | `[--db-path]` | Backup the database, delete it, then re-index all vaults from scratch |
| `config detect-noise` | `[--notes-dir]` | Scan vault for exclusion candidates (read-only) |
| `config-migrate` | `[--config]` | Migrate config from old `[vault]` format to new `[database]` + `[vaults.xxx]` format |
| `delete <path>` | `[--notes-dir]` `[--db-path]` | Remove a note from the index by its vault-relative path |
| `dive <query>` | `[--notes-dir]` `[--db-path]` `[--limit]` `[--mode]` `[--json]` `[--json-pretty]` | Search notes. `--mode`: `keyword` (default), `semantic`, `hybrid`. |
| `doctor` | (no args) | Environment health check with interactive repair for config, database, tokenizer, embedder, and vault directories |
| `dredge` | `[--notes-dir]` `[--db-path]` | Extract and index chunks from existing notes without re-embedding content. Migrates pre-v0.3.3 vaults to chunked schema. |
| `init` | `[--notes-dir]` `[--db-path]` `[--force]` `[--yes]` | Create config file with interactive exclusion selection |
| `log` | `[--db-path]` | Show indexing history |
| `scan` | `[--notes-dir]` `[--db-path]` | Watch all configured vaults for file changes and auto-re-index |
| `setup` | `[--check]` `[--model-path]` | Setup/check ONNX embedding model and Vaporetto tokenizer. `--check` verifies model availability and hash. |
| `support` | (no subcommands) | Display build info, dependency versions, and system information |
| `tide` | `[--db-path]` | Show vault statistics (chunks, files, vec index status) |

## Global Options

All commands accept (via CLI flag or environment variable):
- `--notes-dir` / `SHIOTSUCHI_NOTES_DIR` — Vault root directory
- `--db-path` / `SHIOTSUCHI_DB_PATH` — SQLite database path
- `--verbose` — Enable logging

## Configuration File

Path: `~/.config/shiotsuchi/config.toml` (or `$XDG_CONFIG_HOME/shiotsuchi/config.toml`)

### New format (v0.3.7+)

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
auto_exclude_hidden = true
follow_links = false
dynamic_threshold = 5

[watcher]
enabled = true
```

### Old format (pre-v0.3.7, still readable)

```toml
[vault]
notes_dir = "/home/name/Notes"
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"
```

> **Migration:** Run `shiotsuchi config-migrate` to upgrade an old-format config to the new format.
> This creates a timestamped `.bak` backup before rewriting the file.

### Multi-vault example

```toml
[database]
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[vaults.personal]
notes_dir = "/home/name/Documents/Personal"

[vaults.work]
notes_dir = "/home/name/Documents/Work"
```

## Configuration Fields

### `[database]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `db_path` | string | `~/.cache/shiotsuchi/db.sqlite3` | Path to the shared SQLite database |

### `[vaults.*]` sections

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `notes_dir` | string | `.` | Root directory of the vault |
| `db_path` | string | — | **Legacy:** ignored in `[vaults.*]`; use `[database].db_path` instead |

### `[indexing]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `snippet_lines` | integer | 3 | Context lines to show around each search match |
| `max_snippet_chars` | integer | 1000 | Maximum characters in a search snippet (clamped to 128–65 535) |
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
unknown field `exclude_patterns`, expected one of `snippet_lines`, `max_snippet_chars`, `include_extensions`, `exclude_dirs`, ...
```

**Fix:** Rename the key to `exclude_dirs` in your config file:

```toml
# Before (v0.2.8 and earlier)
exclude_patterns = ["node_modules", "templates"]

# After (v0.2.9+)
exclude_dirs = ["node_modules", "templates"]
```

## Implementation Files

- `cli/src/main.rs` — Entry point, argument parsing with `clap`, dynamic version info
- `cli/src/config.rs` — Config types (`ShiotsuchiConfig`, `DatabaseConfig`, `VaultEntry`, `IndexingConfig`, `WatcherConfig`)
- `cli/src/build_info.rs` — Dynamic version string generation for `--version` / `support`
- `cli/src/util.rs` — Shared utilities (e.g., resolving DB path)
- `cli/src/commands/chart.rs` — Full vault indexing (chunk-aware, optional embedding)
- `cli/src/commands/clean.rs` — DB backup, delete, and re-index
- `cli/src/commands/config.rs` — Config subcommands (`detect-noise`)
- `cli/src/commands/config_migrate.rs` — Old-to-new config format migration
- `cli/src/commands/delete.rs` — Remove a note from the index
- `cli/src/commands/dive.rs` — Search with keyword/semantic/hybrid modes, JSON output
- `cli/src/commands/doctor.rs` — Environment health check with interactive repair
- `cli/src/commands/dredge.rs` — Chunk migration for pre-v0.3.3 vaults
- `cli/src/commands/init.rs` — Config file creation with interactive exclusion selection
- `cli/src/commands/log.rs` — Metadata listing
- `cli/src/commands/noise.rs` — Vault scanning logic for exclusion candidate detection
- `cli/src/commands/scan.rs` — File watcher setup
- `cli/src/commands/setup.rs` — ONNX model download/check
- `cli/src/commands/support.rs` — Build info display
- `cli/src/commands/tide.rs` — Statistics display (chunk/file/vector counts)

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
| `doctor` | Human-readable diagnostic with interactive repair prompts (TTY) or read-only checks (non-TTY) |
| `tide` | Human-readable statistics |
| `scan` | Watcher logs |
| `log` | Table with columns |
| `init` | Human-readable config creation summary |
| `config detect-noise` | Human-readable exclusion candidate list |
| `setup` | Model availability and hash verification output |
| `dredge` | Re-indexing progress |
| `support` | Build info and dependency version table |
