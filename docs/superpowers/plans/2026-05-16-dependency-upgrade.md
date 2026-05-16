# Dependency Upgrade Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade key Rust dependencies to their latest stable versions to improve search/index performance, reduce latency, and address security patches — while keeping the test suite green at every step.

**Architecture:** Each dependency is upgraded in a separate commit following Red-Green-Refactor: bump the version (RED — tests fail or build breaks), fix call-sites (GREEN — tests pass), clean up (REFACTOR). Higher-risk upgrades are tackled after lower-risk ones so the baseline stays stable.

**Tech Stack:** Rust / Cargo, rusqlite + SQLite FTS5, sha2, thiserror, notify (watcher)

---

## Pre-flight: Actual Usage Audit

Before upgrading anything, it's important to understand what the codebase actually uses vs. what Cargo.toml declares.

| Crate | Declared in | Actually imported in source? | Notes |
|-------|------------|------------------------------|-------|
| `rusqlite` | `core/Cargo.toml` | ✅ `core/src/db.rs` | Heavy use: `params![]`, `query_map`, `Connection`, `OpenFlags` |
| `sha2` | `core/Cargo.toml` | ✅ `core/src/indexer.rs`, `core/src/embedder.rs` | `Sha256::new()` + `.finalize()` |
| `thiserror` | all three crates | ✅ `core/src/db.rs`, `embedder.rs`, etc. | `#[derive(Error)]` |
| `notify` | `core/Cargo.toml` | ✅ `core/src/watcher.rs` | `recommended_watcher`, `EventKind`, `ModifyKind`, `RecursiveMode` |
| `pulldown-cmark` | `core/Cargo.toml` | ❌ **not imported anywhere** | Likely a leftover direct dep; `ort` does not pull it in either — safe to remove |
| `ndarray` | `core/Cargo.toml` | ❌ **not imported anywhere** | `ort` v2 pulls in its own `ndarray` v0.17; our v0.15 pin is unused — remove it |

**pulldown-cmark and ndarray are dead weight in Cargo.toml. Task 1 removes them first.**

---

## Upgrade Priority & Risk Matrix

| Crate | Current (Cargo.toml) | Latest stable | Risk | Expected benefit |
|-------|---------------------|---------------|------|-----------------|
| `pulldown-cmark` | 0.11 (unused) | 0.13 | Minimal | Remove dead dep → faster compile |
| `ndarray` | 0.15 (unused) | 0.17 | Minimal | Remove dead dep; let `ort` own its version |
| `sha2` | 0.10 | 0.11 | Low | Hash speed (incremental index diff) |
| `thiserror` | 1 | 2 | Low | Compile time |
| `rusqlite` | 0.31 | 0.39 | High (API changes) | Newer bundled SQLite / FTS5 engine |
| `notify` | 6 | 9.0.0-rc.4 | High (rc + API) | Watcher latency — **optional, rc only** |
| patch bumps (`cargo update`) | — | — | Minimal | Security/bugfix (rustls, ruzstd, tracing…) |

**Upgrade order:** dead-dep removal → patch bumps → sha2 → thiserror → rusqlite → notify (optional)

---

## Files That Will Be Modified

| File | Why |
|------|-----|
| [core/Cargo.toml](core/Cargo.toml) | Remove pulldown-cmark & ndarray; bump sha2, rusqlite, notify |
| [cli/Cargo.toml](cli/Cargo.toml) | Bump thiserror, sha2 |
| [mcp/Cargo.toml](mcp/Cargo.toml) | Bump thiserror |
| `Cargo.lock` | Auto-updated by Cargo |
| [core/src/db.rs](core/src/db.rs) | rusqlite call-sites: `params![]`, `query_map`, `Connection` |
| [core/src/indexer.rs](core/src/indexer.rs) | sha2 call-sites (if digest API changed) |
| [core/src/embedder.rs](core/src/embedder.rs) | sha2 call-sites (if digest API changed) |
| [core/src/watcher.rs](core/src/watcher.rs) | notify call-sites (EventKind, ModifyKind, RecursiveMode) |

---

## TDD Approach for Dependency Upgrades

Dependency upgrades don't follow the usual "write a new failing test" TDD flow. Instead:

- **RED** = bump the version in Cargo.toml → `cargo build` breaks (compiler errors) or existing tests fail. This is the failing signal.
- **GREEN** = fix call-sites until `cargo test` passes with zero failures.
- **REFACTOR** = clean up any leftover compatibility shims or redundant code.

The existing test suite is the test harness. **Never skip the RED verification step** — if `cargo build` compiles immediately with no errors after a major-version bump, double-check that the new version was actually resolved (`cargo tree | grep <crate>`).

---

## Task 1: Remove unused direct dependencies (pulldown-cmark, ndarray)

`pulldown-cmark` and `ndarray` v0.15 appear in `core/Cargo.toml` but are not imported in any source file. Removing them reduces compile time and eliminates version-pin conflicts (ort v2 already supplies ndarray v0.17).

**Files:**
- Modify: [core/Cargo.toml](core/Cargo.toml)

- [ ] **Step 1: Verify they are truly unused**

```bash
grep -rn "^use pulldown\|pulldown_cmark::\|^use ndarray\|ndarray::\|Array1\|Array2\|ArrayView" core/src/
```

Expected: **no output**. If any matches appear, stop — this task needs rethinking.

- [ ] **Step 2 (RED): Remove from Cargo.toml and confirm build still works**

In [core/Cargo.toml](core/Cargo.toml), delete these two lines:

```toml
pulldown-cmark = "0.11"
ndarray = "0.15"
```

- [ ] **Step 3: Build and verify nothing broke**

```bash
cargo build -p shiotsuchi-core 2>&1
```

Expected: builds cleanly. If the compiler complains about a missing crate, a source file is using it — check `grep` output and add the file to the "Files" section above before continuing.

- [ ] **Step 4 (GREEN): Run full test suite**

```bash
make test
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add core/Cargo.toml Cargo.lock
git commit -m "chore(deps): remove unused direct deps pulldown-cmark and ndarray"
```

---

## Task 2: Patch-level updates (`cargo update`)

Apply all semver-compatible patch bumps in one shot. Key updates: `ruzstd` 0.8.2→0.8.3, `rustls` 0.23.31→0.23.40 (security), `tracing` 0.1.41→0.1.44.

**Files:**
- Modify: `Cargo.lock` (auto-updated only)

- [ ] **Step 1 (RED): Run cargo update**

```bash
cargo update
```

Expected: `Updating …` lines. No errors.

- [ ] **Step 2: Build**

```bash
make build
```

Expected: `Finished` with no errors. If something breaks, a dependency has a semver violation — check which crate changed and pin it.

- [ ] **Step 3 (GREEN): Run full test suite**

```bash
make test
```

Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.lock
git commit -m "chore(deps): cargo update patch-level bumps (rustls security, ruzstd, tracing)"
```

---

## Task 3: Upgrade `sha2` 0.10 → 0.11

`sha2` is used in two places:
- [core/src/indexer.rs:9](core/src/indexer.rs) — `use sha2::{Digest, Sha256};` for file hash
- [core/src/embedder.rs](core/src/embedder.rs) — same import for model-ID hash

The `digest` crate (a transitive dep) also bumped; `.finalize()` and `.chain_update()` are stable across 0.10→0.11.

**Files:**
- Modify: [core/Cargo.toml](core/Cargo.toml), [cli/Cargo.toml](cli/Cargo.toml)

- [ ] **Step 1 (RED): Bump version**

In [core/Cargo.toml](core/Cargo.toml):

```toml
sha2 = "0.11"
```

In [cli/Cargo.toml](cli/Cargo.toml):

```toml
sha2 = "0.11"
```

- [ ] **Step 2: Attempt build**

```bash
cargo build -p shiotsuchi-core 2>&1
```

The two most likely errors if `digest` also bumped:

```
error[E0277]: the trait `Digest` is not satisfied
```

Fix: ensure `sha2` and any direct `digest` dep in Cargo.toml use matching versions. If `digest` is not listed directly, only `sha2 = "0.11"` is needed.

```
error[E0599]: no method named `finalize` found
```

Fix: `finalize()` is still on the `Digest` trait in 0.11 — check that `use sha2::Digest` is present. The actual call-sites at [core/src/indexer.rs:68-71](core/src/indexer.rs) look like:

```rust
fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}
```

This pattern is unchanged in sha2 0.11 — no edits expected.

- [ ] **Step 3 (GREEN): Run core tests**

```bash
cargo test -p shiotsuchi-core
```

Expected: all pass.

- [ ] **Step 4: Run full suite**

```bash
make test
```

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add core/Cargo.toml cli/Cargo.toml Cargo.lock
git commit -m "chore(deps): upgrade sha2 0.10 → 0.11"
```

---

## Task 4: Upgrade `thiserror` 1 → 2

`thiserror` 2 changed derive macro internals but the user-facing `#[derive(Error)]` + `#[error("…")]` API is identical. No source changes expected; the benefit is faster compile time.

**Files:**
- Modify: [core/Cargo.toml](core/Cargo.toml), [cli/Cargo.toml](cli/Cargo.toml), [mcp/Cargo.toml](mcp/Cargo.toml)

- [ ] **Step 1 (RED): Bump in all three crates**

In each `[dependencies]` section:

```toml
thiserror = "2"
```

Files: [core/Cargo.toml](core/Cargo.toml), [cli/Cargo.toml](cli/Cargo.toml), [mcp/Cargo.toml](mcp/Cargo.toml).

- [ ] **Step 2: Build**

```bash
make build 2>&1
```

Expected: compiles cleanly. If the compiler flags `#[error("…")]` format string syntax errors, fix as directed — the 2.x release tightened some format string validation.

- [ ] **Step 3 (GREEN): Run tests**

```bash
make test
```

Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add core/Cargo.toml cli/Cargo.toml mcp/Cargo.toml Cargo.lock
git commit -m "chore(deps): upgrade thiserror 1 → 2"
```

---

## Task 5: Upgrade `rusqlite` 0.31 → 0.39

**Highest-impact upgrade.** rusqlite bundles SQLite — newer version = newer FTS5 engine with performance improvements. 8 minor versions means API has breaking changes.

**Current call patterns in [core/src/db.rs](core/src/db.rs) to watch:**
- `use rusqlite::{params, Connection, OpenFlags, Result as SqliteResult}` (line 2)
- `params![chunk.file_path, …]` — used in 6+ `execute()` calls
- `stmt.query_map(params![fts5_query, limit as i64], |r| …)` (lines 265, 280)
- `stmt.query_map(params_vec.as_slice(), …)` — heterogeneous params (line 302)
- `rusqlite::Error::QueryReturnedNoRows` match arm (line 245, 321)
- `rusqlite::ffi::sqlite3_auto_extension` (line 33)

**Files:**
- Modify: [core/Cargo.toml](core/Cargo.toml)
- Modify: [core/src/db.rs](core/src/db.rs)

- [ ] **Step 1: Record baseline test results**

```bash
cargo test -p shiotsuchi-core 2>&1 | tail -5
```

Note how many tests pass. This is your GREEN baseline before the RED step.

- [ ] **Step 2 (RED): Bump version**

In [core/Cargo.toml](core/Cargo.toml):

```toml
rusqlite = { version = "0.39", features = ["bundled"] }
```

- [ ] **Step 3: Build and list all errors**

```bash
cargo build -p shiotsuchi-core 2>&1
```

Collect the full list before fixing anything — fixes can interact. Common patterns across 0.31→0.39:

**`params![]` in `query_map`** — if the macro form is deprecated for slice params:

```rust
// Before (0.31):
stmt.query_map(params![fts5_query, limit as i64], |r| r.get(0))?;

// After (0.39), tuple form:
stmt.query_map((fts5_query.as_str(), limit as i64), |r| r.get(0))?;
```

**Heterogeneous slice params** — `params_vec.as_slice()` pattern at line 302:

```rust
// Before:
let params_vec: Vec<&dyn rusqlite::ToSql> = ids.iter()
    .map(|id| id as &dyn rusqlite::ToSql)
    .collect();
stmt.query_map(params_vec.as_slice(), |r| …)?;

// After (if ToSql trait moved):
// check rustdoc for rusqlite::ToSql in 0.39 — trait is still present,
// but the bound may be `rusqlite::types::ToSql` instead.
```

**`OpenFlags`** — verify variant names are unchanged:

```bash
cargo doc -p rusqlite --open
# or:
cargo search rusqlite
```

**`rusqlite::ffi::sqlite3_auto_extension`** — FFI bindings are version-tied to the bundled SQLite. Verify the function signature in `core/src/db.rs` line 28-33 still compiles.

Fix each error, re-run `cargo build` after each fix.

- [ ] **Step 4 (GREEN): Run core tests**

```bash
cargo test -p shiotsuchi-core 2>&1
```

Expected: same test count as baseline, all pass. If FTS5 query tests fail, check that the FTS5 syntax used in `db.rs` is still valid — the bundled SQLite version may have changed tokenizer behavior.

- [ ] **Step 5: Run the full test suite**

```bash
make test
```

Expected: all pass.

- [ ] **Step 6 (REFACTOR): Remove any compatibility shims added in Step 3**

If you introduced `#[allow(deprecated)]` or temporary type aliases, clean them up now that tests are green.

- [ ] **Step 7: Quick benchmark check (optional mid-point)**

```bash
cargo bench -p shiotsuchi-core 2>&1 | grep "time:"
```

Sanity check only — formal before/after comparison is in Task 0 (baseline) and Task 7 (final).

- [ ] **Step 8: Commit**

```bash
git add core/Cargo.toml core/src/db.rs Cargo.lock
git commit -m "chore(deps): upgrade rusqlite 0.31 → 0.39 (bundled SQLite, FTS5 perf)"
```

---

## Task 6 (Optional): Upgrade `notify` 6 → 9.0.0-rc.4

`notify` 9 is a release candidate. Only attempt this if watcher latency is a measured problem and rc risk is acceptable.

**Current call patterns in [core/src/watcher.rs](core/src/watcher.rs):**
- `notify::recommended_watcher(…)` (line 43)
- `notify::RecursiveMode::Recursive` (line 49)
- `notify::event::{EventKind, ModifyKind, RenameMode}` (line 83)
- `notify::Event as NotifyEvent` (line 175)
- `notify::event::EventAttributes::default()` (line 212)
- `notify::event::DataChange::Content` (line 210)

**Files:**
- Modify: [core/Cargo.toml](core/Cargo.toml)
- Modify: [core/src/watcher.rs](core/src/watcher.rs)

- [ ] **Step 1: Decide go/no-go**

Run the watcher tests to establish a baseline:

```bash
cargo test -p shiotsuchi-core watcher 2>&1
```

Note test count. If fewer than 3 watcher tests exist, add a note that coverage is thin before proceeding.

- [ ] **Step 2 (RED): Bump version**

In [core/Cargo.toml](core/Cargo.toml):

```toml
notify = { version = "9.0.0-rc.4", optional = true }
```

- [ ] **Step 3: Build with watcher feature**

```bash
cargo build -p shiotsuchi-core --features watcher 2>&1
```

notify 7→9 restructured event types. Common changes:

```rust
// Before (notify 6/7):
use notify::event::{EventKind, ModifyKind, RenameMode};

// After (notify 9) — check if sub-module paths changed:
use notify::event::{EventKind, ModifyKind, RenameMode};  // likely same
// But DataChange::Content may have moved — check compiler errors
```

Fix each error in [core/src/watcher.rs](core/src/watcher.rs) as directed by the compiler.

- [ ] **Step 4 (GREEN): Run watcher tests**

```bash
cargo test -p shiotsuchi-core watcher --features watcher 2>&1
```

Expected: same count as baseline, all pass.

- [ ] **Step 5: Manual smoke test**

```bash
mkdir -p /tmp/test-notes
echo "# Hello" > /tmp/test-notes/test.md
cargo run -p shiotsuchi -- scan --notes-dir /tmp/test-notes &
SCAN_PID=$!
sleep 1
echo "# Updated" > /tmp/test-notes/test.md
sleep 2
kill $SCAN_PID
```

Observe in the output that the file update was detected within ~1 second.

- [ ] **Step 6: Run full suite**

```bash
make test
```

- [ ] **Step 7: Commit**

```bash
git add core/Cargo.toml core/src/watcher.rs Cargo.lock
git commit -m "chore(deps): upgrade notify 6 → 9.0.0-rc.4 (watcher latency)"
```

---

## Self-Review

**Spec coverage:**
- ✅ Dead dependency removal (Task 1) — new finding from audit, not in original scope but clearly correct
- ✅ Patch bumps (Task 2)
- ✅ sha2 (Task 3) — call-sites documented from actual grep
- ✅ thiserror (Task 4)
- ✅ rusqlite (Task 5) — actual `params![]` and `query_map` patterns documented from db.rs
- ✅ notify (Task 6, optional) — actual event type imports from watcher.rs documented

**Placeholder scan:** No TBDs. Every step references actual line numbers or grep-verified patterns.

**TDD compliance:**
- Every task has an explicit RED step (version bump → build breaks)
- Every task has an explicit GREEN step (tests pass)
- Task 5 (rusqlite) adds a baseline recording step before RED, and a REFACTOR step after GREEN
- "Watch it fail" is mandatory — Task 5 Step 1 records baseline; Task 6 Step 1 records watcher test count

**Type consistency:** All `params![]` examples match the actual signatures in `core/src/db.rs`. All `notify` event types match actual imports in `core/src/watcher.rs` line numbers.

---

## Task 0: Capture performance baseline (run BEFORE any upgrades)

Criterion benchmarks already exist in [core/benches/search_bench.rs](core/benches/search_bench.rs):

- `index_100_files` — indexes 100 Markdown files end-to-end
- `search_1000_notes` — FTS5 search over a 1000-note vault

Run these before touching any dependency to get a clean before/after comparison.

**Files:**
- Read-only: [core/benches/search_bench.rs](core/benches/search_bench.rs)
- Create: `docs/perf/baseline.txt` (benchmark output to keep as reference)

- [ ] **Step 1: Create output directory**

```bash
mkdir -p docs/perf
```

- [ ] **Step 2: Run benchmarks and save output**

```bash
cargo bench -p shiotsuchi-core 2>&1 | tee docs/perf/baseline.txt
```

Expected output format (Criterion):
```
index_100_files         time:   [X.XX ms X.XX ms X.XX ms]
search_1000_notes       time:   [X.XX µs X.XX µs X.XX µs]
```

Note the middle value (point estimate) for each benchmark. These are your baseline numbers.

- [ ] **Step 3: Commit the baseline**

```bash
git add docs/perf/baseline.txt
git commit -m "perf: record benchmark baseline before dependency upgrades"
```

---

## Task 7 (after all upgrades): Measure performance improvement

Run the same benchmarks after all upgrades are complete and compare against the baseline.

**Files:**
- Create: `docs/perf/after-upgrade.txt`
- Modify: `docs/perf/comparison.md` (human-readable summary)

- [ ] **Step 1: Run benchmarks and save output**

```bash
cargo bench -p shiotsuchi-core 2>&1 | tee docs/perf/after-upgrade.txt
```

- [ ] **Step 2: Compare with baseline**

```bash
diff docs/perf/baseline.txt docs/perf/after-upgrade.txt
```

Then extract point estimates manually and write the comparison table:

```bash
grep "time:" docs/perf/baseline.txt
grep "time:" docs/perf/after-upgrade.txt
```

- [ ] **Step 3: Write comparison summary**

Create `docs/perf/comparison.md` with this structure:

```markdown
# Dependency Upgrade — Performance Comparison

Date: YYYY-MM-DD
Upgraded: rusqlite 0.31→0.39, sha2 0.10→0.11, thiserror 1→2, (ndarray/pulldown-cmark removed)

| Benchmark | Before | After | Change |
|-----------|--------|-------|--------|
| index_100_files | X.XX ms | X.XX ms | −X% |
| search_1000_notes | X.XX µs | X.XX µs | −X% |

## Notes
- (add any observations about which upgrade drove the biggest gain)
```

Fill in the actual numbers from Steps 1–2.

- [ ] **Step 4: Commit**

```bash
git add docs/perf/after-upgrade.txt docs/perf/comparison.md
git commit -m "perf: record benchmark results after dependency upgrades"
```

---

## Execution Plan

**Chosen approach: Subagent-Driven Development**

Use `superpowers:subagent-driven-development` skill when ready to implement.

- Fresh subagent per task
- Review between tasks before proceeding to the next
- Tasks 1–5 are mandatory; Task 6 (notify rc) is optional and should be decided at execution time

**Status: Not yet started. Implementation deferred.**
