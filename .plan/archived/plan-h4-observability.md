# Plan: Structured Observability and Metrics

**Issue**: H4 — SRE/Ops Specialist (Checking Team)
**Severity**: High
**Status**: Plan only (not implemented)

> **Review (2026-05-08): Not needed at this stage. All three layers are over-engineered for a single-user local CLI/MCP tool.**
>
> **Why deferred:**
> - **Logging is adequate** — `env_logger` + `RUST_LOG` handles filtering. The codebase has only ~13 `log::warn!()` calls and no async/multi-thread complexity that would benefit from tracing spans. Moving to `tracing` would add 3 dependencies for negligible gain.
> - **Health check has no consumer** — The MCP server is a stdio child process of Claude Desktop. There is no HTTP endpoint, no external monitor, and no automated caller for a `vault_health` tool. `PRAGMA integrity_check` is also expensive on large DBs.
> - **Metrics are speculative** — Atomic counters for indexing rate, latency histograms, etc. serve no actionable purpose in a single-user tool. The `scan` watcher runs in the foreground; errors are visible immediately on stderr.
> - **Three orders of magnitude over-spec** — This plan is designed for a production server (multi-user, HTTP/gRPC, monitoring infrastructure). Shiotsuchi Search is none of those things today.
>
> **Revisit when:** The tool gains a network-facing server, multi-user access, or the logging surface grows past ~50 call sites.

## Problem

The current project uses only `env_logger` for unstructured logging. There are no metrics, health checks, or structured observability hooks. This makes it impossible to:
- Monitor long-running `scan` (watcher) sessions
- Diagnose silent failures in production
- Alert on degraded performance or errors

### Current State

- `env_logger` initialized in `cli/src/main.rs:41-43` (or MCP equivalent)
- Default log level: `warn`, overridable via `RUST_LOG` or `--verbose`
- No structured JSON logging
- No health check endpoint for MCP server
- No metrics counters (indexing rate, errors, DB size)
- No tracing IDs for correlating multi-step operations

## Design

### Approach: Incremental Layer Addition

Phase in observability in three layers — each layer can be shipped independently.

### Layer 1: Structured Logging with `tracing`

**Goal**: Replace `env_logger` with `tracing` + `tracing-subscriber` for structured, JSON-format logging.

**Changes**:
1. **Dependencies** (`core/Cargo.toml`, `cli/Cargo.toml`, `mcp/Cargo.toml`):
   ```toml
   tracing = "0.1"
   tracing-subscriber = { version = "0.3", features = ["json", "env-filter"] }
   ```

2. **`cli/src/main.rs`**: Replace `env_logger` init:
   ```rust
   use tracing_subscriber::EnvFilter;
   tracing_subscriber::fmt()
       .json()
       .with_env_filter(EnvFilter::from_default_env())
       .init();
   ```
   Preserve `RUST_LOG` compatibility via `EnvFilter`.

3. **`mcp/src/main.rs`**: Apply the same subscriber.

4. **Key locations to add tracing spans**:
   - `core/src/indexer.rs`: `index_directory()`, `index_file()` — add `#[instrument]` or spans
   - `core/src/watcher.rs`: `handle_event()` — add span with file path
   - `core/src/search.rs`: `search()` — add span with query text
   - `core/src/db.rs`: key operations — add debug spans

### Layer 2: Health Check

**Goal**: Add a health check mechanism for the MCP server so that Claude Desktop or external monitors can verify the server is operational.

**Changes**:
1. **New MCP tool**: `vault_health` returning:
   ```json
   {
     "status": "ok" | "degraded" | "error",
     "db_connected": true,
     "db_path": "/path/to/db",
     "total_notes": 42,
     "last_indexed_at": "2026-05-06T00:00:00Z",
     "db_size_bytes": 123456
   }
   ```

2. **MCP handler addition** (`mcp/src/handler.rs`):
   - Add `vault_health` to the tool dispatch
   - Query `NoteDatabase::stats()` for DB health
   - Check `PRAGMA integrity_check` on the database

3. **Signal-based health for CLI**: For `scan` mode, respond to `SIGUSR1` by printing health info to stderr.

### Layer 3: Metrics Export

**Goal**: Expose basic counters and gauges for operational visibility.

**Changes**:
1. **New module `core/src/metrics.rs`**:
   ```rust
   pub struct Metrics {
       pub notes_indexed: Counter,
       pub notes_deleted: Counter,
       pub notes_skipped: Counter,
       pub index_errors: Counter,
       pub search_latency: Histogram,
       pub db_size_bytes: Gauge,
   }
   ```

2. **Use `metrics` crate** (or manual atomic counters if simplicity preferred).

3. **For MCP**: Expose metrics as a tool or on `SIGUSR1`.

4. **For CLI**: Periodic metrics dump to stderr during long `scan` sessions.

### File Changes

| File | Layer | Change |
|------|-------|--------|
| `cli/Cargo.toml` | 1 | Add `tracing`, `tracing-subscriber` |
| `mcp/Cargo.toml` | 1 | Add `tracing`, `tracing-subscriber` |
| `core/Cargo.toml` | 1 | Add `tracing` |
| `cli/src/main.rs` | 1 | Replace `env_logger` with `tracing-subscriber` |
| `mcp/src/main.rs` | 1 | Replace `env_logger` with `tracing-subscriber` |
| `core/src/indexer.rs` | 1 | Add `#[instrument]` |
| `core/src/watcher.rs` | 1 | Add span to `handle_event` |
| `core/src/search.rs` | 1 | Add span to `search` |
| `mcp/src/handler.rs` | 2 | Add `vault_health` tool |
| `core/src/metrics.rs` | 3 | New file — metrics counters |
| `core/src/lib.rs` | 3 | Export `pub mod metrics` |

### Testing

- Layer 1: Test that tracing output is emitted (capture with `tracing_subscriber::fmt().with_test_writer()`)
- Layer 2: Unit test `vault_health` response format and error cases
- Layer 3: Test metrics counter increments on index/search operations

## Trade-offs

- **Layer 1 only** (recommended minimum): ~50 line changes, significant debugging improvement with no API change
- **Layer 1+2** (recommended): ~100 line changes, enables Claude Desktop health monitoring
- **Layer 3** (nice-to-have): Additional complexity, defer until multi-user or production deployment

## Not In Scope

- Prometheus/OpenTelemetry exporter
- Remote metrics aggregation
- Distributed tracing (single-process tool only)
- Alerting rules (infrastructure concern)
