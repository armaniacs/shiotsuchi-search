# Coverage Gaps Audit — shiotsuchi-search

**Date:** 2026-05-25
**Codebase:** shiotsuchi-search (Rust, CLI + Core + MCP)
**Total tests:** 373 (CLI 109 + Core 216 + Integration 7 + E2E 16 + MCP 25)
**Production:Test ratio:** ~51:49 (5,233 prod / 5,005 test lines in core+cli)

---

## Executive Summary

**Score: 5.3/10**

The codebase has excellent overall test density (~50% test code), but has
targeted gaps in security-critical and core-flow paths. 2 CRITICAL and 1 HIGH
gaps exist around path traversal protections and MCP integration. All gaps are
actionable with S/M effort.

| Severity | Count | Points Lost |
|----------|-------|-------------|
| CRITICAL | 2 | -3.0 |
| HIGH     | 1 | -0.8 |
| MEDIUM   | 2 | -0.8 |
| LOW      | 1 | -0.1 |
| **Total** | **6** | **-4.7** |

---

## Coverage by Domain

### Security Flows (Priority 20+)

**Well-covered:**
- `watcher.rs` — symlink escape, path traversal, vault boundary (8 tests)
- `delete.rs` path validation via `canonicalize()` — **NOT TESTED**
- `util.rs::secure_parent_dir` — 4 tests
- DB/Config file `0o600` permissions — 4 tests across init, chart, db
- MCP `resolve_path_env` — 4 tests
- `mcp/src/handler.rs` `read_full_note` path traversal guard — **NOT TESTED**
- `clean.rs::delete_db_files` symlink refusal — 1 test
- `indexer.rs` symlink rejection — 2 tests

**Gaps:**

| # | File | Function | Priority | Severity | Issue |
|---|------|----------|----------|----------|-------|
| 1 | `cli/src/commands/delete.rs` | `run_delete` | 20+ | **CRITICAL** | Path traversal validation (absolute/`..`/canonicalize) and vault-aware file deletion has **zero** test coverage. A bug here could delete files outside the vault. |
| 2 | `mcp/src/handler.rs` | `search_local_notes` vault dir canonicalize check (line 116-119) | 20+ | **CRITICAL** | The `canonicalize()` check on the vault directory path in `search_local_notes` has **no dedicated test**. A non-canonicalizable or symlink-escaped vault dir could cause unexpected behavior. |

### Data Integrity (Priority 15+)

**Well-covered:**
- DB migrations (v1→v2→v3→v4) — 3 integration tests + 3 inline tests
- Transaction safety (`reindex_file`, `delete_chunks_for_file`, `upsert`) — integration test
- WAL checkpoint — 2 tests
- SHA-256 hash tracking — 6 tests
- File permissions — 4 tests
- Backup/restore patterns — 8 tests across clean, init, doctor

**Gaps:**

| # | File | Function | Priority | Severity | Issue |
|---|------|----------|----------|----------|-------|
| 3 | `cli/src/commands/config_migrate.rs` | `run_config_migrate` | 15+ | **MEDIUM** | The standalone `config-migrate` CLI command has no direct tests. Mitigation: the identical migration logic is tested via `doctor::tests::test_fix_old_vault_format_*`. The CLI wrapper (args parsing, messaging) remains untested. |

### Core User Journeys (Priority 15+)

**Well-covered:**
- `index_directory` (chart, clean, watcher) — 10+ tests
- `search_fts` / `search_vec` / `search_hybrid` — 12 tests
- `handle_event` (create/modify/remove/rename) — 5 tests
- Watcher setup — 1 test
- `cleanup_deleted` — 1 test

**Gaps:**

| # | File | Function | Priority | Severity | Issue |
|---|------|----------|----------|----------|-------|
| 4 | `mcp/src/handler.rs` | `call_tool("rebuild_index", ...)` routing | 15+ | **MEDIUM** | `rebuild_index` is dispatched in `main.rs` (line 329), not `handler.rs`. The handler's `call_tool` returns `Err("Unknown tool: rebuild_index")` — this edge case is tested via `test_unknown_tool_returns_error`. The actual rebuild logic IS tested in `main.rs::test_spawn_rebuild_indexes_vault`. Downgraded to MEDIUM. |
| 5 | `cli/src/commands/doctor.rs` | Full `run_doctor` interactive flow | 10+ | **MEDIUM** | The interactive prompt flow (`ask()`, `is_tty()`, user decision branches) is not automated. Mitigation: all fix helpers are unit-tested (13 tests). The prompt layer is a known architectural limitation (requires TTY). An E2E test for `doctor --help` or non-interactive diagnostics would close this gap. |

### Money Flows

Not applicable — this is a local CLI search tool with no financial
transactions.

---

## Detailed Findings

### Finding CRIT-1: `delete.rs` — Zero test coverage on security-critical path

**File:** `cli/src/commands/delete.rs` (52 lines)
**Test functions:** 0
**E2E coverage:** None

**Critical paths in this file:**
1. **Line 18:** `Path::new(path).is_absolute() || path.split('/').any(|c| c == "..")` — rejects traversal paths
2. **Lines 23-25:** Vault resolution — iterates vaults to find matching file
3. **Lines 38-44:** `canonicalize()` + `starts_with()` — symlink escape verification
4. **Lines 47-49:** DB cleanup — `delete_chunks_for_file` + `delete_file_cache`

**Attack scenario:** A malformed path passing the traversal check could delete
notes outside the intended vault. The vault resolution fallback (line 31) uses
`&vaults[0]` which panics on empty vaults.

**Suggested tests:**
- Unit: absolute path rejection (`/etc/passwd`)
- Unit: directory traversal rejection (`../../secret.md`)
- Unit: valid relative path within vault accepted
- Unit: file outside vault after canonicalize rejected
- Unit: empty vaults panic-safe guard
- Effort: **S** (single-file, deterministic)

### Finding CRIT-2: MCP `search_local_notes` vault dir canonicalize check untested

**File:** `mcp/src/handler.rs` lines 116-119
**Test functions:** 0 for this specific check

**Critical path:**
```rust
if let Some((_, notes_dir)) = vaults.first() {
    let _canonical_vault = notes_dir.canonicalize()?;
}
```

**Impact:** If the vault directory can't be canonicalized (non-existent, broken
symlink), the error propagates and the search fails. This path is not
specifically tested.

**Note:** `read_full_note` was removed from the MCP tool set in a previous
version. The only remaining path traversal check is the vault dir
canonicalization above.

**Suggested tests:**
- Unit: `search_local_notes` with non-existent vault dir returns error
- Unit: `search_local_notes` with symlink-escaped vault dir returns error
- Effort: **S** (temp-dir based)

### Finding HIGH-1: MCP `rebuild_index` tool untested

**File:** `mcp/src/handler.rs`
**Test functions:** 0

**Critical path:**
The `rebuild_index` tool spawns a background task via `tokio::spawn`, calls
`index_directory` with progress reporting, and handles `IndexProgress`
callbacks. This is a complex async flow with:
- Background task lifecycle
- Progress notification serialization to stdout
- Error recovery (what happens if index_directory fails mid-way?)

**Suggested tests:**
- Unit: `call_tool("rebuild_index", ...)` returns immediate acknowledgment
- Integration: verify that rebuild actually re-indexes when run synchronously
- Effort: **M** (requires tokio test runtime)

### Finding MED-1: `config_migrate.rs` CLI wrapper untested

**File:** `cli/src/commands/config_migrate.rs` (82 lines)
**Test functions:** 0

**Mitigation:** The migration logic is tested indirectly through
`doctor::tests::test_fix_old_vault_format_*` (2 tests). What's missing:
- `--config` flag parsing
- "file not found" early return branch
- "already new format" early return branch
- Permission-setting fallback on non-Unix

**Suggested tests:**
- Unit: nonexistent config path produces no error
- Unit: already-new-format config produces no migration
- Effort: **S**

### Finding MED-2: Doctor E2E flow not covered

**File:** `cli/src/commands/doctor.rs`
**Test functions:** 13 (all fix helpers)
**E2E coverage:** None

**Mitigation:** All fix helpers (`fix_config_unknown_fields`,
`fix_config_old_vault_format`, `index_vault`, `rebuild_db`,
`backup_config_file`, `find_unknown_indexing_fields`) have unit tests.

**What's missing:**
- `shiotsuchi doctor --help` parses correctly
- `shiotsuchi doctor` runs in non-TTY mode (diagnose only, no prompts)
- TTY detection logic

**Suggested tests:**
- Unit: `is_tty()` returns false when stdin is not a terminal
- E2E: `shiotsuchi doctor` exits successfully with readable output
- Effort: **S**

### Finding LOW-1: Interactive prompt layer not tested

**File:** `cli/src/commands/doctor.rs` (`ask()`, `is_tty()`)
**Test functions:** 0 for interactive flow

**Architectural limitation:** `dialoguer::Confirm::interact()` requires a real
TTY and cannot be mocked. This is consistent with `init.rs` which has the same
limitation. Tested manually.

---

## Recommendations (Priority Order)

| Priority | Action | Target | Effort | Status |
|----------|--------|--------|--------|--------|
| P0 | Add delete.rs unit tests (path traversal, vault resolution, canonicalize) | `cli/src/commands/delete.rs` | S | ✅ Done |
| P0 | Add MCP `search_local_notes` vault dir canonicalize test | `mcp/src/handler.rs` | S | Doing |
| P1 | Add MCP `rebuild_index` routing test already exists in `main.rs` | `mcp/src/main.rs` | S | ✅ Already tested |
| P2 | Add config_migrate CLI tests | `cli/src/commands/config_migrate.rs` | S | Pending |
| P2 | Add doctor E2E smoke test | `e2e/src/lib.rs` | S | Pending |

---

## Methodology

- **Scan scope:** `core/src/`, `cli/src/`, `mcp/src/`, `e2e/src/`
- **Tooling:** Read/Grep/Glob/Bash (MCP unavailable for this workspace)
- **Layer 1:** Keyword-based scan for security, data, and core-flow patterns
  across all production code, followed by test function matching
- **Layer 2:** Context analysis for each gap candidate — checked E2E coverage,
  helper <10 line rule, and false positive classification
- **False positive note:** `paymentIcon()`-style false positives do not apply
  (no UI code in this project)
- **Downgrades applied:**
  - `config_migrate.rs`: from HIGH→MEDIUM (logic tested via doctor.rs)
  - Doctor interactive prompts: from MEDIUM→LOW (architectural limitation)
