# CLI Usage — shiotsuchi

`shiotsuchi` is the command-line interface for indexing, searching, and watching Markdown note vaults.

> For installation instructions, see [docs/INSTALL.md](INSTALL.md).

---

## Quick start

```sh
# 1. Create a config file
shiotsuchi init --notes-dir ~/Notes

# 2. Index your vault
shiotsuchi chart

# 3. Search
shiotsuchi dive "project plan"
```

---

## Commands

### `init` — Create a config file

Generates `~/.config/shiotsuchi/config.toml` (or `$XDG_CONFIG_HOME/shiotsuchi/config.toml`) with default settings. When run interactively in a TTY, it scans the vault for exclusion candidates (directories like `node_modules`, `dist`, `templates`, etc.) and presents a 2-stage selection UI. Use `--yes` to auto-accept all candidates in non-interactive environments (CI, scripts).

```sh
# Interactive mode (default)
shiotsuchi init --notes-dir ~/Notes

# Non-interactive mode (CI, scripts)
shiotsuchi init --notes-dir ~/Notes --yes

# Regenerate config with latest exclusion candidates
shiotsuchi init --notes-dir ~/Notes --force --yes
```

| Option | Default | Description |
|--------|---------|-------------|
| `--notes-dir` | `.` | Vault root directory to store in the config |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Database path to store in the config |
| `--force` | off | Overwrite an existing config file (creates a timestamped `.bak` backup) |
| `--yes` | off | Non-interactive mode: auto-accept all detected exclusion candidates |

---

### `chart` — Index a vault

Walks every `.md` file in the vault, tokenizes content using the bundled Vaporetto model, and writes a SQLite index.

```sh
shiotsuchi chart --notes-dir ~/Notes
```

Re-running `chart` is safe — it compares file hashes and only updates changed files.

| Option | Default | Description |
|--------|---------|-------------|
| `--notes-dir` | `.` | Root directory of the vault |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Path to the SQLite index |
| `--verbose` | off | Print per-file progress |

---

### `dive` — Search notes

Runs a full-text AND search against the index and returns matching notes with snippets.

```sh
shiotsuchi dive "weekly review"
shiotsuchi dive "Q3 budget" --limit 5
shiotsuchi dive "meeting" --json   # machine-readable output
```

| Option | Default | Description |
|--------|---------|-------------|
| `--notes-dir` | from config / `.` | Used to resolve relative snippet paths |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Index to search |
| `--limit` | 20 | Maximum number of results |
| `--json` | off | Output raw JSON instead of pretty-printed |

Result fields: `path`, `title`, `snippet`, `score`.

---

### `scan` — Watch for changes

Monitors the vault directory for file changes and updates the index automatically.

```sh
shiotsuchi scan --notes-dir ~/Notes
```

Keep this running in a terminal or register it as a background service. The watcher debounces rapid edits (default 500 ms) before re-indexing.

| Option | Default | Description |
|--------|---------|-------------|
| `--notes-dir` | from config / `.` | Vault root to watch |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Index to update |

---

### `tide` — Vault statistics

Shows the total note count, last indexed time, and database size.

```sh
shiotsuchi tide
```

---

### `config detect-noise` — Scan for exclusion candidates

Scans the vault for directories matching known noise patterns or containing many markdown files, and prints a human-readable report. Does **not** modify the config file — use `shiotsuchi init --force` to update the config with the detected candidates.

```sh
shiotsuchi config detect-noise --notes-dir ~/Notes
```

| Option | Default | Description |
|--------|---------|-------------|
| `--notes-dir` | from config | Vault root to scan |

Output format:

```
Exclusion candidates in /Users/yourname/Notes:
  1. node_modules [known] (142 files)
  2. dist [known] (3 files)
  3. archive [known] (0 files)
  4. generated_docs [dynamic] (15 files)
```

---

### `log` — Indexing history

Lists the most recently indexed files with timestamps.

```sh
shiotsuchi log
```

---

## Configuration file

Create `~/.config/shiotsuchi/config.toml` (or `$XDG_CONFIG_HOME/shiotsuchi/config.toml`) to avoid repeating flags on every command.

```toml
[vault]
notes_dir = "/Users/yourname/Notes"
db_path   = "/Users/yourname/.cache/shiotsuchi/db.sqlite3"

[indexing]
snippet_lines      = 3
include_extensions = ["md", "markdown"]
exclude_dirs       = ["node_modules"]
auto_exclude_hidden = true
follow_links       = false
dynamic_threshold  = 5

[watcher]
debounce_ms = 500
enabled     = true
```

> **Note:** The field `exclude_patterns` was renamed to `exclude_dirs` in v0.2.9.
> If your existing config uses `exclude_patterns`, rename the key to `exclude_dirs`.

CLI flags always take precedence over config file values.

---

## Using multiple vaults

One index = one vault. Use `--db-path` to point each command at the correct index.

### Example: Personal and Work vaults

Index:

```sh
shiotsuchi chart --notes-dir ~/Personal --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi chart --notes-dir ~/Work     --db-path ~/.cache/shiotsuchi/work.db
```

Search:

```sh
shiotsuchi dive "photo trip"   --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi dive "Q3 budget"    --db-path ~/.cache/shiotsuchi/work.db
```

Watch both vaults (run each in a separate terminal or background process):

```sh
shiotsuchi scan --notes-dir ~/Personal --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi scan --notes-dir ~/Work     --db-path ~/.cache/shiotsuchi/work.db
```

---

## Using the CLI together with the MCP server

The CLI builds and maintains the index; the MCP server makes it searchable by an LLM.

Typical workflow:

1. **Index** — `shiotsuchi chart` (one-off or scheduled)
2. **Watch** — `shiotsuchi scan` (keeps the index current as you write)
3. **Search via LLM** — `shiotsuchi-mcp` answers tool calls from Claude or another LLM client

The CLI and MCP server share the same SQLite database. WAL mode is enabled so both can access it concurrently without conflict.

> For MCP server setup (Claude Desktop, Claude Code CLI, generic clients), see [docs/MCP-SETUP.md](MCP-SETUP.md).

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `command not found: shiotsuchi` | Add `~/.local/bin` (or `~/.cargo/bin`) to `PATH`; see [INSTALL.md](INSTALL.md) |
| `dive` returns no results | Run `shiotsuchi chart` first to build the index |
| `dive` says index not found | Check `--db-path` matches the path used with `chart` |
| New notes not appearing | Re-run `chart`, or start `scan` to watch for changes |
| Config file ignored | Confirm path is `~/.config/shiotsuchi/config.toml`; TOML syntax errors are logged as warnings |

---

## Further reading

- [docs/INSTALL.md](INSTALL.md) — Build and install binaries
- [docs/MCP-SETUP.md](MCP-SETUP.md) — Use the index from an LLM via MCP
- [ref/cli.md](../ref/cli.md) — Command reference (all flags)
- [ref/architecture.md](../ref/architecture.md) — Design and data model
