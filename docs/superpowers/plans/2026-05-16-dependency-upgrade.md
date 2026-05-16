# Dependency Upgrade Implementation Plan — Immediate

> **Status:** ✅ Completed. All 4 commits merged to `main`. See [deferred plan](./2026-05-16-dependency-upgrade-deferred.md) for sha2, thiserror, and notify upgrades.

**Goal:** Upgrade key Rust dependencies — focusing on highest-impact changes first. Remove dead deps, apply security patches, upgrade rusqlite (bundled SQLite / FTS5 engine).

**Branch:** `chore/upgrade-rusqlite-and-deps`

**Execution:** Sequential in a single session (no subagents). Each step is a separate commit after `cargo test` passes.

**Architecture:** RED (bump → build breaks) → GREEN (fix → tests pass) → REFACTOR (cleanup).

**Tech Stack:** Rust / Cargo, rusqlite + SQLite FTS5, sha2, thiserror, notify (watcher)

---

## Pre-flight: Actual Usage Audit

| Crate | Declared in | Actually imported in source? | Notes |
|-------|------------|------------------------------|-------|
| `rusqlite` | `core/Cargo.toml` | ✅ `core/src/db.rs` | Heavy use: `params![]`, `query_map`, `Connection`, `OpenFlags` |
| `sha2` | `core/Cargo.toml` | ✅ `core/src/indexer.rs`, `core/src/embedder.rs` | **Deferred to later plan** |
| `thiserror` | all three crates | ✅ `core/src/db.rs`, `embedder.rs`, etc. | **Deferred to later plan** |
| `notify` | `core/Cargo.toml` | ✅ `core/src/watcher.rs` | **Deferred to later plan (optional)** |
| `pulldown-cmark` | `core/Cargo.toml` | ❌ **not imported anywhere** | Remove now |
| `ndarray` | `core/Cargo.toml` | ❌ **not imported anywhere** | Remove now |

---

## Immediate Upgrade Tasks

| Task | Scope | Risk |
|------|-------|------|
| Task 0 | Benchmark baseline | — |
| Task 1 | Remove dead deps: pulldown-cmark, ndarray | Minimal |
| Task 2 | `cargo update` (patch bumps incl. rustls security) | Minimal |
| Task 5 | Upgrade rusqlite 0.31 → 0.39 | High (API changes) |

**Deferred (separate plan):** sha2 0.10→0.11, thiserror 1→2, notify 6→9.0.0-rc.4 (optional), post-benchmark.

---

## Files That Will Be Modified

| File | Why |
|------|-----|
| [core/Cargo.toml](core/Cargo.toml) | Remove pulldown-cmark & ndarray; bump rusqlite |
| `Cargo.lock` | Auto-updated by Cargo |
| [core/src/db.rs](core/src/db.rs) | rusqlite call-sites |

---

## TDD Approach for Dependency Upgrades

- **RED** = bump the version → `cargo build` breaks (compiler errors). This is the failing signal.
- **GREEN** = fix call-sites until `cargo test` passes with zero failures.
- **REFACTOR** = clean up any leftover compatibility shims.

The existing test suite is the test harness. **Never skip the RED verification step.**

---

## Task 0: Capture performance baseline

Criterion benchmarks exist in [core/benches/search_bench.rs](core/benches/search_bench.rs):
- `index_100_files` — indexes 100 Markdown files end-to-end
- `search_1000_notes` — FTS5 search over a 1000-note vault

- [x] **Step 1: Create output directory**

```bash
mkdir -p docs/perf
```

- [x] **Step 2: Run benchmarks and save output**

```bash
cargo bench -p shiotsuchi-core 2>&1 | tee docs/perf/baseline.txt
```

Expected: Criterion output with `index_100_files` and `search_1000_notes` timing.

- [x] **Step 3: Commit the baseline**

```bash
git add docs/perf/baseline.txt
git commit -m "perf: record benchmark baseline before dependency upgrades"
```

---

## Task 1: Remove unused direct dependencies (pulldown-cmark, ndarray)

- [x] **Step 1: Verify they are truly unused**

```bash
grep -rn "^use pulldown\|pulldown_cmark::\|^use ndarray\|ndarray::\|Array1\|Array2\|ArrayView" core/src/
```

Expected: **no output**.

- [x] **Step 2 (RED): Remove from Cargo.toml**

In [core/Cargo.toml](core/Cargo.toml), delete:
```toml
pulldown-cmark = "0.11"
ndarray = "0.15"
```

- [x] **Step 3: Build**

```bash
cargo build -p shiotsuchi-core 2>&1
```

Expected: builds cleanly.

- [x] **Step 4 (GREEN): Run full test suite**

```bash
make test
```

Expected: all tests pass.

- [x] **Step 5: Commit**

```bash
git add core/Cargo.toml Cargo.lock
git commit -m "chore(deps): remove unused direct deps pulldown-cmark and ndarray"
```

---

## Task 2: Patch-level updates (`cargo update`)

Semver-compatible patch bumps: `ruzstd` 0.8.2→0.8.3, `rustls` 0.23.31→0.23.40 (security), etc.

- [x] **Step 1 (RED): Run cargo update**

```bash
cargo update
```

Expected: `Updating …` lines. No errors.

- [x] **Step 2: Build**

```bash
make build
```

Expected: `Finished` with no errors.

- [x] **Step 3 (GREEN): Run full test suite**

```bash
make test
```

Expected: all tests pass.

- [x] **Step 4: Commit**

```bash
git add Cargo.lock
git commit -m "chore(deps): cargo update patch-level bumps (rustls security, ruzstd, tracing)"
```

---

## Pre-check: rusqlite CHANGELOG survey

Before bumping rusqlite, understand what changed across 0.31 → 0.39.

- [x] **Read what changed**

Fetched from https://github.com/rusqlite/rusqlite/releases (v0.32.0 through v0.39.0).

Key findings:
| Version | Breaking Change | Impact |
|---------|----------------|--------|
| 0.35.0 | `execute()` rejects multi-statement SQL | Low — single statements only |
| 0.38.0 | `u64`/`usize` ToSql/FromSql disabled by default | **Required `i64` cast in `stats()`** |
| 0.38.0 | Statement cache made optional | No impact (not used) |
| 0.39.0 | `sqlite3_auto_extension` expects `*mut *mut c_char` not `*mut *const c_char` | **Required FFI fix** |

- [x] **Document findings**

Breaking changes were within expected range. See `db.rs` commits for the two fixes.

---

## Task 5: Upgrade rusqlite 0.31 → 0.39

**Highest-impact upgrade.** 8 minor versions of API changes.

**Current call patterns in [core/src/db.rs](core/src/db.rs) to watch:**
- `use rusqlite::{params, Connection, OpenFlags, Result as SqliteResult}`
- `params![chunk.file_path, …]` — used in 6+ `execute()` calls
- `stmt.query_map(params![fts5_query, limit as i64], |r| …)`
- `stmt.query_map(params_vec.as_slice(), …)` — heterogeneous params
- `rusqlite::Error::QueryReturnedNoRows` match arm
- `rusqlite::ffi::sqlite3_auto_extension`

- [x] **Step 1: Record baseline test results**

Baseline: 84 unit tests + 7 integration tests all pass.

- [x] **Step 2 (RED): Bump version**

In [core/Cargo.toml](core/Cargo.toml):
```toml
rusqlite = { version = "0.39", features = ["bundled"] }
```

- [x] **Step 3: Build and list all errors**

5 errors found:
1. `sqlite3_auto_extension` FFI: `*mut *const c_char` → `*mut *mut c_char`
2–5. `usize: FromSql` not satisfied (4 occurrences in `stats()`)

Fixes applied:
- FFI: Changed function pointer type to `AutoExtFn` with `*mut *mut c_char`
- `usize` → `i64` for `COUNT(*)` and `page_count * page_size` retrievals, then cast to `usize` in struct

Note: `params![]` and `ToSql` still work unchanged — no migration needed.

- [x] **Step 4 (GREEN): Run core tests**

```bash
cargo test -p shiotsuchi-core 2>&1
```

84 unit + 7 integration = **91 pass, 0 fail**.

- [x] **Step 5: Run the full test suite**

Core 91 + CLI 81 + MCP 28 = **200 pass, 0 fail**.

- [x] **Step 6 (REFACTOR): Remove compatibility shims**

No shims needed — all fixes were direct.

- [x] **Step 7: Commit**

```bash
git add core/Cargo.toml core/src/db.rs Cargo.lock
git commit -m "chore(deps): upgrade rusqlite 0.31 → 0.39 (bundled SQLite, FTS5 perf)"
```

---

## Wrap-up

- [x] **Push branch**

```bash
git push -u origin chore/upgrade-rusqlite-and-deps
```

- [x] **Merge to main (no PR needed)**

```bash
git checkout main
git merge chore/upgrade-rusqlite-and-deps
git push
```

- [x] **Clean up branch**

```bash
git branch -d chore/upgrade-rusqlite-and-deps
```
