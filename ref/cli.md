# CLI (shiotsuchi)

Binary name: `shiotsuchi`
Crate path: `cli/`

## Interactive mode

Running `shiotsuchi` without a subcommand opens an interactive welcome screen with an onboarding wizard and categorized command menu. See `docs/CLI-USE.md` for details.

## Commands

| Command | Args | Description |
|---------|------|-------------|
| `index` (alias: `chart`) | `[--notes-dir]` `[--db-path]` `[--vault]` | Index/re-index all Markdown files in all configured vaults. Reports indexed/skipped/error/excluded counts. |
| `search` (alias: `dive`) | `[--notes-dir]` `[--db-path]` `[--limit]` `[--mode]` `[--json]` `[--json-pretty]` `[--fuzzy]` `[--alpha]` `[--tag]` `[--since]` `[--vault]` `[--mmr]` `[--lambda]` `[--threshold]` `[--model-path]` | Search notes. `--mode`: `fts` (default), `vec`, `hybrid`. Old name: `dive`. |
| `prune` (alias: `dredge`) | `[--notes-dir]` `[--db-path]` | Extract and index chunks from existing notes without re-embedding content. Migrates pre-v0.3.3 vaults to chunked schema. |
| `watch` (alias: `scan`) | `[--notes-dir]` `[--db-path]` `[--vault]` | Watch all configured vaults for file changes and auto-re-index |
| `list` (alias: `log`) | `[--db-path]` | Show indexing history |
| `stats` (alias: `tide`) | `[--db-path]` `[--json]` | Show vault statistics with optional JSON output (chunks, files, tags, vec status) |
| `check-ignore <path>` | `[--vault]` | Check whether a relative path would be excluded by `exclude_dirs` or `.shiotsuchiignore` patterns |
| `clean` | `[--db-path]` | Backup the database, delete it, then re-index all vaults from scratch |
| `config detect-noise` | `[--notes-dir]` | Scan vault for exclusion candidates (read-only) |
| `config-migrate` | `[--config]` | Migrate config from old `[vault]` format to new `[database]` + `[vaults.xxx]` format |
| `delete <path>` | `[--notes-dir]` `[--db-path]` | Remove a note from the index by its vault-relative path |
| `doctor` | (no args) | Environment health check with interactive repair for config, database, tokenizer, embedder, and vault directories |
| `init` | `[--notes-dir]` `[--db-path]` `[--force]` `[--yes]` | Create config file with interactive exclusion selection |
| `setup` | `[--check]` `[--model-path]` | Setup/check ONNX embedding model and Vaporetto tokenizer. `--check` verifies model availability and hash. |
| `synonym` | `add/remove/list` | — | Manage thesaurus entries via CLI (synonym add/remove/list)
| `tasks` | `[<keyword>]` `[--all]` | — | Cross-vault task checkbox search (incomplete `- [ ]` and completed `- [x]`) |
| `support` | (no subcommands) | Display build info, dependency versions, and system information |

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

### Default vault

```toml
[database]
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"
vault_default = "work"

[vaults.personal]
notes_dir = "/home/name/Documents/Personal"

[vaults.work]
notes_dir = "/home/name/Documents/Work"
```

When `vault_default` is set and no `--vault` flag is given, `search`, `index`, and `watch` operate on only that vault.```

## Configuration Fields

### `[database]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `db_path` | string | `~/.cache/shiotsuchi/db.sqlite3` | Path to the shared SQLite database |
| `vault_default` | string | — | Default vault ID used when `--vault` is not specified |
| `semantic_threshold` | float | — | Minimum score threshold for search results. FTS/Vec: excludes results with score > threshold. Hybrid: excludes results with RRF score < threshold. CLI `--threshold` overrides this. |

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
| `user_dictionary` | string array | `[]` | Custom dictionary entries for Vaporetto tokenization |

Note: `exclude_dirs` patterns support glob wildcards (`*`, `**`, `?`, `[abc]`, `{a,b}`).
Patterns containing `/` are matched against the full relative path (e.g. `private/**`).
Bare names match directories at any depth (e.g. `node_modules` matches `a/node_modules/foo.md`).

Additionally, you can place a `.shiotsuchiignore` file in the vault root directory.
It uses the same glob syntax as `exclude_dirs`. Patterns from both sources are merged
at index time. Use `shiotsuchi check-ignore <path>` to diagnose why a file is excluded.

```sh
# Example .shiotsuchiignore
node_modules
*.tmp
private/
draft_*
```

### `[synonyms]` section (thesaurus)

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| (key) | string array | — | Synonym expansion map. Keys are query tokens, values are lists of synonyms OR-expanded in FTS5 queries. Example: `AWS = ["Amazon Web Services"]` |

Synonyms are also loaded from a standalone `~/.config/shiotsuchi/thesaurus.toml` file, managed by the `shiotsuchi synonym` CLI command.

### `[embedder]` section

Controls which embedding model is used for semantic indexing. Omitting this section (or setting `provider = "built-in"`) uses the standard model resolution order: `SHIOTSUCHI_EMBED_MODEL_PATH` env var → `~/.local/share/shiotsuchi/model.onnx`.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `provider` | string | `"built-in"` | Embedding provider. `"built-in"` uses env var / XDG default; `"onnx-file"` loads a specific file; `"api"` uses an OpenAI-compatible HTTP API. |
| `path` | string | — | Required when `provider = "onnx-file"`. Absolute path to the ONNX model file (must be alongside `tokenizer.json`). |
| `endpoint` | string | — | Required when `provider = "api"`. Base URL of the OpenAI-compatible embedding API (e.g. `https://api.ai.sakura.ad.jp/v1/embeddings`). |
| `model` | string | — | Required when `provider = "api"`. Model name to request (e.g. `multilingual-e5-large`). |
| `api_key` | string | — | Optional fallback API key when `provider = "api"`. The `SHIOTSUCHI_API_KEY` environment variable takes precedence; use it instead of this field for better security. |

**Example — custom ONNX model:**

```toml
[embedder]
provider = "onnx-file"
path = "/path/to/my-model/model.onnx"
```

**Example — API provider (Sakura AI):**

```toml
[embedder]
provider = "api"
endpoint = "https://api.ai.sakura.ad.jp/v1/embeddings"
model = "multilingual-e5-large"
```

> **Security note:** When using `provider = "api"`, set the API key via the `SHIOTSUCHI_API_KEY` environment variable instead of `api_key` in `config.toml`. The CLI will warn you if the key is stored in the config file.

> **Note on model changes:** If you change the model after indexing, the existing vector embeddings in the database were generated with a different model and will be incompatible. Run `shiotsuchi index` to re-index all files. A warning is shown at index time when a model change is detected.

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
- `cli/src/commands/check_ignore.rs` — Exclude pattern diagnostics
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
- `cli/src/commands/synonym.rs` — Thesaurus entry management
- `cli/src/commands/tasks.rs` — Task checkbox search
- `cli/src/commands/setup.rs` — ONNX model download/check
- `cli/src/commands/support.rs` — Build info display
- `cli/src/commands/tide.rs` — Statistics display (chunk/file/vector counts, tag stats)

## DB Path Resolution

Default DB path: `~/.cache/shiotsuchi/db.sqlite3`
Resolution order:
1. `XDG_CACHE_HOME/shiotsuchi/db.sqlite3` (if env var set)
2. `~/.cache/shiotsuchi/db.sqlite3` (fallback)
3. Current directory `./.cache/shiotsuchi/db.sqlite3` (if home dir unavailable)

## Error Handling

- `main()` returns `Result<(), Box<dyn std::error::Error>>`
- `search` checks `db_path.exists()` before opening and shows a helpful message if not found
- Config parse errors are logged as warnings (not silently ignored)

## Outputs

| Command | Output Format |
|---------|--------------|
| `index` | Human-readable progress (indexed/skipped/errors, invalid patterns if any, excluded file count) |
| `check-ignore` | Human-readable exclusion diagnosis (EXCLUDED / NOT excluded + matching pattern source) |
| `search` | Pretty JSON with ANSI-highlighted matched terms (or raw JSON with `--json`) |
| `doctor` | Human-readable diagnostic with interactive repair prompts (TTY) or read-only checks (non-TTY) |
| `stats` | Human-readable statistics (or JSON with `--json`) |
| `tasks` | Human-readable task list with status markers (`[ ]` / `[x]`) |
| `watch` | Watcher logs |
| `list` | Table with columns |
| `init` | Human-readable config creation summary |
| `config detect-noise` | Human-readable exclusion candidate list |
| `synonym` | Human-readable add/remove/list results |
| `setup` | Model availability and hash verification output |
| `prune` | Re-indexing progress |
| `support` | Build info and dependency version table |
