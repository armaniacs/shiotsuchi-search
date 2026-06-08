# MCP Server (shiotsuchi-mcp)

Binary name: `shiotsuchi-mcp`
Crate path: `mcp/`

## Protocol

Implements [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) over stdio JSON-RPC 2.0.
Uses tokio multi-thread runtime for async background tasks.

## Available Tools

### `search_local_notes`

Search the user's Markdown vault using keyword (FTS5), semantic (vector), or hybrid retrieval.

**Input**:
```json
{
  "query": "search terms",
  "limit": 10,
  "mode": "hybrid",
  "min_score": 0.5
}
```

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `query` | string | required | Japanese or English search query |
| `limit` | integer | 10 | Max results (1–50) |
| `mode` | string | `"hybrid"` | `"fts"` (keyword-only, no model needed), `"vec"` (semantic, requires model), `"hybrid"` (RRF fusion) |
| `min_score` | number | optional | Minimum relevance score threshold |

**Output**: Structured Markdown with `### RETRIEVED CONTEXT ###` / `### END RETRIEVED CONTEXT ###` delimiters, source numbering, parent heading hierarchy, chunk IDs, and relevance scores.

### `get_surrounding_context`

Retrieve chunks immediately before and after a given chunk for expanded context.

**Input**:
```json
{
  "chunk_id": 123,
  "window": 2
}
```

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `chunk_id` | integer | required | ID from `search_local_notes` results |
| `window` | integer | 2 | Chunks before and after (1–5) |

**Output**: Markdown content of surrounding chunks with parent headers.

### `index_status`

Get vault indexing statistics.

**Input**: `{}`

**Output**: Plain text with total files, total chunks, vec-indexed chunks, database size, embedder status, and last indexed time. Reflects state at query time (may be slightly stale if background rebuild is running).

### `rebuild_index`

Trigger a full re-index of the vault in the background. Sends MCP `notifications/progress` on stdout with per-file (current/total) updates.

**Input**: `{}`

**Behavior**:
- Spawns a background tokio task
- Calls `core::indexer::index_directory()` with progress callback
- Emits `notifications/progress` on each file processed
- Does not block other tool calls

## JSON-RPC Methods

| Method | Description |
|--------|-------------|
| `initialize` | Server capabilities handshake |
| `tools/list` | List available tools |
| `tools/call` | Execute a tool |
| `ping` | Health check |
| `notifications/progress` | Emitted by background rebuild_index (params: current, total) |

## Tool Dispatch Flow

```
tools/call
    │
    ├── "rebuild_index" → spawn_rebuild() [background tokio task, own NoteDatabase]
    │                       └── index_directory() with progress callback
    │
    └── other tools → handler::call_tool()
                        ├──  shared db (Mutex<NoteDatabase> opened at startup)
                        ├── "search_local_notes" → ctx.db.lock() → search()
                        ├── "get_surrounding_context" → ctx.db.lock() → get_surrounding_chunks()
                        └── "index_status" → ctx.db.lock() → stats()
```

## Implementation Files

- `mcp/src/main.rs` — Stdio JSON-RPC loop, tokio `#[tokio::main]` entry point, dispatch routing for `rebuild_index`
- `mcp/src/protocol.rs` — `McpRequest`, `McpResponse`, `McpError`, `McpNotification`
- `mcp/src/tools.rs` — Tool definitions (JSON Schema for each tool)
- `mcp/src/handler.rs` — Tool execution logic: `call_tool()`, `search_local_notes()`, `get_surrounding_context()`, `index_status()`, structured Markdown formatting

## Error Handling

- Internal errors are mapped to generic `"Internal tool execution error"` to avoid information leakage
- Uses `get_tokenizer()` for cached tokenizer access (avoids per-request model init cost)
- `NoteDatabase::open()` is called once at startup and shared across all handlers via `Arc<std::sync::Mutex<NoteDatabase>>` (connection pooling). No per-request open overhead. `rebuild_index` opens its own `NoteDatabase` to avoid blocking the shared one during long-running indexing.
- `rebuild_index` errors are logged but not returned to client (background task)

## Configuration

Environment variables:
- `SHIOTSUCHI_NOTES_DIR` — Vault root (default: `.`)
- `SHIOTSUCHI_DB_PATH` — Database path (default: `~/.cache/shiotsuchi/db.sqlite3`)
- `SHIOTSUCHI_MODEL_PATH` — Vaporetto tokenizer model path

### Sensitive Data Masking

MCP responses have sensitive data masking enabled by default. Configure via the `[sensitive_data]` section in `~/.config/shiotsuchi/config.toml` (MCP reads from the same config file as the CLI):

```toml
[sensitive_data]
detection = true           # default: true (safe by default)
patterns = []              # optional additional regex patterns
```

When `detection = true`, API keys, email addresses, tokens, and other secrets detected in search snippets are replaced with placeholder strings like `[API_KEY]`, `[EMAIL]`, etc. This masking is applied only on output and does not affect stored data.

## Claude Desktop Setup

Add to `claude_desktop_config.json`:
```json
{
  "mcpServers": {
    "shiotsuchi": {
      "command": "/usr/local/bin/shiotsuchi-mcp",
      "env": {
        "SHIOTSUCHI_NOTES_DIR": "/Users/name/Notes",
        "SHIOTSUCHI_DB_PATH": "/home/name/.cache/shiotsuchi/db.sqlite3"
      }
    }
  }
}
```
