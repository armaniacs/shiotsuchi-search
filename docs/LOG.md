# Logging Guide — shiotsuchi

This document explains how to read, control, and understand the logging system in `shiotsuchi`.

> **Prerequisite:** As of v0.4.20, shiotsuchi uses the `tracing` crate for logging. It has been fully migrated from the legacy `log` + `env_logger` system.

---

## Table of Contents

- [Basic Usage: RUST_LOG](#basic-usage-rust_log)
- [Log Output Destinations](#log-output-destinations)
- [Reading Log Formats](#reading-log-formats)
  - [CLI Format](#cli-format)
  - [HTTP Server Format](#http-server-format)
  - [MCP Server Format](#mcp-server-format)
  - [index_directory span](#index_directory-span)
- [Common Use Cases](#common-use-cases)
- [Design Rationale](#design-rationale)
  - [Why tracing instead of log](#why-tracing-instead-of-log)
  - [Why stderr instead of stdout](#why-stderr-instead-of-stdout)
  - [Why initialization differs per crate](#why-initialization-differs-per-crate)
  - [Why the LogTracer bridge is needed](#why-the-logtracer-bridge-is-needed)
  - [Why the HTTP server is special](#why-the-http-server-is-special)

---

## Basic Usage: RUST_LOG

Log levels are controlled via the `RUST_LOG` environment variable:

```sh
# All crates: warn and above (default)
RUST_LOG=warn shiotsuchi index

# shiotsuchi_core: debug and above
RUST_LOG=shiotsuchi_core=debug shiotsuchi index

# Multiple crate filters (comma-separated)
RUST_LOG=shiotsuchi_core=debug,shiotsuchi=info shiotsuchi index

# All crates: trace and above (most verbose)
RUST_LOG=trace shiotsuchi index

# tower-http trace logs (HTTP request detail)
RUST_LOG=tower_http=trace shiotsuchi serve
```

Available log levels (lowest to highest):

| Level | Usage |
|-------|-------|
| `error` | Unrecoverable errors. Also displayed to the user. |
| `warn` | Recoverable issues. Search fallback, permission failures, etc. |
| `info` | Informational. HTTP requests, MCP tool calls, index completion, etc. |
| `debug` | Debug details. File exclusion reasons, backlink updates, etc. |
| `trace` | Trace. Currently unused (reserved for future expansion). |

### `--verbose` / `-v` flag

When `--verbose` is passed, the default filter changes from `warn` to `debug` (only when `RUST_LOG` is not set).

```sh
# RUST_LOG unset, no verbose → warn and above only
shiotsuchi index

# RUST_LOG unset, verbose → debug and above
shiotsuchi index --verbose

# RUST_LOG explicitly set takes precedence over --verbose
RUST_LOG=info shiotsuchi index --verbose   # info and above only (verbose ignored)
```

---

## Log Output Destinations

| Subsystem | Destination | Reason |
|-----------|-------------|--------|
| CLI | stderr | stdout is reserved for search results and user-facing output |
| HTTP server | stderr | stdout is for process management (systemd, etc.) |
| MCP server | stderr | **stdout is exclusively for JSON-RPC protocol. Never mix logs into it.** |

### MCP Server Special Notes

**The MCP server must never write logs to stdout.** stdout is used for JSON-RPC communication with MCP clients such as Claude Desktop. Even a single byte of log output will corrupt the protocol.

The MCP server's `tracing-subscriber` initialization explicitly sets `.with_writer(std::io::stderr)`. As defense-in-depth, `.with_ansi(false)` is also configured to prevent escape sequences from polluting log files.

```rust
// mcp/src/main.rs initialization
tracing_log::LogTracer::init().ok();
tracing_subscriber::fmt()
    .with_writer(std::io::stderr)
    .with_ansi(false)
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

---

## Reading Log Formats

### CLI Format

The CLI uses `compact()` + `with_target(false)`. Module paths are omitted, producing one event per line:

```
2026-06-06T15:00:00.123456Z WARN shiotsuchi_core::indexer: File path "..." outside vault root "..."
2026-06-06T15:00:00.123789Z WARN shiotsuchi_core::indexer: Skipping invalid exclude pattern "invalid[": glob parse error
```

Field reference:

| Field | Example | Description |
|-------|---------|-------------|
| Timestamp | `2026-06-06T15:00:00.123456Z` | ISO 8601 UTC time with microsecond precision |
| Level | `WARN` | Log level, right-padded to 5 characters |
| Module | `shiotsuchi_core::indexer` | Rust module path that emitted the event |
| Message | `File path "..." outside vault root "..."` | Free-form text message |

### HTTP Server Format

The HTTP server uses `TraceLayer` to emit span-based structured logs. Each request generates a single response line containing structured fields within the span context.

```
2026-06-06T15:00:00.123456Z  INFO request{request_id="a1b2c3d4-e5f6-7890-abcd-ef1234567890" method=GET path=/api/v1/health}: tower_http::trace::on_response: status=200 latency_ms=2
```

Structured fields:

| Field | Example | Description |
|-------|---------|-------------|
| `request_id` | `a1b2c3d4-e5f6-7890-abcd-ef1234567890` | Per-request UUID, or client-specified value from `x-request-id` header |
| `method` | `GET` | HTTP method |
| `path` | `/api/v1/health` | Request path |
| `status` | `200` | HTTP status code |
| `latency_ms` | `2` | Request processing time in milliseconds |

### MCP Server Format

The MCP server logs structured events when tools are invoked:

```
2026-06-06T15:00:00.123456Z  INFO shiotsuchi_mcp::handler: MCP tool called tool="search_local_notes"
```

| Field | Example | Description |
|-------|---------|-------------|
| `tool` | `"search_local_notes"` | Name of the tool being called |

### index_directory span

When running `shiotsuchi index`, the `index_directory` function automatically generates span events via `#[tracing::instrument]`:

```
2026-06-06T15:00:00.000000Z  INFO index_directory{vault_count=3}: shiotsuchi_core::indexer: started
2026-06-06T15:00:10.000000Z  INFO index_directory{vault_count=3}: shiotsuchi_core::indexer: 10 inserted, 2 updated, 0 skipped, 0 errors
```

The `{vault_count=3}` following the span name shows span fields — in this case, the number of configured vaults.

---

## Common Use Cases

### Monitoring Index Progress

```sh
# Detailed index processing logs
RUST_LOG=shiotsuchi_core=debug shiotsuchi index
```

Sample output:
```
2026-06-06T15:00:00.123456Z WARN shiotsuchi_core::indexer: Skipping invalid exclude pattern "[": glob parse error
2026-06-06T15:00:01.456789Z DEBUG shiotsuchi_core::indexer: Excluded node_modules (matched exclude pattern)
2026-06-06T15:00:05.123456Z  INFO index_directory{vault_count=2}: shiotsuchi_core::indexer: 150 inserted, 3 updated, 12 skipped, 0 errors
```

### Checking Search Fallback Reasons

```sh
RUST_LOG=shiotsuchi_core=warn shiotsuchi search "query"
```

Sample output:
```
2026-06-06T15:00:00.123456Z WARN shiotsuchi_core::search: Hybrid search vec component failed (embedding error), falling back to FTS only
```

### Tracing HTTP Requests

```sh
# Start the server
RUST_LOG=tower_http=trace shiotsuchi serve

# In another terminal, send a request
curl -i http://localhost:7171/api/v1/health
```

Server stderr output:
```
2026-06-06T15:00:00.123456Z  INFO request{request_id="a1b2c3d4-e5f6-7890-abcd-ef1234567890" method=GET path=/api/v1/health}: tower_http::trace::on_response: status=200 latency_ms=2
```

Response headers:
```
x-request-id: a1b2c3d4-e5f6-7890-abcd-ef1234567890
```

### Debugging the MCP Server

```sh
# Debug Claude Desktop communication
RUST_LOG=info shiotsuchi-mcp

# Tool call logs only
RUST_LOG=shiotsuchi_mcp=info shiotsuchi-mcp
```

To verify that stdout contains only JSON-RPC messages and stderr contains log output:

```sh
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | RUST_LOG=info shiotsuchi-mcp 2>/tmp/mcp.log
cat /tmp/mcp.log   # ← logs go here
# stdout contains JSON only
```

### Identifying Slow Endpoints

```sh
RUST_LOG=tower_http=trace shiotsuchi serve 2>&1 | grep latency_ms
```

Sample output:
```
2026-06-06T15:00:00.123456Z  INFO request{...}: tower_http::trace::on_response: status=200 latency_ms=2340
2026-06-06T15:00:01.456789Z  INFO request{...}: tower_http::trace::on_response: status=200 latency_ms=5
```

The first request took 2.3 seconds — an easily identifiable slow endpoint.

---

## Design Rationale

### Why tracing instead of log

The previous system used `log` + `env_logger`. The migration to `tracing` was driven by these factors:

| Aspect | `log` | `tracing` |
|--------|-------|-----------|
| Structured data | Not supported (text only) | Native structured field support |
| Spans | None | Auto-generated span via `#[instrument]` |
| Performance | No compile-time filtering | Disabled spans/events are zero-cost (compiled out) |
| Ecosystem | Incompatible with axum/tower-http TraceLayer | Integrates with TraceLayer, OpenTelemetry, Loki |
| Async support | Not supported | Native tokio/async tracing |

Structured logging is critical for log aggregation platforms (Loki, CloudWatch Logs, Datadog):

- `log`: `"Hybrid search vec component failed (embedding error), falling back to FTS only"` — full-text search only
- `tracing`: `status=200 latency_ms=2340` → filter on `{status}"200"`, alert on `latency_ms > 1000`

### Why stderr instead of stdout

Per Unix philosophy, diagnostic logs should **always go to stderr**. This lets pipelines extract only the standard output:

```sh
# Logs go to stderr; only results are written to the file
shiotsuchi search "project plan" > results.json
```

For the MCP server, this is even more critical: stdout is the JSON-RPC protocol transport. Even a single byte of log output is unacceptable.

### Why initialization differs per crate

The three binary crates (cli, mcp, HTTP server) each have **independent main functions**, so each initializes `tracing-subscriber` to suit its specific needs:

| Crate | Initialization | Rationale |
|-------|---------------|-----------|
| **cli** | `.compact().with_target(false)` + `try_from_default_env().unwrap_or_else(...)` | Human-readable compact format. Default level controlled by `-v` flag when `RUST_LOG` is unset. |
| **mcp** | `.with_writer(stderr).with_ansi(false)` + `from_default_env()` + `LogTracer::init()` | stdout protection is paramount. No escape sequences. Bridges `log::` calls from dependencies. |
| **HTTP** | (No subscriber init in the server crate. `shiotsuchi serve` uses the CLI subscriber.) | `TraceLayer` generates spans; the CLI subscriber renders them. |
| **core** | (Library crate — no subscriber initialization) | Library crates must not assume a subscriber is configured. Events are no-op when no subscriber is registered. |

### Why the LogTracer bridge is needed

`tracing_log::LogTracer` is a bridge that forwards `log` crate macro calls to `tracing` events.

`LogTracer::init()` is called **only in the MCP server**, for these reasons:

- MCP server has the strictest stdout isolation requirements
- `tracing-subscriber` alone does not intercept `log::warn!` (it does not call `log::set_logger`)
- `LogTracer::init()` ensures that any `log::` calls from dependencies (e.g., pre-migration code) also go to stderr
- CLI handles this through its own subscriber initialization; core is a library so it doesn't need it

### Why the HTTP server is special

The HTTP server uses `tower-http`'s `TraceLayer` to generate request/response spans. The design choices:

- **Request ID**: `SetRequestIdLayer` assigns a UUID per request. If the client sends an `x-request-id` header, that value is propagated instead.
- **Latency measurement**: `TraceLayer` automatically measures processing time per request, requiring no handler changes.
- **Error tracing**: Logs can be correlated with response `x-request-id` headers to identify which requests were slow or errored.

```rust
// Layer composition (built with ServiceBuilder)
// 1. SetRequestIdLayer — generates/UUIDs requests and attaches them (outermost, runs first)
// 2. TraceLayer       — creates request spans, records status+latency on response
// 3. PropagateRequestIdLayer — propagates request ID to response x-request-id header (innermost)
```

Layer ordering is critical:
1. `SetRequestIdLayer` first: assigns UUID or client-specified ID to the request
2. `TraceLayer` next: uses that ID for span creation
3. `PropagateRequestIdLayer` last: adds `x-request-id` to the response header
