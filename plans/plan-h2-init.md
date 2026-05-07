# Plan: `shiotsuchi init`

## Goal

Provide a single command to bootstrap a user's local configuration file, reducing onboarding friction.

## Motivation

Currently, users must manually create `~/.config/shiotsuchi/config.toml` or rely entirely on environment variables and CLI flags. An `init` command creates a discoverable, editable starting point.

## Specification

### Command

```
shiotsuchi init [OPTIONS]
```

### Options

| Flag | Description |
|------|-------------|
| `--force` | Overwrite an existing config file. Without this, the command fails if the file already exists. |
| `--notes-dir <PATH>` | Set the vault directory in the generated config. Defaults to `.` (current directory). |
| `--db-path <PATH>` | Set the database path in the generated config. Defaults to XDG cache fallback. |
| `--verbose` | Enable debug logging (global). |

### Behavior

1. **Resolve config path**
   - `$XDG_CONFIG_HOME/shiotsuchi/config.toml`
   - Fallback: `~/.config/shiotsuchi/config.toml`

2. **Check for existing file**
   - If the file exists and `--force` is **not** set, exit with a clear error:
     ```
     Config file already exists at ~/.config/shiotsuchi/config.toml. Use --force to overwrite.
     ```
   - If `--force` is set, proceed to overwrite.

3. **Ensure directory exists**
   - `mkdir -p $(dirname config_path)`

4. **Build config**
   - Start with `ShiotsuchiConfig::default()`.
   - Override `vault.notes_dir` if `--notes-dir` is provided (CLI global flag).
   - Override `vault.db_path` if `--db-path` is provided (CLI global flag).

5. **Serialize and write**
   - Use `toml::to_string_pretty` for human-readable output.
   - Write atomically (write to temp, then rename) is **not** required for a local config file.

6. **Inform user**
   - Print the absolute path of the created file.
   - Print a suggested next step: `Next, run shiotsuchi chart to index your vault.`

### Generated File Format

```toml
[vault]
notes_dir = "/home/user/Notes"
db_path = "/home/user/.cache/shiotsuchi/db.sqlite3"

[indexing]
snippet_lines = 3
include_extensions = ["md", "markdown"]
exclude_patterns = [".obsidian", ".git", "node_modules"]

[watcher]
debounce_ms = 500
enabled = true
```

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| Config dir does not exist | Create it automatically. |
| Config file exists, no `--force` | Error message, non-zero exit. |
| Config file exists, with `--force` | Overwrite silently. |
| `--notes-dir` points to a non-existent path | Still write the path into the config; do not validate existence at init time. |
| Home directory is unavailable | Fallback to current directory for both config and default DB path. |

## Implementation Checklist

- [x] Add `toml` dependency to `cli/Cargo.toml`.
- [x] Create `cli/src/commands/init.rs` with `InitArgs` and `run_init`.
- [x] Add `Init` variant to `Commands` enum in `cli/src/main.rs`.
- [x] Wire `run_init` into the `match cli.command` block.
- [x] Export `xdg_config_home()` and add `default_config_path()` in `cli/src/config.rs`.
- [x] Pass resolved `ShiotsuchiConfig` (after CLI overrides) to `run_init`.
- [x] Write unit tests for create, refuse-overwrite, and force-overwrite scenarios.
- [x] Update `ref/cli.md` command table.

# Future Work 

- Interactive prompts (e.g. `Enter your notes directory:`).
  - initをかけたフォルダがデフォルトでよい
- Validation that `notes_dir` exists and contains Markdown files.
- Backing up the old config before overwrite.


「. で始まるディレクトリを全部除外」 するようにしよう。
その上で、templates, archive, archived, dist, build など色々候補はあるだろうが、shiotsuchi init したときにそこから下のフォルダを検索して、除外候補を探すようにコマンドを変えよう。
インタラクティブに尋ねると反映する形式にする。

また、init したときにすでに 設定ファイルがある場合には、現時点のファイルをいったん .bak でコピーを作った上で、修正する。