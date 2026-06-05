# Plan: `shiotsuchi init` — Future Work Implementation

## Overview

Implement three enhancements for `shiotsuchi init`:

1. **Auto-exclude hidden directories** in the indexer (`.git`, `.obsidian`, `.trash`, etc.)
2. **Scan vault for exclusion candidates** during `init` and offer an interactive prompt
3. **Back up existing config** as `.bak` before overwriting

---

## 1. Auto-exclude hidden directories

### Current behavior
`exclude_patterns` uses substring matching. Users must manually discover and list every hidden directory in their vault.

### Proposed change
In `core/src/indexer.rs`, modify the `WalkDir` filter to automatically skip any directory whose name starts with `.`. This is a **hardcoded behavior** independent of `exclude_patterns`.

```rust
// In index_directory(), before processing files:
.filter_entry(|e| {
    if e.file_type().is_dir() {
        !e.file_name().to_string_lossy().starts_with('.')
    } else {
        true
    }
})
// WalkDir::filter_entry is applied before into_iter(), so hidden dirs are pruned entirely.
```

### Rationale
- Hidden directories are overwhelmingly non-content (`.git`, `.obsidian`, `.Trash`, `.DS_Store`, `.vscode`, `.idea`, etc.)
- Reduces surprise for new users
- `exclude_patterns` remains useful for non-hidden directories like `node_modules`, `dist`, `templates`

### Open question
- Should we keep `.obsidian` / `.git` in the default `exclude_patterns` for backwards compatibility / explicitness, or remove them since they're now auto-excluded?

---

## 2. Interactive exclusion prompt during `init`

### Current behavior
`init` writes a static default config.

### Proposed change
After resolving `notes_dir`, scan the vault for directories that:
- Have names starting with `.` (hidden dirs — these will be auto-excluded, but we can still surface them for user awareness)
- Match known noise patterns: `node_modules`, `dist`, `build`, `templates`, `archive`, `archived`, `.trash`
- Contain `>= 5` files with matching `include_extensions`

Present a multi-select question: "Exclude these directories from indexing?" Pre-select the ones we detected. User's choices are written into `exclude_patterns`.

### Implementation options

| Option | Approach | Pros | Cons |
|--------|----------|------|------|
| A | Add `dialoguer` crate + use `MultiSelect` in `run_init` | Rich UX, arrow keys, space to toggle | New dependency, adds ~100KB, extra compile time |
| B | Stdin prompt (`println!` + `std::io::stdin`) | Zero deps, simple | Manual input parsing, no multi-select UI, rougher UX |

The prompt is **non-blocking**: if stdin is not a TTY (e.g. CI, piped), fall back to defaults silently.

### Open question
- Preferred interaction method: `dialoguer` (rich) or plain stdin (minimal)?
- Should the scan recurse into subdirectories or only look one level deep?

---

## 3. Backup before overwrite

### Current behavior
`--force` overwrites unconditionally.

### Proposed change
Before writing, if the config file exists:
```rust
let backup_path = config_path.with_extension("toml.bak");
std::fs::copy(config_path, &backup_path)?;
println!("Backed up existing config to {}", backup_path.display());
```

This is a **simple single backup** (no versioning). If `.bak` already exists, overwrite it.

### Open question
- Should `--force` preserve the old `.bak` (overwrite `.bak` each time), or create timestamped backups like `.bak.20260507-143022`?

---

## Implementation order

1. Auto-exclude hidden dirs (`indexer.rs` + tests)
2. Backup logic (`init.rs`)
3. Interactive scan + prompt (`init.rs`)
4. Update config defaults (`config.rs`)
5. Update `docs/CLI-USE.md` and `docs/CLI-USE.ja.md`
6. Update `plans/plan-h2-init.md`

---

## Tests to add

| Test | Scope |
|------|-------|
| `test_hidden_dir_auto_excluded` | `core/src/indexer.rs` — vault with `.hidden/` containing `.md` files |
| `test_backup_created_on_force` | `cli/src/commands/init.rs` — force overwrite creates `.bak` |
| `test_init_scans_vault` | `cli/src/commands/init.rs` — mock vault with `node_modules/` and `templates/`; verify detected patterns appear in generated config |
