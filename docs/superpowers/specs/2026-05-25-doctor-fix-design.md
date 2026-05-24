# shiotsuchi doctor — Interactive Fix Mode

## Summary

Extend `shiotsuchi doctor` from a read-only diagnostic command to one that
detects issues **and** offers to fix them interactively. Each check that fails
prompts the user with `Fix this? [y/N]` immediately after the diagnostic line.
If the user accepts, the fix is applied with a backup taken beforehand.

## Scope

All currently checked items are covered, but only **fixable** issues get a
prompt. Unfixable environment-dependent issues (tokenizer missing, embedder
missing) show a human-readable hint instead.

| Check | Fixable? | Prompt | Action |
|-------|----------|--------|--------|
| Config parse error (unknown field) | Yes | Remove unknown field? | Read as `toml::Table`, strip unknown keys from `[indexing]`, backup + rewrite |
| Config old `[vault]` format | Yes | Migrate to new format? | Run `config-migrate` logic inline (backup + rewrite) |
| DB not found | Yes | Index now? | Run `index_directory` directly (like `chart` — discover files, tokenize, embed, write) |
| DB open/stats failure | Yes | Rebuild from scratch? | Backup DB, delete old files, re-index (`clean`-equivalent) |
| Vault dir not found | **No** | — | Show `[!!]` + hint "Directory does not exist. Configure the correct path or create the directory." Leave config untouched |
| Tokenizer unavailable | **No** | — | Show `[..]` + hint "Run `shiotsuchi setup` or set SHIOTSUCHI_MODEL_PATH" |
| Embedder unavailable | **No** | — | Show `[..]` + hint "Run `shiotsuchi setup` to download the model" |

## Architecture

### Single-file change: `cli/src/commands/doctor.rs`

No new modules. No new structs. The fix logic lives in private helper functions
inside `doctor.rs`. Existing utilities — `backup_file`, `delete_db_files`,
`secure_parent_dir` from `clean.rs` and `util.rs` — are reused.

Helpers to add:

| Helper | Purpose |
|--------|---------|
| `ask() -> bool` | Wraps `dialoguer::Confirm` with common defaults |
| `fix_config_unknown_fields(path, error) -> Result` | Parse TOML, strip unknown fields, backup + rewrite |
| `fix_config_old_vault_format(path) -> Result` | Read old config, write new format, backup |
| `fix_db_not_found(db_path, vaults, indexing_cfg) -> Result` | Index directory into new DB |
| `fix_db_corrupt(db_path, vaults, indexing_cfg) -> Result` | Backup, delete old, re-index |

### Dialog

`dialoguer = "0.12"` is already a dependency. Prompt style:

```
[!!] Config: /path/config.toml (parse error: unknown field `snippet_lines`)
    Remove unknown field `snippet_lines` from [indexing]? [y/N]
```

Each fix shows its outcome:

```
[ok] Config: fixed (backup: /path/config.toml.bak.1712345678)
```

### Signature change

```rust
// Before
pub fn run_doctor(db_path: &Path) -> Result<(), Box<dyn std::error::Error>>

// After
pub fn run_doctor(
    cfg: &ShiotsuchiConfig,
    db_path: &Path,
    vaults: &[(String, PathBuf)],
    indexing_cfg: &IndexingConfig,
) -> Result<(), Box<dyn std::error::Error>>
```

### Call-site change in `cli/src/main.rs`

```rust
Commands::Doctor(_args) => {
    commands::doctor::run_doctor(&cfg, &db_path, &resolved_vaults, &cfg.indexing)?;
}
```

### Config unknown field detection

1. Try `ShiotsuchiConfig::load_from(path)`
2. On error, parse the deserialization error message to extract the unknown
   field name from patterns like `unknown field \`snippet_lines\``
3. Read the raw file as `toml::Table`
4. Traverse to `[indexing]` subsection, identify keys not in the known set
   (`include_extensions`, `exclude_dirs`, `auto_exclude_hidden`,
   `follow_links`, `dynamic_threshold`)
5. Remove only the unknown keys and serialize back with `toml::to_string_pretty`

## Error handling

- Each fix is wrapped independently. If a fix fails (e.g. disk full), the error
  is printed but doctor continues to subsequent checks.
- Backups are taken before any destructive write. If the fix itself corrupts
  the file, the user can restore from backup.
- The overall `all_ok` flag still reflects whether *all checks currently pass*,
  regardless of whether fixes were applied.

## Testing

- **Config fix**: Write a TOML file with an extra `snippet_lines` field in
  `[indexing]`, run the fix helper, verify the output parses cleanly and the
  unknown field is gone. Verify backup file was created.
- **DB re-index**: Already covered by existing `clean` integration tests.
  The fix-DB helper delegates to the same `index_directory` path.
- **Interactive prompts**: Tested manually; dialoguer prompts are not
  automated in this project's test suite.

## Files changed

| File | Change |
|------|--------|
| `cli/src/commands/doctor.rs` | Main implementation (~250 lines → ~450 lines) |
| `cli/src/main.rs` | Update `run_doctor` call (1 line) |

No new dependencies. No new files.
