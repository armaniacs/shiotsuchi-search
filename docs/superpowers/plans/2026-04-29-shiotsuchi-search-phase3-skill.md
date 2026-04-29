# Shiotsuchi-Search Phase 3: Kilo Skill Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `shiotsuchi-skill` binary (`skill/` crate) implementing the Kilo skill protocol, exposing `search-vault`, `read-note`, `vault-status` commands over JSON-RPC stdio.

**Architecture:** A Rust binary crate (`skill/`) that is a thin wrapper around `obsidian-shiotsuchi-vault-core`. Communicates with the Kilo agent via JSON-RPC 2.0 over stdin/stdout. Reads configuration from `~/.shiotsuchi/config.toml`.

**Tech Stack:** Rust, serde, serde_json, obsidian-shiotsuchi-vault-core

**Prerequisite:** Phase 1 complete — `obsidian-shiotsuchi-vault-core` builds and all tests pass.

> **Note on Kilo Protocol:** Before implementing, investigate the exact Kilo skill protocol version in use (`kilo agent --version`, inspect existing skill manifests in `~/.config/killo/agents/skills/`). The JSON-RPC structure below is based on the design spec; adjust if the real protocol differs.

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
skill/
├── Cargo.toml
├── skill.yaml              # Kilo skill manifest
└── src/
    ├── main.rs             # Entry point + stdio JSON-RPC dispatch
    ├── handler.rs          # Command handlers (search, read, status)
    └── protocol.rs         # JSON-RPC message types
```

---

## Task 1: Skill Crate Skeleton

**TDD exception:** Cargo manifest and empty stubs have no testable behavior.

**Files:**
- Create: `skill/Cargo.toml`
- Create: `skill/src/main.rs`
- Create: `skill/skill.yaml`

- [ ] **Step 1: Investigate Kilo skill protocol**

Before writing code, inspect:
```bash
kilo agent --version
ls ~/.config/killo/agents/skills/
cat ~/.config/killo/agents/skills/*.yaml  # (or .json)
```
Note the exact manifest format, JSON-RPC version, and method naming convention.

- [ ] **Step 2: Write skill/Cargo.toml**

```toml
[package]
name = "shiotsuchi-skill"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true

[[bin]]
name = "shiotsuchi-skill"
path = "src/main.rs"

[dependencies]
obsidian-shiotsuchi-vault-core = { path = "../core" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
log = "0.4"
env_logger = "0.11"
```

- [ ] **Step 3: Write skill/skill.yaml**

Adjust structure to match the actual Kilo protocol discovered in Step 1.

```yaml
name: shiotsuchi-search
version: "0.1.0"
description: "Search your Markdown vault for relevant context"
binary: shiotsuchi-skill
commands:
  - name: search-vault
    description: Search notes matching a query. Returns paths, snippets, and scores.
    params:
      - name: query
        type: string
        required: true
  - name: read-note
    description: Read the full content of a specific note.
    params:
      - name: path
        type: string
        required: true
  - name: vault-status
    description: Get vault indexing statistics.
```

- [ ] **Step 4: Write skill/src/main.rs skeleton**

```rust
mod handler;
mod protocol;

fn main() {
    env_logger::init();
    // stdio JSON-RPC loop (implement in Task 3)
}
```

- [ ] **Step 5: Verify workspace compiles**

Run: `cargo check --workspace`
Expected: Compiles

- [ ] **Step 6: Commit**

```bash
git add skill/Cargo.toml skill/src/main.rs skill/skill.yaml Cargo.toml
git commit -m "chore(skill): initialize skill crate skeleton"
```

---

## Task 2: JSON-RPC Protocol Types (TDD)

**Files:**
- Create: `skill/src/protocol.rs`

- [ ] **(RED) Step 1: Write failing tests for protocol types**

Create `skill/src/protocol.rs` with test module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_deserialize() {
        // FAIL: JsonRpcRequest not defined yet
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"search-vault","params":{"query":"test"}}"#;
        let req: JsonRpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "search-vault");
        assert_eq!(req.id, 1);
    }

    #[test]
    fn test_response_serialize() {
        let resp = JsonRpcResponse::success(1, serde_json::json!({"results": []}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":1"));
    }

    #[test]
    fn test_error_response() {
        let resp = JsonRpcResponse::error(1, -32601, "Method not found");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("Method not found"));
    }
}
```

- [ ] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p shiotsuchi-skill protocol`
Expected: Compilation error — `JsonRpcRequest`, `JsonRpcResponse` not found

- [ ] **(GREEN) Step 3: Implement protocol.rs**

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn success(id: u64, result: Value) -> Self {
        Self { jsonrpc: "2.0".to_string(), id, result: Some(result), error: None }
    }

    pub fn error(id: u64, code: i64, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message: message.to_string() }),
        }
    }
}
```

- [ ] **(GREEN VERIFY) Step 4: Run protocol tests, confirm they pass**

Run: `cargo test -p shiotsuchi-skill protocol`
Expected: 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add skill/src/protocol.rs
git commit -m "feat(skill): add JSON-RPC protocol types"
```

---

## Task 3: Command Handlers (TDD)

**Files:**
- Create: `skill/src/handler.rs`

- [ ] **(RED) Step 1: Write failing tests for handlers**

Create `skill/src/handler.rs` with test module only:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn make_vault_with_db() -> (TempDir, std::path::PathBuf) {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("note.md"), "# Hello\n\nThis is a skill test.").unwrap();
        let db = temp.path().join("test.db");
        // Index the vault
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
    fn test_handle_search_vault() {
        // FAIL: handle_search_vault not defined yet
        let (temp, db) = make_vault_with_db();
        let result = handle_search_vault("skill test", temp.path(), &db, 10).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_handle_vault_status() {
        let (temp, db) = make_vault_with_db();
        let stats = handle_vault_status(&db).unwrap();
        assert_eq!(stats["total_notes"], 1);
    }

    #[test]
    fn test_handle_read_note() {
        let (temp, db) = make_vault_with_db();
        let content = handle_read_note("note.md", temp.path()).unwrap();
        assert!(content.contains("Hello"));
    }
}
```

- [ ] **(RED VERIFY) Step 2: Run tests, confirm they fail**

Run: `cargo test -p shiotsuchi-skill handler`
Expected: Compilation error — handlers not found

- [ ] **(GREEN) Step 3: Implement handler.rs**

```rust
use obsidian_shiotsuchi_vault_core::{
    db::NoteDatabase,
    models::SearchResult,
    search::search,
    tokenizer::{JapaneseTokenizer, TokenizerConfig},
};
use serde_json::Value;
use std::{fs, path::Path};

pub fn handle_search_vault(
    query: &str,
    notes_dir: &Path,
    db_path: &Path,
    limit: usize,
) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    let tokenizer = JapaneseTokenizer::new(TokenizerConfig::default())?;
    Ok(search(&db, &tokenizer, notes_dir, query, limit)?)
}

pub fn handle_vault_status(db_path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    let stats = db.stats()?;
    Ok(serde_json::json!({
        "total_notes": stats.total_notes,
        "total_size_bytes": stats.total_size_bytes,
        "last_indexed_at": stats.last_indexed_at,
    }))
}

pub fn handle_read_note(
    path: &str,
    notes_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
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
    Ok(fs::read_to_string(&canonical)?)
}
```

- [ ] **(GREEN VERIFY) Step 4: Run handler tests, confirm they pass**

Run: `cargo test -p shiotsuchi-skill handler`
Expected: 3 tests pass

- [ ] **Step 5: Commit**

```bash
git add skill/src/handler.rs
git commit -m "feat(skill): add search, read, and status handlers"
```

---

## Task 4: stdio JSON-RPC Loop (TDD)

**Files:**
- Modify: `skill/src/main.rs`

- [ ] **(RED) Step 1: Write failing test for dispatch**

Add test module to `main.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_unknown_method() {
        // FAIL: dispatch not defined yet
        let req = crate::protocol::JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "unknown-method".to_string(),
            params: None,
        };
        let resp = dispatch(req, std::path::Path::new("/tmp"), std::path::Path::new("/tmp/db"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
    }
}
```

- [ ] **(RED VERIFY) Step 2: Run test, confirm it fails**

Run: `cargo test -p shiotsuchi-skill`
Expected: Compilation error — `dispatch` not found

- [ ] **(GREEN) Step 3: Implement dispatch and stdio loop in main.rs**

```rust
mod handler;
mod protocol;

use protocol::{JsonRpcRequest, JsonRpcResponse};
use std::{
    io::{self, BufRead, Write},
    path::Path,
};

pub fn dispatch(req: JsonRpcRequest, notes_dir: &Path, db_path: &Path) -> JsonRpcResponse {
    let params = req.params.unwrap_or(serde_json::Value::Null);

    match req.method.as_str() {
        "search-vault" => {
            let query = params["query"].as_str().unwrap_or("");
            match handler::handle_search_vault(query, notes_dir, db_path, 20) {
                Ok(results) => JsonRpcResponse::success(req.id, serde_json::to_value(results).unwrap()),
                Err(e) => JsonRpcResponse::error(req.id, -32000, &e.to_string()),
            }
        }
        "read-note" => {
            let path = params["path"].as_str().unwrap_or("");
            match handler::handle_read_note(path, notes_dir) {
                Ok(content) => JsonRpcResponse::success(req.id, serde_json::json!({"content": content})),
                Err(e) => JsonRpcResponse::error(req.id, -32000, &e.to_string()),
            }
        }
        "vault-status" => {
            match handler::handle_vault_status(db_path) {
                Ok(stats) => JsonRpcResponse::success(req.id, stats),
                Err(e) => JsonRpcResponse::error(req.id, -32000, &e.to_string()),
            }
        }
        _ => JsonRpcResponse::error(req.id, -32601, "Method not found"),
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

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.is_empty() => l,
            _ => continue,
        };
        let resp = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => dispatch(req, &notes_dir, &db_path),
            Err(_) => JsonRpcResponse::error(0, -32700, "Parse error"),
        };
        writeln!(out, "{}", serde_json::to_string(&resp).unwrap()).ok();
        out.flush().ok();
    }
}
```

- [ ] **(GREEN VERIFY) Step 4: Run all skill tests, confirm they pass**

Run: `cargo test -p shiotsuchi-skill`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add skill/src/main.rs
git commit -m "feat(skill): add stdio JSON-RPC dispatch loop"
```

---

## Task 5: Kilo Registration and Manual Test

**TDD exception:** Registration and manual smoke test cannot be automated without a live Kilo environment.

- [ ] **Step 1: Build release binary**

```bash
cargo build -p shiotsuchi-skill --release
```

- [ ] **Step 2: Install and register skill**

```bash
cp target/release/shiotsuchi-skill /usr/local/bin/
cp skill/skill.yaml ~/.config/killo/agents/skills/shiotsuchi-search.yaml
```

Adjust paths to match local Kilo installation.

- [ ] **Step 3: Manual smoke test**

```bash
kilo agent run shiotsuchi-search search-vault --query "テスト"
```

Expected: JSON array of search results

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(skill): complete Kilo skill integration"
```

---

## Self-Review

### 1. Spec Coverage Check

| Spec Requirement | Plan Task |
|------------------|-----------|
| `search-vault` command | Task 3, 4 |
| `read-note` command | Task 3, 4 |
| `vault-status` command | Task 3, 4 |
| JSON-RPC 2.0 over stdio | Task 2, 4 |
| Skill manifest (`skill.yaml`) | Task 1 |
| Path traversal security check | Task 3 |
| Config via env vars | Task 4 |

### 2. TDD Cycle Compliance

- ✅ Task 1: TDD不適用（マニフェスト・空スタブ）と明示
- ✅ Task 2〜4: 各タスクに RED → RED VERIFY → GREEN → GREEN VERIFY
- ✅ Task 5: 手動テストのためTDD略、ビルドと登録手順を明示

### 3. テスト実行前提

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
    cargo test -p shiotsuchi-skill
```

---

## Next Steps

Phase 4: MCP — `mcp/` crate with `shiotsuchi-mcp` standalone binary
