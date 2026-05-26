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

> The `--verbose` flag is available on every command and prints debug-level logging (e.g., per-file processing details, SQL queries). Useful for troubleshooting.

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
| `--quiet` | off | Suppress the summary output |
| `--vault` | — | Restrict indexing to a specific vault (e.g., `--vault work`) |

### Exclude patterns

You can exclude files from indexing using two mechanisms (both use the same glob syntax):

1. **`config.toml`:** Set `exclude_dirs` in the `[indexing]` section.
2. **`.shiotsuchiignore`:** Place a file named `.shiotsuchiignore` in the vault root directory.

Patterns support `*` (any chars), `**` (recursive), `?` (single char), `[abc]` (character class).

```sh
# Example .shiotsuchiignore
node_modules
*.tmp
private/
draft_*
```

Patterns from both sources are merged at index time.

### `check-ignore` — Diagnose exclude patterns

Checks whether a given relative path would be excluded by `exclude_dirs` or `.shiotsuchiignore`.

```sh
shiotsuchi check-ignore "node_modules/foo.md"
# ✗ EXCLUDED: node_modules/foo.md
#   Reason: matched config.toml exclude_dirs (pattern: node_modules)

shiotsuchi check-ignore "doc/manual.md"
# ✓ NOT excluded: doc/manual.md
```

| Option | Default | Description |
|--------|---------|-------------|
| `<path>` | — | Relative path to check (e.g. `private/notes.md`) |
| `--vault` | first vault | Vault whose exclude rules to check against |

---

### `dive` — Search notes

Searches the index using keyword (FTS5 BM25), vector (semantic), or hybrid mode. Returns matching chunks with file paths, parent headings, and snippets.

```sh
shiotsuchi dive "weekly review"
shiotsuchi dive "Q3 budget" --limit 5
shiotsuchi dive "プロジェクト計画" --mode vec       # semantic vector search
shiotsuchi dive "meeting" --mode hybrid --alpha 0.3  # vec-weighted hybrid
shiotsuchi dive "app dev" --fuzzy                    # case/NFC-normalized search
shiotsuchi dive "plan" --tag project --since 2026-01-01  # frontmatter filters
shiotsuchi dive "AWS" --mmr --lambda 0.7             # diversity reranking
shiotsuchi search "project plan"                     # alias for dive
```

| Option | Default | Description |
|--------|---------|-------------|
| `--notes-dir` | from config / `.` | Used to resolve relative snippet paths |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Index to search |
| `--limit` | 20 | Maximum number of results |
| `--mode` | `hybrid` (or `fts` if no model) | Search mode: `fts`, `vec`, `hybrid` |
| `--format` | `table` | Output format: `table` / `json` / `json-pretty` |
| `--vault` | — | Filter results to a specific vault (e.g., `--vault work`) |
| `--tag` | — | Filter by frontmatter tag (e.g., `--tag project`) |
| `--since` | — | Filter by frontmatter date, ISO 8601 (e.g., `--since 2026-01-01`) |
| `--fuzzy` | off | Enable Unicode NFKC normalization + case folding for typo-tolerant search |
| `--alpha` | 0.5 | Hybrid blend ratio (0.0=vec only, 1.0=FTS only) |
| `--mmr` | off | Enable MMR diversity re-ranking |
| `--lambda` | 0.5 | MMR relevance/diversity balance (0.0=diversity, 1.0=relevance) |
| `--threshold` | — | Minimum score threshold. FTS/Vec: excludes results with score above threshold. Hybrid: excludes results below threshold. |
| `--model-path` | — | Path to ONNX embedding model file (overrides config/env) |

> **ANSI highlighting:** Matched search terms are highlighted in the table format output. Set `NO_COLOR=1` or pipe to a file to disable colors.

**Search modes:**

| Mode | Description | Model required |
|------|-------------|---------------|
| `fts` | Keyword search via FTS5 BM25. Full-width/half-width normalization. | No |
| `vec` | Semantic vector search via cosine similarity. Requires `--model-path` or config. | Yes |
| `hybrid` | Default. Reciprocal Rank Fusion (RRF) of FTS + Vec. Falls back to FTS if no model. | Optional |

**MMR (Maximal Marginal Relevance):**

When `--mmr` is enabled, results are re-ranked to balance relevance and diversity. Lambda controls the trade-off:
- `--lambda 1.0`: pure relevance (same as default ranking)
- `--lambda 0.5`: equal balance (default)
- `--lambda 0.0`: max diversity

---

### `delete` — Remove a note from the index

Removes a note entry from the SQLite index by relative vault path. The path is validated to prevent directory traversal (`..`) and vault escape. If the file no longer exists on disk, the DB entry is cleaned up directly.

```sh
shiotsuchi delete meeting/notes.md
```

| Argument | Description |
|----------|-------------|
| `<path>` | Relative path within the vault (e.g., `meeting/notes.md`) |

**Global options** (available on all commands):

| Option | Default | Description |
|--------|---------|-------------|
| `--notes-dir` | from config / `.` | Vault root to resolve paths |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Database to modify |

---

### `scan` — Watch for changes

Monitors the vault directory for file changes and updates the index automatically.

```sh
shiotsuchi scan --notes-dir ~/Notes
```

Keep this running in a terminal or register it as a background service. Rapid edits are debounced before re-indexing.

| Option | Default | Description |
|--------|---------|-------------|
| `--notes-dir` | from config / `.` | Vault root to watch |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Index to update |
| `--vault` | — | Watch only a specific vault (e.g., `--vault work`) |

---

### `tide` — Vault statistics

Shows total note count, last indexed time, database size, top 10 tags by frequency, and total character count.

```sh
shiotsuchi tide
shiotsuchi tide --json   # JSON output
```

| Option | Default | Description |
|--------|---------|-------------|
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Database to read statistics from |
| `--json` | off | Output statistics as JSON |

---

### `synonym` — Manage thesaurus entries

Manages synonym/thesaurus entries for FTS5 query expansion. Entries are stored in `~/.config/shiotsuchi/thesaurus.toml`.

```sh
shiotsuchi synonym add AWS "Amazon Web Services"
shiotsuchi synonym add AWS "アマゾンウェブサービス"
shiotsuchi synonym list
shiotsuchi synonym remove AWS
```

The thesaurus file is auto-created on first use. Entries are merged into `config.toml` synonyms at startup (thesaurus takes priority).

| Subcommand | Description |
|------------|-------------|
| `add <word> <synonyms>...` | Add a synonym pair (word → one or more synonyms) |
| `remove <word>` | Remove an entire word entry |
| `list` | List all registered entries |

---

### `tasks` — Search tasks across all vaults

Searches all indexed notes for Markdown task checkboxes (`- [ ]` and `- [x]`).

```sh
shiotsuchi tasks                          # show all incomplete tasks
shiotsuchi tasks "レビュー"                # filter tasks by keyword
shiotsuchi tasks --all                    # include completed tasks
```

| Option | Default | Description |
|--------|---------|-------------|
| `<keyword>` | — | Filter tasks by keyword (case-insensitive LIKE) |
| `--all` | off | Include completed tasks (`- [x]`) in results |

---

### `clean` — Backup and re-index from scratch

Backs up the current database file (with timestamp), deletes it, and then re-indexes all vaults from scratch.

```sh
shiotsuchi clean
```

Backup files are created alongside the database file:
- `db.sqlite3.bak.<timestamp>`
- `db.sqlite3-wal.bak.<timestamp>` (if exists)
- `db.sqlite3-shm.bak.<timestamp>` (if exists)

| Option | Default | Description |
|--------|---------|-------------|
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Database to back up and re-create |

---

### `config-migrate` — Upgrade config format

Converts the config file from the old `[vault]` format to the new `[database]` + `[vaults.xxx]` format. Creates a timestamped `.bak` backup before rewriting.

```sh
shiotsuchi config-migrate
```

| Option | Default | Description |
|--------|---------|-------------|
| `--config` | `~/.config/shiotsuchi/config.toml` | Path to config file |

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

| Option | Default | Description |
|--------|---------|-------------|
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | Database to read history from |

---

### `doctor` — Environment health check with interactive repair

Checks that all components of a shiotsuchi installation are working: config file, database, Vaporetto tokenizer, ONNX embedder model, and vault directories.

When run in a terminal, doctor will detect fixable issues and prompt you to repair them interactively with `[y/N]`. In non-TTY environments (pipes, CI), it runs in read-only diagnostic mode.

```sh
shiotsuchi doctor
```

**Fixable issues:**

| Issue | Prompt | Action |
|-------|--------|--------|
| Config unknown field in `[indexing]` | Remove unknown field? | Strips the unknown key, saves timestamped backup |
| Config old `[vault]` format | Migrate to new format? | Converts to `[database]` + `[vaults.xxx]`, saves backup |
| Database not found | Index your vault now? | Creates database and indexes all vault files |
| Database open/stats failure | Rebuild from scratch? | Backs up corrupt DB, deletes old files, re-indexes |

**Non-fixable issues** show a hint instead of a prompt (e.g., missing tokenizer model, embedder model, or vault directory).

Example output with interactive fix:

```
[!!] Config: /home/name/.config/shiotsuchi/config.toml (parse error: unknown field `snippet_lines`)
    Remove unknown field(s) `snippet_lines` from [indexing]? [y/N] y
  Backup saved to: config.toml.bak.1712345678.000000
[ok] Config: fixed
[ok] Database: /home/name/.cache/shiotsuchi/db.sqlite3 (1,234 files, 5,678 chunks)
[ok] Tokenizer: Vaporetto model loaded
[..] Embedder: ONNX model not found (vector search disabled)
     [hint] Run `shiotsuchi setup` to install the embedder model.
[ok] Vault 'default': /home/name/Notes

All checks passed.
```

---

### `completion` — Generate shell completion scripts

Outputs a shell completion script for `shiotsuchi` subcommands and flags. Source the output in your shell's rc file.

```sh
# Bash
source <(shiotsuchi completion bash)

# Zsh (add to ~/.zshrc)
shiotsuchi completion zsh > /usr/local/share/zsh/site-functions/_shiotsuchi

# Fish
shiotsuchi completion fish > ~/.config/fish/completions/shiotsuchi.fish

# PowerShell
shiotsuchi completion powershell | Out-String | Invoke-Expression
```

---

## Configuration file

Create `~/.config/shiotsuchi/config.toml` (or `$XDG_CONFIG_HOME/shiotsuchi/config.toml`) to avoid repeating flags on every command.

### New format (v0.4.0+)

```toml
[database]
db_path = "~/.cache/shiotsuchi/db.sqlite3"
vault_default = "personal"          # optional: default vault when --vault omitted

[vaults.personal]
notes_dir = "/Users/name/Documents/Personal"

[vaults.work]
notes_dir = "/Users/name/Documents/Work"

[indexing]
snippet_lines       = 3
max_snippet_chars   = 1000
include_extensions  = ["md", "markdown"]
exclude_dirs        = ["node_modules"]
auto_exclude_hidden = true
follow_links        = false
dynamic_threshold   = 5
user_dictionary     = ["Vaporetto", "shiotsuchi"]  # custom Vaporetto tokens

# Thesaurus synonyms (also managed via `shiotsuchi synonym`)
[synonyms]
AWS = ["Amazon Web Services", "アマゾンウェブサービス"]

# Search tuning (optional)
hybrid_alpha       = 0.5   # blend ratio (0.0=vec only, 1.0=FTS only, default 0.5)
semantic_threshold = 0.75  # minimum score threshold
```

### Old format (pre-v0.3.7, still readable)

```toml
[vault]
notes_dir = "/home/name/Notes"
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"
```

> **Migration:** Run `shiotsuchi config-migrate` to upgrade from the old `[vault]` format.
> A timestamped `.bak` backup is created before rewriting.

### Multi-vault example

```toml
[database]
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[vaults.personal]
notes_dir = "/home/name/Documents/Personal"

[vaults.work]
notes_dir = "/home/name/Documents/Work"
```

> **Note:** The field `exclude_patterns` was renamed to `exclude_dirs` in v0.2.9.
> If your existing config uses `exclude_patterns`, rename the key to `exclude_dirs`.

CLI flags always take precedence over config file values.

---

## Using multiple vaults

Multiple vaults share a single SQLite database. Each chunk is tagged with a `vault_name` so search results indicate which vault they came from. All commands operate on all configured vaults by default.

### Setup in config

```toml
[database]
db_path = "~/.cache/shiotsuchi/db.sqlite3"

[vaults.personal]
notes_dir = "/Users/name/Documents/Personal"

[vaults.work]
notes_dir = "/Users/name/Documents/Work"
```

### Indexing

```sh
# Indexes both vaults
shiotsuchi chart
```

### Search

Search works across all vaults. The MCP handler accepts an optional `vault` parameter for filtering.

```sh
# Searches all vaults
shiotsuchi dive "Q3 budget"
```

### Watching

```sh
# Watches all configured vaults
shiotsuchi scan
```

### Clean (backup + re-index)

```sh
# Backs up DB, deletes, re-indexes all vaults
shiotsuchi clean
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

- [README.md](../README.md) — Project overview, features, and commands
- [docs/INSTALL.md](INSTALL.md) — Build and install binaries
- [docs/MCP-SETUP.md](MCP-SETUP.md) — Use the index from an LLM via MCP
- [ref/cli.md](../ref/cli.md) — Command reference (all flags)
- [ref/architecture.md](../ref/architecture.md) — Design and data model
