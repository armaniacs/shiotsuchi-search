# How to Use shiotsuchi-search with MCP

shiotsuchi-search exposes a Model Context Protocol (MCP) server (`shiotsuchi-mcp`) so that LLMs can search your Markdown vaults directly. This guide covers configuration and client setup for one or more vaults.

> For installation instructions, see [docs/INSTALL.md](INSTALL.md).

---

## Concepts

| Term | Meaning |
|------|---------|
| **vault** | A directory of Markdown files (e.g. an Obsidian vault) |
| **index** | The SQLite database built by `shiotsuchi chart` |
| **MCP server** | `shiotsuchi-mcp` — the stdio process that answers LLM tool calls |

One MCP server process = one vault. To search multiple vaults, run one process per vault and register each in your client.

---

## Step 1 — Index your vault

Before the MCP server can answer queries, the vault must be indexed:

```sh
shiotsuchi chart --notes-dir ~/Personal
```

For a second vault:

```sh
shiotsuchi chart --notes-dir ~/Work
```

Each run writes a SQLite database. The default location is `~/.cache/shiotsuchi/db.sqlite3`. Use a config file (see below) to set a separate path per vault.

---

## Step 2 — Create a config file per vault

`shiotsuchi-mcp` reads a TOML config file passed via `--config`. The format reuses the same schema as the CLI config.

### Personal vault

```toml
# ~/.config/shiotsuchi/personal.toml
notes_dir = "/Users/yourname/Personal"
db_path   = "/Users/yourname/.cache/shiotsuchi/personal.db"
```

### Work vault

```toml
# ~/.config/shiotsuchi/work.toml
notes_dir = "/Users/yourname/Work"
db_path   = "/Users/yourname/.cache/shiotsuchi/work.db"
```

Re-index with the matching db_path:

```sh
shiotsuchi chart --notes-dir ~/Personal --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi chart --notes-dir ~/Work     --db-path ~/.cache/shiotsuchi/work.db
```

If `--config` is omitted, `shiotsuchi-mcp` falls back to `~/.config/shiotsuchi/config.toml`, then to built-in defaults.

---

## Step 3 — Register the MCP server in your client

### Claude Desktop

Edit `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS) or `%APPDATA%\Claude\claude_desktop_config.json` (Windows):

```json
{
  "mcpServers": {
    "shiotsuchi-personal": {
      "command": "shiotsuchi-mcp",
      "args": ["--config", "/Users/yourname/.config/shiotsuchi/personal.toml"]
    },
    "shiotsuchi-work": {
      "command": "shiotsuchi-mcp",
      "args": ["--config", "/Users/yourname/.config/shiotsuchi/work.toml"]
    }
  }
}
```

Restart Claude Desktop. Both vaults appear as separate tool namespaces.

### Claude Code (CLI)

```sh
claude mcp add shiotsuchi-personal -- shiotsuchi-mcp --config ~/.config/shiotsuchi/personal.toml
claude mcp add shiotsuchi-work     -- shiotsuchi-mcp --config ~/.config/shiotsuchi/work.toml
```

Verify:

```sh
claude mcp list
```

### Generic MCP client

Any client that supports stdio MCP servers can launch the process directly:

```sh
shiotsuchi-mcp --config ~/.config/shiotsuchi/personal.toml
```

The server reads JSON-RPC requests from stdin and writes responses to stdout, following the MCP 2024-11-05 protocol.

---

## Available tools

Once connected, the LLM can call three tools per vault:

| Tool | Description |
|------|-------------|
| `search_vault` | Search notes by keyword or phrase. Returns paths, snippets, and scores. |
| `read_full_note` | Read the full Markdown content of a note by its relative path. |
| `vault_status` | Get indexing statistics: total notes, last indexed time, DB size. |

### Example interactions

```
User: What did I write about the Q3 budget in my work notes?
→ LLM calls search_vault(query: "Q3 budget") on shiotsuchi-work
→ LLM calls read_full_note(path: "finance/q3-review.md") to retrieve the full note
```

```
User: Summarize my recent personal notes on photography.
→ LLM calls search_vault(query: "photography") on shiotsuchi-personal
```

---

## Keep the index up to date

Run `shiotsuchi chart` again after adding or editing notes, or use the watcher to update continuously:

```sh
shiotsuchi scan --notes-dir ~/Personal --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi scan --notes-dir ~/Work     --db-path ~/.cache/shiotsuchi/work.db
```

---

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| LLM says "no results found" | Run `shiotsuchi chart` to (re-)index the vault |
| `shiotsuchi-mcp: command not found` | Ensure the binary is on `PATH`; see [INSTALL.md](INSTALL.md) |
| Config parse error in logs | Check TOML syntax; `notes_dir` and `db_path` must be absolute paths |
| Wrong vault searched | Verify `--config` path passed to each `mcpServers` entry |
| Notes added but not found | Re-run `shiotsuchi chart` or start `shiotsuchi scan` |

---

## Further reading

- [docs/INSTALL.md](INSTALL.md) — Build and install binaries
- [ref/cli.md](../ref/cli.md) — All CLI commands and flags
- [ref/mcp.md](../ref/mcp.md) — MCP protocol details
- [ref/architecture.md](../ref/architecture.md) — Design and data model
