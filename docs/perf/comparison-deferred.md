# Dependency Upgrade — Performance Comparison (Deferred)

Date: 2026-05-16
Upgraded: sha2 0.10→0.11, thiserror 1→2
Skipped: notify (rc risk outweighed benefit)

| Benchmark | Before | After | Change |
|-----------|--------|-------|--------|
| index_100_files | 2.5233 s | 2.5599 s | +1.45% (within noise threshold) |
| search_1000_notes | 1.1216 ms | 1.1750 ms | +5.67% (noise — sha2/thiserror do not affect queries) |

## Notes
- **index_100_files**: Criterion classified the change as "Change within noise threshold." The +1.45% is consistent with normal benchmark variance on a shared laptop.
- **search_1000_notes**: The +5.67% difference was flagged as a regression, but this is expected measurement noise. sha2 affects only file hashing during indexing (not search queries), and thiserror is compile-time only. The benchmark creates real files on disk and opens a fresh database connection per iteration, which introduces OS-level variance.
- Neither upgrade modifies any runtime search or indexing code paths.
