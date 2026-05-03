# MCP Server (shiotsuchi-mcp)

Binary name: `shiotsuchi-mcp`
Crate path: `mcp/`

## Protocol

Implements [Model Context Protocol (MCP)](https://modelcontextprotocol.io/) over stdio JSON-RPC 2.0.

## Available Tools

### `search_vault`

Search the user's Markdown vault for notes matching a query.

**Input**: `{ "query": string }`
**Output**: JSON array of search results with paths, snippets, and scores

### `read_full_note`

Read the complete Markdown content of a specific note.

**Input**: `{ "path": string }` (relative path within vault)
**Output**: Markdown text content

**Security**: Path traversal protection — rejects `..`, absolute paths, and paths escaping the vault directory.

### `vault_status`

Get vault indexing statistics.

**Input**: `{}`
**Output**: Plain text with total notes, DB size, last indexed time

## JSON-RPC Methods

| Method | Description |
|--------|-------------|
| `initialize` | Server capabilities handshake |
| `tools/list` | List available tools |
| `tools/call` | Execute a tool |
| `ping` | Health check |

## Implementation Files

- `mcp/src/main.rs` — Stdio JSON-RPC loop, dispatch routing
- `mcp/src/protocol.rs` — `McpRequest`, `McpResponse`, `McpError`, `McpNotification`
- `mcp/src/tools.rs` — Tool definitions (JSON Schema)
- `mcp/src/handler.rs` — Tool execution logic

## Error Handling

- Internal errors are mapped to generic `"Internal tool execution error"` to avoid information leakage
- Uses `get_tokenizer()` for cached tokenizer access (avoids per-request model init cost)
- Opens DB connection per request

## Configuration

Environment variables:
- `SHIOTSUCHI_NOTES_DIR` — Vault root (default: `.`)
- `SHIOTSUCHI_DB_PATH` — Database path (default: `~/.cache/shiotsuchi/db.sqlite3`)

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
