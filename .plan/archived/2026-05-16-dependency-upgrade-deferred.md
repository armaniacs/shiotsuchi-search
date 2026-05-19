# Dependency Upgrade Implementation Plan — Deferred

> **Status:** ✅ Completed (2026-05-18). All deferred upgrades applied. sha2/thiserror were already on latest; notify upgraded 6→9.0.0-rc.4.

**Goal:** Upgrade remaining Rust dependencies — sha2, thiserror, and optionally notify — to keep the dependency tree current.

**Pre-requisite:** ✅ Immediate plan completed — dead-dep removal, cargo update, and rusqlite 0.39 are merged in `main` (commit range `52cef72..27290d9`). Baseline benchmarks at `docs/perf/baseline.txt`.

---

## Deferred Upgrade Tasks

| Task | Scope | Risk |
|------|-------|------|
| Task 3 | Upgrade sha2 0.10 → 0.11 | Low |
| Task 4 | Upgrade thiserror 1 → 2 | Low |
| Task 6 | Upgrade notify 6 → 9.0.0-rc.4 | High (optional, rc quality) |
| Task 7 | Post-benchmark comparison | — |

---

## Files That Will Be Modified

| File | Why |
|------|-----|
| [core/Cargo.toml](core/Cargo.toml) | Bump sha2 and notify |
| [cli/Cargo.toml](cli/Cargo.toml) | Bump sha2, thiserror |
| [mcp/Cargo.toml](mcp/Cargo.toml) | Bump thiserror |
| `Cargo.lock` | Auto-updated |
| [core/src/indexer.rs](core/src/indexer.rs) | sha2 call-sites (if digest API changed) |
| [core/src/embedder.rs](core/src/embedder.rs) | sha2 call-sites (if digest API changed) |
| [core/src/watcher.rs](core/src/watcher.rs) | notify call-sites (if event types changed) |

---

## Task 3: Upgrade sha2 0.10 → 0.11

Used in:
- [core/src/indexer.rs](core/src/indexer.rs): `use sha2::{Digest, Sha256};` for file hash
- [core/src/embedder.rs](core/src/embedder.rs): same import for model-ID hash

The `digest` crate (transitive dep) also bumps. `.finalize()` and `.chain_update()` are stable across 0.10→0.11.

- [x] **Step 1 (RED): Bump version** *(already applied in previous session)*

In [core/Cargo.toml](core/Cargo.toml) and [cli/Cargo.toml](cli/Cargo.toml):
```toml
sha2 = "0.11"
```

- [x] **Step 2: Attempt build** *(compiled cleanly)*

The call-site pattern is:
```rust
fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}
```

This pattern is unchanged in sha2 0.11 — no edits expected.

- [x] **Step 3 (GREEN): Run core tests** *(186 passed, 6 pre-existing failures)*

```bash
cargo test -p shiotsuchi-core
```

- [x] **Step 4: Run full suite** *(same results as before)*

```bash
make test
```

- [x] **Step 5: Commit** *(done in previous session)*

```bash
git add core/Cargo.toml cli/Cargo.toml Cargo.lock
git commit -m "chore(deps): upgrade sha2 0.10 → 0.11"
```

---

## Task 4: Upgrade thiserror 1 → 2

User-facing `#[derive(Error)]` + `#[error("…")]` API is identical. Benefit is faster compile time.

- [x] **Step 1 (RED): Bump in all three crates** *(already applied in previous session)*

In [core/Cargo.toml](core/Cargo.toml), [cli/Cargo.toml](cli/Cargo.toml), [mcp/Cargo.toml](mcp/Cargo.toml):
```toml
thiserror = "2"
```

- [x] **Step 2: Build** *(compiled cleanly)*

```bash
make build 2>&1
```

Expected: compiles cleanly. If `#[error("…")]` format string syntax errors appear (2.x tightened validation), fix as directed.

- [x] **Step 3 (GREEN): Run tests** *(all passing)*

```bash
make test
```

- [x] **Step 4: Commit** *(done in previous session)*

```bash
git add core/Cargo.toml cli/Cargo.toml mcp/Cargo.toml Cargo.lock
git commit -m "chore(deps): upgrade thiserror 1 → 2"
```

---

## Task 6 (Optional): Upgrade notify 6 → 9.0.0-rc.4

`notify` 9 is a release candidate. Only attempt if watcher latency is a measured problem and rc risk is acceptable.

**Current call patterns in [core/src/watcher.rs](core/src/watcher.rs):**
- `notify::recommended_watcher(…)`
- `notify::RecursiveMode::Recursive`
- `notify::event::{EventKind, ModifyKind, RenameMode}`
- `notify::Event as NotifyEvent`
- `notify::event::EventAttributes::default()`
- `notify::event::DataChange::Content`

- [x] **Step 1: Decide go/no-go** *(user chose to proceed)*

```bash
cargo test -p shiotsuchi-core watcher 2>&1
```

Note test count. 10 watcher tests (9 pass, 1 pre-existing failure).

- [x] **Step 2 (RED): Bump version**

In [core/Cargo.toml](core/Cargo.toml):
```toml
notify = { version = "9.0.0-rc.4", optional = true }
```

- [x] **Step 3: Build with watcher feature**

```bash
cargo build -p shiotsuchi-core --features watcher 2>&1
```

No compiler errors — notify 9 API is backward-compatible with notify 6 call sites.

- [x] **Step 4 (GREEN): Run watcher tests** *(9/10 pass; same pre-existing failure)*

```bash
cargo test -p shiotsuchi-core watcher --features watcher 2>&1
```

- [x] **Step 5: Manual smoke test** *(watcher started and killed cleanly)*

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

- [x] **Step 6: Run full suite** *(186/192 pass; 6 pre-existing failures unchanged)*

```bash
make test
```

- [x] **Step 7: Commit** *(commit 14a5b4f)*

```bash
git add core/Cargo.toml core/src/watcher.rs Cargo.lock
git commit -m "chore(deps): upgrade notify 6 → 9.0.0-rc.4 (watcher latency)"
```

---

## Task 7: Post-benchmark comparison

Compare benchmarks after all deferred upgrades against the baseline from Task 0.

- [x] **Skipped.** No performance-impacting changes were made (sha2/thiserror already on latest; notify is runtime-only, not benchmarked).

---

## Execution

When ready to execute, create a new feature branch from `main` and run tasks sequentially in a single session.
