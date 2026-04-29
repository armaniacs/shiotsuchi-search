# Shiotsuchi-Search Phase 4: MCP Server Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Build the `shiotsuchi-mcp` standalone binary (`mcp/` crate) implementing the MCP (Model Context Protocol) server, exposing `search_vault`, `read_full_note`, `vault_status` tools over JSON-RPC 2.0 stdio transport.

**Architecture:** A Rust binary crate (`mcp/`) that is a thin wrapper around `obsidian-shiotsuchi-vault-core`. Reads config from env vars (`SHIOTSUCHI_NOTES_DIR`, `SHIOTSUCHI_DB_PATH`). Communicates with Claude Desktop via stdio MCP protocol.

**Tech Stack:** Rust, serde, serde_json, obsidian-shiotsuchi-vault-core

**Prerequisite:** Phase 1 complete — `obsidian-shiotsuchi-vault-core` builds and all tests pass.

---

## 実装状況サマリー（2026-04-30 時点）

### ✅ 実装済み（Tasks 1–4 + リリースビルド）

- `mcp/` クレートの全ソースファイル（`Cargo.toml`, `protocol.rs`, `tools.rs`, `handler.rs`, `main.rs`）
- 13 テスト全パス（TDD サイクル RED→VERIFY→GREEN→VERIFY を全タスクで遵守）
- リリースバイナリのビルド確認済み（`cargo build -p shiotsuchi-mcp --release`）
- `initialize` / `tools/list` / `tools/call` / `ping` のスモークテスト済み

### ⚠️ 計画との差分（実装時に変更した点）

| 箇所 | 計画 | 実装 |
|------|------|------|
| `Cargo.toml` | `dirs` 依存なし | `dirs = "5"` を追加（`main()` のデフォルト db パス解決に必要） |
| `test_call_vault_status` | `temp.path()` を `notes_dir` として渡す | `/tmp` を渡す（vault_status は notes_dir を使わないため） |
| Task 4 テスト | `test_dispatch_tools_list`, `test_dispatch_unknown_method` の 2 件 | `test_dispatch_initialize`, `test_dispatch_ping` を追加（合計 4 件） |

### ❌ 未実施（手動作業が必要）

- `cp target/release/shiotsuchi-mcp /usr/local/bin/` — バイナリのシステムインストール
- `claude_desktop_config.json` への設定追記と Claude Desktop の再起動
- 実 vault を使った `search_vault` の動作確認（本物の Notes ディレクトリとインデックス済み DB が必要）

### 🔜 次にやること

**Phase 4 の残作業（手動）:**
1. vault をインデックス済みの DB で `shiotsuchi-mcp` を Claude Desktop に接続する
2. 実際のノートに対して `search_vault` を呼び出し、結果を目視確認する

**Phase 5（次フェーズ）:**
- `shiotsuchi chart` コマンドの完成（watcher / `scan` サブコマンド）
- ベンチマーク・エラー UX の改善
- README 整備

---

## TDD (Test-Driven Development) Approach

All implementation in this plan follows strict TDD cycles:

1. **RED** - Write a failing test for the desired behavior.
2. **RED VERIFY** - Run the test, confirm it fails (feature not yet implemented).
3. **GREEN** - Write minimal code to make the test pass.
4. **GREEN VERIFY** - Run the test, confirm it passes.
5. **REFACTOR** - Clean up code while keeping tests green.
6. Repeat for next behavior.

**Mandatory Rules:**
- Never write production code without a failing test first.
- If code was written before tests, delete it and start over.
- Verify RED before writing GREEN code — if the test passes immediately, the test is wrong.
- Verify GREEN before moving to next cycle.
- RED VERIFY is never skippable: watching the test fail is proof that it tests the right thing.

**Exception — Task 1 (Skeleton):** Cargo manifest and empty stubs have no testable behavior; TDD does not apply. All other tasks follow strict TDD.

---

## File Structure

```
mcp/
├── Cargo.toml
└── src/
    ├── main.rs          # Entry point + stdio MCP dispatch loop
    ├── protocol.rs      # MCP JSON-RPC message types
    ├── tools.rs         # Tool definitions (schema)
    └── handler.rs       # Tool call handlers
```

---

## Task 1: MCP Crate Skeleton

**TDD exception:** Cargo manifest and empty stubs have no testable behavior.

**Files:**
- Create: `mcp/Cargo.toml`
- Create: `mcp/src/main.rs`

- [x] **Step 1: Write mcp/Cargo.toml**

```toml
[package]
name = "shiotsuchi-mcp"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[[bin]]
name = "shiotsuchi-mcp"
path = "src/main.rs"

[dependencies]
obsidian-shiotsuchi-vault-core = { path = "../core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
log = "0.4"
env_logger = "0.11"
dirs = "5"
```

- [x] **Step 2: Write mcp/src/main.rs skeleton**

```rust
mod handler;
mod protocol;
mod tools;

fn main() {
    env_logger::init();
    // stdio MCP loop (implement in Task 4)
}
```

- [x] **Step 3: Verify workspace compiles**

Run: `cargo check --workspace`
Expected: Compiles

- [x] **Step 4: Commit**

```bash
git add mcp/Cargo.toml mcp/src/main.rs Cargo.toml
git commit -m "chore(mcp): initialize MCP crate skeleton"
```

---

## Task 2: MCP Protocol Types (TDD)

MCP uses JSON-RPC 2.0. Key message types: `initialize`, `tools/list`, `tools/call`.

**Files:**
- Create: `mcp/src/protocol.rs`

- [x] **(RED) Step 1: Write failing tests for protocol types**

Create `mcp/src/protocol.rs` with test module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_deserialize() {
        // FAIL: McpRequest not defined yet
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search_vault","arguments":{"query":"test"}}}"#;
        let req: McpRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "tools/call");
    }

    #[test]
    fn test_success_response_serialize() {
        let resp = McpResponse::success(1, serde_json::json!({"content": []}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"result\""));
    }

    #[test]
    fn test_error_response_serialize() {
        let resp = McpResponse::error(1, -32601, "Method not found");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("Method not found"));
    }

    #[test]
    fn test_notification_no_id() {
        let notif = McpNotification::new("notifications/initialized", serde_json::Value::Null);
        let json = serde_json::to_string(&notif).unwrap();
        assert!(!json.contains("\"id\""));
        assert!(json.contains("notifications/initialized"));
    }
}
```

- [x] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p shiotsuchi-mcp protocol`
Expected: Compilation error — `McpRequest`, `McpResponse`, `McpNotification` not found

- [x] **(GREEN) Step 3: Implement protocol.rs**

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,  // Can be number or string per JSON-RPC spec
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct McpNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl McpResponse {
    pub fn success(id: impl Into<Value>, result: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id: id.into(), result: Some(result), error: None }
    }

    pub fn error(id: impl Into<Value>, code: i64, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            result: None,
            error: Some(McpError { code, message: message.to_string() }),
        }
    }
}

impl McpNotification {
    pub fn new(method: &str, params: Value) -> Self {
        let params = if params.is_null() { None } else { Some(params) };
        Self { jsonrpc: "2.0".to_string(), method: method.to_string(), params }
    }
}
```

- [x] **(GREEN VERIFY) Step 4: Run protocol tests, confirm they pass**

Run: `cargo test -p shiotsuchi-mcp protocol`
Expected: 4 tests pass

- [x] **Step 5: Commit**

```bash
git add mcp/src/protocol.rs
git commit -m "feat(mcp): add MCP JSON-RPC protocol types"
```

---

## Task 3: Tool Definitions and Handlers (TDD)

**Files:**
- Create: `mcp/src/tools.rs`
- Create: `mcp/src/handler.rs`

- [x] **(RED) Step 1: Write failing tests for tool list and handlers**

Create `mcp/src/tools.rs` and `mcp/src/handler.rs` with test modules only:

`mcp/src/tools.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_list_has_three_tools() {
        // FAIL: tool_list() not defined yet
        let tools = tool_list();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"search_vault"));
        assert!(names.contains(&"read_full_note"));
        assert!(names.contains(&"vault_status"));
    }
}
```

`mcp/src/handler.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn indexed_vault() -> (TempDir, std::path::PathBuf) {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("note.md"), "# Hello\n\nMCP integration test.").unwrap();
        let db = temp.path().join("test.db");
        use obsidian_shiotsuchi_vault_core::{
            db::NoteDatabase, indexer::index_directory,
            models::IndexConfig, tokenizer::{JapaneseTokenizer, TokenizerConfig},
        };
        let ndb = NoteDatabase::open(&db).unwrap();
        let tok = JapaneseTokenizer::new(TokenizerConfig::default())
            .unwrap_or_else(|_| panic!("SHIOTSUCHI_MODEL_PATH を設定してください"));
        let cfg = IndexConfig { notes_dir: temp.path().to_path_buf(), ..Default::default() };
        index_directory(&ndb, &tok, &cfg).unwrap();
        (temp, db)
    }

    #[test]
    fn test_call_search_vault() {
        // FAIL: call_tool not defined yet
        let (temp, db) = indexed_vault();
        let args = serde_json::json!({"query": "MCP integration"});
        let result = call_tool("search_vault", &args, temp.path(), &db).unwrap();
        let content = &result["content"];
        assert!(content.is_array());
        assert!(!content.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_call_vault_status() {
        let (temp, db) = indexed_vault();
        let result = call_tool("vault_status", &serde_json::Value::Null, temp.path(), &db).unwrap();
        assert_eq!(result["content"][0]["text"].as_str().unwrap().contains("1"), true);
    }

    #[test]
    fn test_call_read_full_note() {
        let (temp, db) = indexed_vault();
        let args = serde_json::json!({"path": "note.md"});
        let result = call_tool("read_full_note", &args, temp.path(), &db).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Hello"));
    }

    #[test]
    fn test_path_traversal_rejected() {
        let (temp, db) = indexed_vault();
        let args = serde_json::json!({"path": "../secret.txt"});
        let result = call_tool("read_full_note", &args, temp.path(), &db);
        assert!(result.is_err());
    }
}
```

- [x] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p shiotsuchi-mcp`
Expected: Compilation errors — `tool_list`, `call_tool` not found

- [x] **(GREEN) Step 3: Implement tools.rs**

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

pub fn tool_list() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "search_vault".to_string(),
            description: "Search the user's Markdown vault for notes matching a query. Returns paths, snippets, and relevance scores.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Japanese or English search query" }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "read_full_note".to_string(),
            description: "Read the complete Markdown content of a specific note by its relative path within the vault.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path inside vault (e.g., 'projects/meeting.md')" }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "vault_status".to_string(),
            description: "Get vault indexing statistics: total notes, last updated, database size.".to_string(),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
    ]
}
```

- [x] **(GREEN) Step 4: Implement handler.rs**

```rust
use obsidian_shiotsuchi_vault_core::{
    db::NoteDatabase,
    search::search,
    tokenizer::{JapaneseTokenizer, TokenizerConfig},
};
use serde_json::Value;
use std::{fs, path::Path};

/// MCP tool call result format: {"content": [{"type": "text", "text": "..."}]}
fn text_content(text: impl Into<String>) -> Value {
    serde_json::json!({ "content": [{ "type": "text", "text": text.into() }] })
}

pub fn call_tool(
    name: &str,
    args: &Value,
    notes_dir: &Path,
    db_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    match name {
        "search_vault" => {
            let query = args["query"].as_str().unwrap_or("");
            let db = NoteDatabase::open(db_path)?;
            let tokenizer = JapaneseTokenizer::new(TokenizerConfig::default())?;
            let results = search(&db, &tokenizer, notes_dir, query, 20)?;
            let text = serde_json::to_string_pretty(&results)?;
            Ok(text_content(text))
        }
        "read_full_note" => {
            let path = args["path"].as_str().unwrap_or("");
            // Security: reject absolute paths and `..` traversal
            if path.starts_with('/') || path.contains("..") {
                return Err("Invalid path: must be relative and within vault".into());
            }
            let full_path = notes_dir.join(path);
            let canonical = full_path.canonicalize()?;
            let vault_canonical = notes_dir.canonicalize()?;
            if !canonical.starts_with(&vault_canonical) {
                return Err("Path escapes vault directory".into());
            }
            let content = fs::read_to_string(&canonical)?;
            Ok(text_content(content))
        }
        "vault_status" => {
            let db = NoteDatabase::open(db_path)?;
            let stats = db.stats()?;
            let text = format!(
                "Total notes: {}\nDB size: {} bytes\nLast indexed: {}",
                stats.total_notes,
                stats.total_size_bytes,
                stats.last_indexed_at.map(|t| t.to_string()).unwrap_or_else(|| "never".to_string()),
            );
            Ok(text_content(text))
        }
        _ => Err(format!("Unknown tool: {}", name).into()),
    }
}
```

- [x] **(GREEN VERIFY) Step 5: Run all MCP tests, confirm they pass**

Run: `cargo test -p shiotsuchi-mcp`
Expected: All tests pass

- [x] **Step 6: Commit**

```bash
git add mcp/src/tools.rs mcp/src/handler.rs
git commit -m "feat(mcp): add tool definitions and call handlers"
```

---

## Task 4: MCP Dispatch Loop (TDD)

**Files:**
- Modify: `mcp/src/main.rs`

- [x] **(RED) Step 1: Write failing test for dispatch**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_tools_list() {
        // FAIL: dispatch not defined yet
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(1),
            method: "tools/list".to_string(),
            params: None,
        };
        let resp = dispatch(req, std::path::Path::new("/tmp"), std::path::Path::new("/tmp/db"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("search_vault"));
    }

    #[test]
    fn test_dispatch_unknown_method() {
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(2),
            method: "unknown".to_string(),
            params: None,
        };
        let resp = dispatch(req, std::path::Path::new("/tmp"), std::path::Path::new("/tmp/db"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
    }
}
```

- [x] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p shiotsuchi-mcp`
Expected: Compilation error — `dispatch` not found

- [x] **(GREEN) Step 3: Implement dispatch and main.rs**

```rust
mod handler;
mod protocol;
mod tools;

use protocol::{McpNotification, McpRequest, McpResponse};
use std::{
    io::{self, BufRead, Write},
    path::Path,
};

pub fn dispatch(req: McpRequest, notes_dir: &Path, db_path: &Path) -> McpResponse {
    let params = req.params.clone().unwrap_or(serde_json::Value::Null);

    match req.method.as_str() {
        "initialize" => {
            McpResponse::success(req.id, serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "shiotsuchi-mcp", "version": env!("CARGO_PKG_VERSION") }
            }))
        }
        "tools/list" => {
            let tool_list = tools::tool_list();
            McpResponse::success(req.id, serde_json::json!({ "tools": tool_list }))
        }
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = &params["arguments"];
            match handler::call_tool(name, args, notes_dir, db_path) {
                Ok(result) => McpResponse::success(req.id, result),
                Err(e) => McpResponse::error(req.id, -32000, &e.to_string()),
            }
        }
        "ping" => McpResponse::success(req.id, serde_json::json!({})),
        _ => McpResponse::error(req.id, -32601, "Method not found"),
    }
}

fn main() {
    env_logger::init();

    let notes_dir = std::env::var("SHIOTSUCHI_NOTES_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let db_path = std::env::var("SHIOTSUCHI_DB_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir().unwrap_or_default()
                .join(".shiotsuchi").join("db.sqlite3")
        });

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    // Send initialized notification
    let notif = McpNotification::new("notifications/initialized", serde_json::Value::Null);
    writeln!(out, "{}", serde_json::to_string(&notif).unwrap()).ok();
    out.flush().ok();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.is_empty() => l,
            _ => continue,
        };
        let resp = match serde_json::from_str::<McpRequest>(&line) {
            Ok(req) => dispatch(req, &notes_dir, &db_path),
            Err(_) => McpResponse::error(0, -32700, "Parse error"),
        };
        writeln!(out, "{}", serde_json::to_string(&resp).unwrap()).ok();
        out.flush().ok();
    }
}
```

- [x] **(GREEN VERIFY) Step 4: Run all MCP tests, confirm they pass**

Run: `cargo test -p shiotsuchi-mcp`
Expected: All tests pass

- [x] **Step 5: Commit**

```bash
git add mcp/src/main.rs
git commit -m "feat(mcp): add MCP dispatch loop with initialize/tools/list/tools/call"
```

---

## Task 5: Claude Desktop Integration Test

**TDD exception:** Claude Desktop integration requires a live environment; cannot be automated.

- [x] **Step 1: Build release binary**

```bash
cargo build -p shiotsuchi-mcp --release
cp target/release/shiotsuchi-mcp /usr/local/bin/
```

- [x] **Step 2: Configure Claude Desktop**

Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "shiotsuchi": {
      "command": "/usr/local/bin/shiotsuchi-mcp",
      "env": {
        "SHIOTSUCHI_NOTES_DIR": "/Users/<name>/Notes",
        "SHIOTSUCHI_DB_PATH": "/Users/<name>/.shiotsuchi/db.sqlite3"
      }
    }
  }
}
```

- [x] **Step 3: Index vault first**

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  ./target/release/shiotsuchi chart \
  --notes-dir ~/Notes \
  --db-path ~/.shiotsuchi/db.sqlite3
```

- [x] **Step 4: Restart Claude Desktop and verify**

Open Claude Desktop → ask: "Search my notes for プロジェクト"
Expected: Claude calls `search_vault` and returns relevant snippets.

- [x] **Step 5: Commit**

```bash
git commit -m "feat(mcp): complete Claude Desktop MCP integration"
```

---

## Self-Review

### 1. Spec Coverage Check

| Spec Requirement | Plan Task |
|------------------|-----------|
| `search_vault` tool | Task 3, 4 |
| `read_full_note` tool | Task 3, 4 |
| `vault_status` tool | Task 3, 4 |
| JSON-RPC 2.0 over stdio | Task 2, 4 |
| `initialize` handshake | Task 4 |
| `tools/list` response | Task 4 |
| `tools/call` dispatch | Task 4 |
| Path traversal security | Task 3 |
| `SHIOTSUCHI_NOTES_DIR` / `SHIOTSUCHI_DB_PATH` env vars | Task 4 |
| Claude Desktop config example | Task 5 |

### 2. TDD Cycle Compliance

- ✅ Task 1: TDD不適用（マニフェスト・空スタブ）と明示
- ✅ Task 2〜4: 各タスクに RED → RED VERIFY → GREEN → GREEN VERIFY
- ✅ Task 5: Claude Desktop 統合は手動テスト、TDD略と明示

### 3. テスト実行前提

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
    cargo test -p shiotsuchi-mcp
```

---

## Next Steps

Phase 5: Polish — watcher `scan` command completion, benchmarks, error UX, README
