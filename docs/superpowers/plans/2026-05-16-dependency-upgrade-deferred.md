# Dependency Upgrade Implementation Plan — Deferred

> **Status:** Ready to execute. The immediate plan has been completed and merged into `main`. Baseline benchmarks remain valid.

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

- [ ] **Step 1 (RED): Bump version**

In [core/Cargo.toml](core/Cargo.toml) and [cli/Cargo.toml](cli/Cargo.toml):
```toml
sha2 = "0.11"
```

- [ ] **Step 2: Attempt build**

```bash
cargo build -p shiotsuchi-core 2>&1
```

The call-site pattern is:
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

- [ ] **Step 4: Run full suite**

```bash
make test
```

- [ ] **Step 5: Commit**

```bash
git add core/Cargo.toml cli/Cargo.toml Cargo.lock
git commit -m "chore(deps): upgrade sha2 0.10 → 0.11"
```

---

## Task 4: Upgrade thiserror 1 → 2

User-facing `#[derive(Error)]` + `#[error("…")]` API is identical. Benefit is faster compile time.

- [ ] **Step 1 (RED): Bump in all three crates**

In [core/Cargo.toml](core/Cargo.toml), [cli/Cargo.toml](cli/Cargo.toml), [mcp/Cargo.toml](mcp/Cargo.toml):
```toml
thiserror = "2"
```

- [ ] **Step 2: Build**

```bash
make build 2>&1
```

Expected: compiles cleanly. If `#[error("…")]` format string syntax errors appear (2.x tightened validation), fix as directed.

- [ ] **Step 3 (GREEN): Run tests**

```bash
make test
```

- [ ] **Step 4: Commit**

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

- [ ] **Step 1: Decide go/no-go**

```bash
cargo test -p shiotsuchi-core watcher 2>&1
```

Note test count. If fewer than 3 watcher tests exist, coverage is thin.

- [ ] **Step 2 (RED): Bump version**

In [core/Cargo.toml](core/Cargo.toml):
```toml
notify = { version = "9.0.0-rc.4", optional = true }
```

- [ ] **Step 3: Build with watcher feature**

```bash
cargo build -p shiotsuchi-core --features watcher 2>&1
```

Fix compiler errors in [core/src/watcher.rs](core/src/watcher.rs).

- [ ] **Step 4 (GREEN): Run watcher tests**

```bash
cargo test -p shiotsuchi-core watcher --features watcher 2>&1
```

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

## Task 7: Post-benchmark comparison

Compare benchmarks after all deferred upgrades against the baseline from Task 0.

- [ ] **Step 1: Run benchmarks**

```bash
cargo bench -p shiotsuchi-core 2>&1 | tee docs/perf/after-deferred-upgrade.txt
```

- [ ] **Step 2: Compare with baseline**

```bash
grep "time:" docs/perf/baseline.txt
grep "time:" docs/perf/after-deferred-upgrade.txt
```

- [ ] **Step 3: Write comparison summary**

Create `docs/perf/comparison-deferred.md`:

```markdown
# Dependency Upgrade — Performance Comparison (Deferred)

Date: YYYY-MM-DD
Upgraded: sha2 0.10→0.11, thiserror 1→2, notify 6→9.0.0-rc.4

| Benchmark | Before | After | Change |
|-----------|--------|-------|--------|
| index_100_files | X.XX ms | X.XX ms | ±X% |
| search_1000_notes | X.XX µs | X.XX µs | ±X% |
```

- [ ] **Step 4: Commit**

```bash
git add docs/perf/after-deferred-upgrade.txt docs/perf/comparison-deferred.md
git commit -m "perf: record benchmark results after deferred dependency upgrades"
```

---

## Execution

When ready to execute, create a new feature branch from `main` and run tasks sequentially in a single session.
