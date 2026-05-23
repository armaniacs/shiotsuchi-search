# ADR-0002: f16 Embedding Quantization

- **Date**: 2026-05-21 (updated 2026-05-23)
- **Status**: **Deferred** — sqlite-vec v0.1.x accepts `FLOAT2` DDL but silently treats it as `FLOAT` (f32). Actual f16 blobs cause `INSERT` failures. Pending sqlite-vec v0.2+ with proper FLOAT2 support.
- **Branch**: feat-min-size

## Context

The shiotsuchi-search project stores vector embeddings for semantic search. Each chunk is embedded as a 1024-dimensional f32 vector, consuming 4 KB per chunk. With 50,000 chunks (a moderate vault), this reaches 200 MB for the embedding table alone — significant for local-first tools that sync via iCloud, Dropbox, or OneDrive.

sqlite-vec supports multiple embedding storage formats: `FLOAT[1024]` (f32, 4 KB/row), `FLOAT2[1024]` (f16 half-precision, 2 KB/row), and `FLOAT4_BINARY[1024]` (binary quantization, 128 B/row).

We benchmarked the three formats to determine whether quantization degrades search quality to an unacceptable degree.

## Benchmark Methodology

- 200 random 1024-dimensional vectors generated with seeded XorShift32 PRNG
- 5 query vectors, 195 candidate vectors
- f32 cosine similarity as ground truth
- precision@k measured as: `|top-k(approx) ∩ top-k(f32)| / k`
- Metrics: quantization speed, similarity calculation speed, rerank throughput

## Benchmark Results

### Precision@k

| k | f16 | binary |
|:-:|:---:|:------:|
| 1 | **1.0000** | 0.0000 |
| 5 | **1.0000** | 0.4800 |
| 10 | **1.0000** | 0.5200 |
| 50 | **1.0000** | 0.6280 |

f16 preserves perfect precision across all k values. binary quantization loses all top-1 accuracy and only reaches 63% precision at k=50 — unacceptable for a search tool.

### Throughput

| Operation | f32 | f16 | binary |
|-----------|:---:|:---:|:------:|
| Quantize 100 vectors | — | 233 µs | 53 µs |
| Cosine similarity (1 pair) | 2.8 µs | 5.2 µs | 12 ns |
| Rerank 195 candidates | 557 µs | 1033 µs | 6.1 µs |
| Storage per chunk | 4 KB | **2 KB** | 128 B |

f16 similarity is ~1.9× slower than f32 (5.2 vs 2.8 µs). This is acceptable because:
1. The primary bottleneck is sqlite-vec's ANN search, not cosine reranking
2. Rerank is only performed on a small candidate set (typically 50-200 rows)
3. The rerank step (1033 µs) is still far below the user-perceptible threshold

## Decision (Deferred)

**Adopt `FLOAT2[1024]` (f16) for embedding storage once sqlite-vec supports it.**  
The decision is deferred because sqlite-vec v0.1.x has a DDL-level incompatibility: it accepts `FLOAT2[1024]` without error but stores data as `FLOAT` (f32). Writing actual f16 blobs causes `INSERT` failures in the `vec0` virtual table. The schema v4 migration was implemented (commit `81fc49c`) but reverted (commit `e7734c0`).

When sqlite-vec v0.2+ ships proper `FLOAT2` support, the following changes should be made:

### Schema change

```sql
-- Before (f32) — current state
CREATE VIRTUAL TABLE vec_chunks USING vec0(
    chunk_id  INTEGER PRIMARY KEY,
    embedding FLOAT[1024]
);

-- After (f16) — deferred
CREATE VIRTUAL TABLE vec_chunks USING vec0(
    chunk_id  INTEGER PRIMARY KEY,
    embedding FLOAT2[1024]
);
```

### Code changes (pending)

1. **`core/src/db.rs` — `create_schema()`**: `FLOAT[1024]` → `FLOAT2[1024]`
2. **`core/src/db.rs` — `insert_embeddings()`**: Convert f32→f16 before serializing (2 bytes per component instead of 4)
3. **`core/src/db.rs` — `vec_search()`**: Convert the query embedding to f16 before passing to sqlite-vec's `MATCH`
4. **`core/src/db.rs` — `reindex_file()`**: Same f32→f16 conversion in the embedding INSERT path
5. **`core/src/embedder.rs`**: Keep `Vec<f32>` as the internal representation (ONNX model outputs f32). Quantize at the DB boundary only
6. **Migration v3→v4**: Recreate `vec_chunks` with the new type; reindex all embeddings transactionally

### No changes needed (even after deferral)

- `search.rs` — `search_vec()` already receives the embedding from `embedder.embed()` (still `Vec<f32>`) and passes it to `db.vec_search()` — the quantization happens inside `vec_search()`
- `indexer.rs` — `index_file_with_embedder()` passes embeddings to `db.reindex_file()` — quantization inside the DB layer

## Rejected Alternatives

### FLOAT4_BINARY (binary quantization) — Rejected

Binary quantization loses all top-1 accuracy (precision@1=0.0). For a search tool where the top result is the most important, this is fundamentally unacceptable. Even at k=50, precision is only 63%.

### Stay with FLOAT (f32) — De facto current state (pending sqlite-vec v0.2+)

FLOAT is the current storage format. It was never rejected as a design choice — it is the **fallback** while sqlite-vec lacks proper FLOAT2 support. The quantization benchmark confirms that migrating to f16 would halve storage (4 KB → 2 KB per chunk, ~100 MB savings at 50K chunks) with zero precision loss, making the switch worthwhile once the dependency catches up.

## Consequences (deferred)

- **Storage halved** (once implemented): f32 4 KB/chunk → f16 2 KB/chunk
- **Zero precision loss** (verified): f16 has sufficient dynamic range for 1024-dimensional cosine similarity
- **Slight rerank slowdown** (expected): 1.9× slower similarity calculation (2.8 → 5.2 µs), negligible in practice
- **Clean boundary** (design preserved): The embedder remains f32 internally; quantization is a DB-layer concern
- **Migration ready** (code reverted, approach validated): v3→v4 schema upgrade path exists; just needs FLOAT2 DDL + f32→f16 conversion re-enabled
- **Schema v4 is live** but uses `FLOAT[1024]` (same as v3 functionally); the version bump was kept to avoid forcing existing users through another migration later
- **Benchmark preserved** at `core/benches/quantization.rs` — valid for precision/throughput verification when re-implementing

## Measurements

Measured on macOS (Apple Silicon M3), `cargo bench -p shiotsuchi-core --bench quantization`:

```text
quantize_f16_100_vectors    time: [179.91 µs 233.48 µs 303.40 µs]
cosine_similarity_f32       time: [2.8162 µs 2.8430 µs 2.8923 µs]
cosine_approximation_f16    time: [5.2180 µs 5.2322 µs 5.2492 µs]
rerank_f32_195_candidates   time: [555.14 µs 557.55 µs 560.48 µs]
rerank_f16_195_candidates   time: [1.0250 ms 1.0329 ms 1.0444 ms]
```

Precision@k output (stderr):
```
--- Quantization Precision@k ---
  precision@k=1: f16=1.0000 binary=0.0000
  precision@k=5: f16=1.0000 binary=0.4800
  precision@k=10: f16=1.0000 binary=0.5200
  precision@k=50: f16=1.0000 binary=0.6280
---
```
