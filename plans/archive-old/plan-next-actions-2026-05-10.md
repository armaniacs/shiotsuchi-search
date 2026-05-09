# Plan: Next Actions After Code Review (Completed)

**Date:** 2026-05-10  
**Branch:** `modify-2026-05-09a`  
**Previous Review:** `plan-checking-team-2026-05-09a.md`

---

## Executive Summary

The Checking Team review identified 8 issues (2 High, 3 Medium, 3 Low). Per `plan-checking-team-2026-05-09a.md`, Phases 1-6 were marked **complete** in committed code. This plan addressed the remaining items plus additional issues discovered during implementation.

**All items are now complete.**

| Issue | Status | Resolution |
|-------|--------|-----------|
| High #1: WAL/SHM 0o600 | ✅ Complete | `core/src/db.rs:32-51` |
| High #2: Parent dir 0o700 | ✅ Complete | `chart.rs:36-50`, `scan.rs:31-43` |
| Medium #3: Backup cleanup | ✅ Complete | `init.rs:166-170` |
| Medium #4: Hidden deprecated flags | ✅ Complete | `chart.rs:13-15`, `scan.rs:12-14` |
| Medium #5: MCP error handling | ✅ Complete | `mcp/src/main.rs:126-127` |
| Low #6: Docs --verbose | ✅ Complete | `CLI-USE.md:21`, `CLI-USE.ja.md:21` |
| Low #7: Docs delete command | ✅ Complete | `CLI-USE.md:77-86` |
| Low #8: debounce_ms audit | ✅ Complete | Removed from `WatcherConfig` + all docs |

---

## Decision Log

| Question | Answer |
|----------|--------|
| 1. MCP path validation scope | Accept `..`-free relative paths |
| 2. Security error handling | Print warning + fallback |
| 3. debounce_ms removal | Remove entirely + update docs |
| 4. Permission utility location | `cli/src/util.rs` |
| 5. Test strategy | Delete existing test |

---

## Actions Implemented

### Action 1: MCP Path Validation [Medium]

**Files:** `mcp/src/main.rs`

**Summary:** Added `resolve_path_env()` that validates environment variables `SHIOTSUCHI_NOTES_DIR` and `SHIOTSUCHI_DB_PATH` — rejects relative paths containing `..`, falls back to config default with stderr warning.

**Tests (6 new):**
- `test_resolve_path_env_uses_env_var_when_set`
- `test_resolve_path_env_falls_back_when_unset`
- `test_resolve_path_env_rejects_dotdot_traversal`
- `test_resolve_path_env_rejects_multiple_dotdot_traversal`
- `test_resolve_path_env_accepts_relative_path_without_dotdot`
- `test_resolve_path_env_accepts_absolute_path_with_dotdot`
- `test_resolve_path_env_falls_back_on_empty_var`

---

### Action 2: Permission Utility [Low]

**Files:** `cli/src/util.rs` (new), `cli/src/main.rs`, `cli/src/commands/chart.rs`, `cli/src/commands/scan.rs`

**Summary:** Extracted `secure_parent_dir()` to eliminate duplicate permission-setting code. Both `chart` and `scan` now call `crate::util::secure_parent_dir(db_path)`.

**Tests (4 new):**
- `test_secure_parent_dir_creates_with_0700`
- `test_secure_parent_dir_preserves_existing_0700`
- `test_secure_parent_dir_handles_nonexistent_parent`
- `test_secure_parent_dir_noop_without_parent`

---

### Action 3: Remove debounce_ms Config [Low]

**Files:** `cli/src/config.rs`, `docs/CLI-USE.md`, `docs/CLI-USE.ja.md`, `docs/INSTALL.md`, `docs/INSTALL.ja.md`, `ref/cli.md`

**Summary:** Removed unused `debounce_ms` field from `WatcherConfig`. Updated all documentation config examples. Updated scan deprecation message.

**Tests changed:** Existing `test_watcher_config_default_debounce_is_500ms` removed (field no longer exists).

---

### Action 4: Makefile Comment [Low]

**File:** `Makefile`

**Summary:** Added comment `# test-all requires Docker/Act installed for the local-ci target` above the `test-all` target.

---

### Action 5: Parent Directory Permission Tests [Medium]

**Files:** `cli/src/commands/chart.rs`, `cli/src/commands/scan.rs`

**Tests (3 new):**
- `test_chart_creates_parent_dir_with_0700`
- `test_chart_parent_dir_0700_with_nested_path`
- `test_scan_parent_dir_0700_via_utility`

---

## Additional Work (Not in Original Plan)

### A: exclude_patterns → exclude_dirs Documentation Fix

**Files:** `README.md`, `README.ja.md`, `docs/INSTALL.md`, `docs/INSTALL.ja.md`, `ref/core.md`, `ref/models.md`

**Summary:** Six documentation files still referenced the old field name `exclude_patterns` or had incomplete struct descriptions. Updated all to `exclude_dirs` with current default values.

**Test added:** `doc_consistency_tests::test_index_config_uses_exclude_dirs` — compile-time guard ensuring field name is up to date.

### B: CLI Global Flag Tests

**Files:** `cli/src/main.rs`

**Summary:** Verified that `global = true` on `--notes-dir`, `--db-path`, and `--verbose` works correctly on all subcommands. Added proper tests replacing the no-op placeholder.

**Tests (11 new):**
- `test_global_notes_dir_on_dive_subcommand`
- `test_global_db_path_on_dive_subcommand`
- `test_global_verbose_on_tide_subcommand`
- `test_global_flag_before_subcommand_position`
- `test_global_db_path_on_scan_subcommand`
- `test_global_notes_dir_on_top_level`
- `test_global_flags_accepted_on_all_subcommands`
- `test_env_var_mapped_notes_dir`
- `test_help_does_not_panic`
- `test_version_flag_compiles` (preserved from before)
- `doc_consistency_tests::test_index_config_uses_exclude_dirs`

### C: Version Bump & Changelog

**Files:** `Cargo.toml`, `CHANGELOG.md`

**Summary:** Bumped version from `0.3.0` → `0.3.1`. Added v0.3.1 changelog entries for all changes.

---

## Changed Files (12 + 2 untracked)

| File | Change |
|------|--------|
| `.github/workflows/ci.yml` | CI: `if: !env.ACT` guards |
| `Makefile` | Comment on test-all Docker dependency |
| `Cargo.toml` | Version 0.3.0 → 0.3.1 |
| `CHANGELOG.md` | Added v0.3.1 section |
| `mcp/src/main.rs` | `resolve_path_env()` + 7 tests |
| `cli/src/main.rs` | `mod util`, 11 global flag tests |
| `cli/src/util.rs` | **NEW** — `secure_parent_dir()` |
| `cli/src/config.rs` | Removed `debounce_ms` from `WatcherConfig` |
| `cli/src/commands/chart.rs` | Use `util::secure_parent_dir`, + 2 tests |
| `cli/src/commands/scan.rs` | Use `util::secure_parent_dir`, + 1 test |
| `ref/cli.md` | Removed `debounce_ms` table row |
| `ref/core.md` | Updated `IndexConfig` fields |
| `ref/models.md` | Updated `IndexConfig` struct definition |
| `README.md`, `README.ja.md` | `exclude_patterns` → `exclude_dirs` |
| `docs/INSTALL.md`, `docs/INSTALL.ja.md` | `exclude_patterns` → `exclude_dirs`, removed `debounce_ms` |
| `docs/CLI-USE.md`, `docs/CLI-USE.ja.md` | Removed `debounce_ms` from config example |
| `plans/plan-next-actions-2026-05-10.md` | This file — completion summary |

---

## Implementation Verification

```
test result: ok. 156 passed; 0 failed (across 8 test binaries)
cargo fmt --all --check  → clean
cargo clippy --workspace --exclude shiotsuchi-e2e -- -D warnings → clean
$ cargo run -p shiotsuchi -- --version
shiotsuchi 0.3.1
```

---

## Suggested Next Steps

1. **Commit** the uncommitted changes to branch `modify-2026-05-09a`
2. **Merge** to `main` (or create PR)
3. **Archive** completed plan files (`plan-next-actions-2026-05-10.md`, `plan-checking-team-2026-05-09a.md`) to `plans/archive/`
4. **Future considerations:**
   - `WatcherConfig::enabled` field could also be removed if the watcher is always enabled when scan runs
   - `model` dependency in `make test-all` could be documented more visibly if users hit Docker/Act issues

---

## Defer / Won't Fix

| Item | Reason |
|------|--------|
| TOCTOU DB creation race | Mitigation via parent dir 0o700 is sufficient for local CLI tool |
| error! level for permissions | warn! is acceptable per Compliance & Privacy guidance |
| VaultWatcher debounce configurability | No user demand; 500ms default works well |
