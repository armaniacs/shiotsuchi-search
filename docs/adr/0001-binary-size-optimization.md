# ADR-0001: Binary Size Optimization Strategy

- **Date**: 2026-05-20
- **Status**: Accepted
- **Branch**: feat-min-size

## Context

The shiotsuchi-search project ships two binaries (`shiotsuchi`, `shiotsuchi-mcp`). Smaller binaries improve download times, reduce disk footprint in deployment, and are generally desirable for a CLI/MCP tool.

We reviewed the techniques catalogued in [min-sized-rust](https://github.com/johnthagen/min-sized-rust) and evaluated each against this project's constraints.

## Decision

### Techniques already applied (baseline)

| Setting | Value | Effect |
|---------|-------|--------|
| `opt-level` | `"z"` | Optimize for size over speed |
| `lto` | `true` | Cross-crate dead code elimination |
| `codegen-units` | `1` | Maximum inlining / optimization |
| `strip` | `"symbols"` | Remove debug symbols from output binary |

### Technique added: `panic = "abort"`

```toml
[profile.release]
panic = "abort"
```

Removes the stack unwinding machinery (`__rust_begin_short_backtrace`, `rust_begin_unwind`, libunwind linkage). This codebase does not use `catch_unwind` or panic-as-control-flow, so the behavioral change (process aborts on panic instead of unwinding) is acceptable. The size saving is meaningful because the unwinding tables and formatting code are eliminated.

### Technique added: `cargo-bloat` analysis

`cargo-bloat --release -p shiotsuchi --crates` and the equivalent for `shiotsuchi-mcp` are used to identify the dominant contributors to binary size. This drives prioritization of further optimization work.

## Rejected Techniques

### UPX compression — Rejected

UPX compresses the binary on disk and decompresses it into memory at startup.

**Reasons for rejection:**

1. **Startup latency**: Decompression adds measurable startup overhead. For a CLI tool invoked frequently (e.g., in shell scripts or editor integrations), this is user-visible.
2. **Antivirus false positives**: UPX-packed binaries are routinely flagged by heuristic-based antivirus software (documented in min-sized-rust). This creates friction for end users on macOS (Gatekeeper) and Windows.
3. **Not an actual size reduction**: The binary on disk is smaller, but the in-memory footprint at runtime is identical. The tradeoff is CPU + latency at every startup.
4. **Distribution complexity**: Pre-built release binaries distributed via GitHub Releases or Homebrew would require special handling; tooling that inspects ELF/Mach-O headers (e.g., `lipo`, codesign, `otool`) may behave unexpectedly on packed binaries.

### Nightly-only techniques — Rejected

Techniques requiring a nightly toolchain (`-Z build-std`, `-Zlocation-detail=none`, `-Zfmt-debug=none`, `panic = "immediate-abort"`, `#![no_main]`, `#![no_std]`):

**Reasons for rejection:**

1. **Toolchain instability**: Nightly features can break between releases with no deprecation notice. This project targets stable Rust to guarantee reproducible builds.
2. **CI complexity**: Pinning a nightly version in CI (`rust-toolchain.toml`) adds maintenance burden; nightly pins rot quickly.
3. **`#![no_std]` / `#![no_main]` incompatibility**: This project depends on `rusqlite` (bundled SQLite), `ort` (ONNX Runtime via C FFI), `rayon`, `tokio`, and `vaporetto` — all of which require `std`. Removing `std` is not feasible without replacing the entire dependency tree.
4. **`-Zfmt-debug=none`** would silently break `dbg!()`, `assert!()`, and `unwrap()` error messages — unacceptable for a CLI tool where panic messages are a primary debugging surface.
5. **Marginal gains given dominant dependencies**: The largest contributors to binary size are `tokenizers` (HuggingFace tokenizer), `ort` (ONNX Runtime), and `rusqlite` (bundled SQLite). Nightly std optimizations save tens of KB; these crates contribute MBs. The nightly complexity is not justified by the marginal improvement.

## Measurements

Measured on macOS (Apple Silicon), `cargo build --release`, comparing `panic = "unwind"` (baseline) vs `panic = "abort"`.

| Binary | panic=unwind | panic=abort | Reduction |
|--------|-------------|-------------|-----------|
| `shiotsuchi` | 20 MB | 19 MB | −1 MB (5%) |
| `shiotsuchi-mcp` | 3.0 MB | 2.8 MB | −0.2 MB (7%) |

The savings are real but modest. `shiotsuchi` remains large (19 MB) because `ort` (ONNX Runtime) and `tokenizers` (HuggingFace) dominate the binary size. Further reduction requires addressing those dependencies directly, not profile tuning.

### `cargo-bloat` results

`cargo bloat --release --crates` output (macOS Apple Silicon, `panic = "abort"`):

**`shiotsuchi`** (19 MB on disk, 15.5 MB `.text`):

| Crate | `.text` size | Share |
|-------|-------------|-------|
| `ort_sys` | 11.1 MB | 71.6% |
| `[Unknown]` (C/C++ objects) | 2.8 MB | 18.1% |
| `std` | 383 KB | 2.4% |
| `tokenizers` | 255 KB | 1.6% |
| everything else | ~900 KB | 6.3% |

**`shiotsuchi-mcp`** (2.8 MB on disk, 1.9 MB `.text`): no dominant crate; `std`, `regex_automata`, `clap_builder` each contribute under 350 KB. Effectively at the floor for its feature set.

The `ort_sys` crate (ONNX Runtime C++ library, statically linked) is responsible for 71% of `shiotsuchi`'s `.text` section. `tokenizers` is negligible at 255 KB. Profile-level tuning cannot reclaim this space.

### Why further reduction was not pursued

Three options were considered for reducing `ort_sys` size:

1. **Dynamic linking for ONNX Runtime** — binary shrinks, but requires users to install ONNX Runtime separately. Unacceptable for a CLI tool that should work out of the box.
2. **Make semantic search an optional feature flag** — gates `ort` behind a feature; a `--no-default-features` build would be small. Significant refactor with uncertain demand.
3. **Accept current size** — 19 MB is within normal range for a CLI tool that bundles a neural network runtime. Chosen.

Option 3 was selected. The binary size is a direct and honest reflection of the functionality shipped.

## Consequences

- `panic = "abort"` is unconditionally set in `[profile.release]`. Any future use of `catch_unwind` in this codebase must be preceded by revisiting this ADR.
- `cargo-bloat` is the recommended tool before any further dependency additions, to maintain awareness of size impact.
- If a future use case demands the smallest possible binary (e.g., embedded or WASM target), a separate `[profile.min-size]` profile with nightly flags can be introduced without affecting the primary release profile.
