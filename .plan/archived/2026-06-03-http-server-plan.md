# HTTP Server Mode (`shiotsuchi serve`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an HTTP API server to shiotsuchi-search, exposing search, stats, list, and health endpoints over REST with CORS support and graceful shutdown.

**Architecture:** Handler functions in `core/src/server/` that accept `State<AppState>` and query params, testable via `axum::test` without server startup. CLI `serve` command wires up DB, tokenizer, and config into `AppState`.

**Tech Stack:** axum, tower-http (CORS), serde/serde_json, tokio (async runtime), rusqlite (existing)

**TDD Rule:** In Rust, function signatures must exist for tests to compile. We define signatures with `todo!()` bodies first, write tests, verify they panic (RED), then implement (GREEN). This is the pragmatic Rust TDD approach.

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `core/src/config.rs` | Modify | Add `ServerConfig` struct + field to `ShiotsuchiConfig` |
| `core/src/server/mod.rs` | Create | Module declarations |
| `core/src/server/types.rs` | Create | `ApiError`, `SearchParams`, response types |
| `core/src/server/handlers.rs` | Create | `AppState`, `create_router`, handler functions + tests |
| `core/src/server/cors.rs` | Create | `create_cors_layer()` — stub in Task 3, TDD in Tasks 17-18 |
| `core/src/lib.rs` | Modify | Add `pub mod server;` |
| `core/Cargo.toml` | Modify | Add axum, tower-http, tower dependencies |
| `cli/src/commands/serve.rs` | Create | `ServeArgs`, `run_serve()` |
| `cli/src/commands/mod.rs` | Modify | Add `pub mod serve;` |
| `cli/src/main.rs` | Modify | Add `Serve` variant to `Commands` enum + match arm |

---

## Task 1: Add ServerConfig to core config (data type, no behavior)

**Files:**
- Modify: `core/src/config.rs`

- [ ] **Step 1: Add ServerConfig struct**

Add after the `WatcherConfig` struct (around line 216):

```rust
/// HTTP server configuration for `shiotsuchi serve`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub port: u16,
    pub host: String,
    pub cors_origins: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            port: 7171,
            host: "127.0.0.1".to_string(),
            cors_origins: vec!["http://localhost".to_string()],
        }
    }
}
```

- [ ] **Step 2: Add server field to ShiotsuchiConfig**

Add to the `ShiotsuchiConfig` struct (after the `embedder` field):

```rust
    /// HTTP server configuration.
    #[serde(default)]
    pub server: ServerConfig,
```

- [ ] **Step 3: Verify existing tests pass**

Run: `cargo test -p shiotsuchi-core`
Expected: All existing tests pass (the new field uses `#[serde(default)]` so existing configs without `[server]` still parse).

- [ ] **Step 4: Commit**

```bash
git add core/src/config.rs
git commit -m "feat(core): add ServerConfig for HTTP server mode"
```

---

## Task 2: Add dependencies to core/Cargo.toml

**Files:**
- Modify: `core/Cargo.toml`

- [ ] **Step 1: Add axum, tower-http, tower dependencies**

Add to `[dependencies]` in `core/Cargo.toml`:

```toml
axum = "0.8"
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["cors"] }
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add core/Cargo.toml
git commit -m "chore(core): add axum, tower-http dependencies for HTTP server"
```

---

## Task 3: Create server types + cors stub (data types, no behavior)

**Files:**
- Create: `core/src/server/types.rs`
- Create: `core/src/server/cors.rs` (stub with `todo!()`)
- Create: `core/src/server/mod.rs` (initial, minimal)

- [ ] **Step 1: Create server/mod.rs with minimal module declaration**

Create `core/src/server/mod.rs`:

```rust
pub mod handlers;
pub mod types;
pub mod cors;
```

- [ ] **Step 2: Create server/types.rs with ApiError**

Create `core/src/server/types.rs`:

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Structured API error type.
#[derive(Debug)]
pub enum ApiError {
    /// 400 — invalid request parameters
    BadRequest(String),
    /// 404 — resource not found
    NotFound(String),
    /// 500 — internal server error
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            ApiError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL_ERROR",
                msg,
            ),
        };
        let body = Json(json!({
            "error": { "code": code, "message": message }
        }));
        (status, body).into_response()
    }
}
```

- [ ] **Step 3: Add response types and query params**

Add to `core/src/server/types.rs`:

```rust
// --- Query Parameters ---

#[derive(Deserialize)]
pub struct SearchParams {
    /// Search query (required)
    pub q: String,
    /// Maximum results to return (default: 20)
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Search mode: "fts", "vec", or "hybrid" (default: "hybrid")
    #[serde(default = "default_mode")]
    pub mode: String,
    /// Filter by vault name
    pub vault: Option<String>,
    /// Filter by tag
    pub tag: Option<String>,
    /// Filter by date (YYYY-MM-DD)
    pub since: Option<String>,
}

fn default_limit() -> usize {
    20
}

fn default_mode() -> String {
    "hybrid".to_string()
}

// --- Response Types ---

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResultItem>,
    pub count: usize,
}

#[derive(Serialize)]
pub struct SearchResultItem {
    pub file_path: String,
    pub title: String,
    pub parent_header: Option<String>,
    pub snippet: String,
    pub score: f64,
    pub vault_name: String,
}

#[derive(Serialize)]
pub struct StatsResponse {
    pub total_files: usize,
    pub total_chunks: usize,
    pub total_size_bytes: usize,
    pub last_indexed_at: Option<i64>,
    pub db_path: String,
    pub embedder_status: String,
    pub top_tags: Vec<(String, usize)>,
}

#[derive(Serialize)]
pub struct ListResponse {
    pub files: Vec<FileItem>,
    pub count: usize,
}

#[derive(Serialize)]
pub struct FileItem {
    pub path: String,
    pub vault_name: String,
}
```

- [ ] **Step 4: Create server/cors.rs stub**

Create `core/src/server/cors.rs` (stub — will be implemented in Tasks 17-18):

```rust
use crate::config::ServerConfig;
use tower_http::cors::CorsLayer;

/// Create a CORS layer from server configuration.
pub fn create_cors_layer(_server_config: &ServerConfig) -> CorsLayer {
    todo!("CORS layer implementation")
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles without errors. The `todo!()` body compiles but panics at runtime.

- [ ] **Step 6: Commit**

```bash
git add core/src/server/
git commit -m "feat(core): add server types, cors stub (compiles, no behavior)"
```

---

## Task 4: Create server module in lib.rs

**Files:**
- Modify: `core/src/lib.rs`

- [ ] **Step 1: Add server module declaration**

Add to `core/src/lib.rs` (after the existing `pub mod watcher;` line):

```rust
pub mod server;
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles without errors.

- [ ] **Step 3: Commit**

```bash
git add core/src/lib.rs
git commit -m "feat(core): register server module in lib.rs"
```

---

## Task 5: Define handler signatures + AppState + create_router (compiles, no behavior)

**Files:**
- Create: `core/src/server/handlers.rs`

This task creates the function signatures with `todo!()` bodies and the `AppState` struct. This is NOT production code — it's the equivalent of defining an interface. Tests will reference these signatures.

- [ ] **Step 1: Create handlers.rs with AppState, function signatures, and todo!() stubs**

Create `core/src/server/handlers.rs`:

```rust
use crate::config::ShiotsuchiConfig;
use crate::db::NoteDatabase;
use crate::server::types::*;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use std::collections::HashMap;
use std::sync::Arc;

/// Shared application state.
pub struct AppState {
    pub db: Arc<NoteDatabase>,
    pub tokenizer: Arc<crate::tokenizer::JapaneseTokenizer>,
    pub synonyms: HashMap<String, Vec<String>>,
    pub hybrid_alpha: Option<f64>,
}

/// Health check endpoint.
pub async fn handle_health() -> Json<serde_json::Value> {
    todo!()
}

/// Search endpoint.
pub async fn handle_search(
    State(_state): State<Arc<AppState>>,
    _params: axum::extract::Query<SearchParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    todo!()
}

/// Stats endpoint.
pub async fn handle_stats(
    State(_state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    todo!()
}

/// List indexed files endpoint.
pub async fn handle_list(
    State(_state): State<Arc<AppState>>,
    _config: axum::extract::Extension<ShiotsuchiConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    todo!()
}

/// Create the axum router with all routes.
pub fn create_router(state: Arc<AppState>, config: &ShiotsuchiConfig) -> Router {
    use crate::server::cors::create_cors_layer;

    let cors = create_cors_layer(&config.server);

    Router::new()
        .route("/api/v1/health", get(handle_health))
        .route("/api/v1/search", get(handle_search))
        .route("/api/v1/stats", get(handle_stats))
        .route("/api/v1/list", get(handle_list))
        .layer(cors)
        .layer(axum::extract::Extension(config.clone()))
        .with_state(state)
}
```

- [ ] **Step 2: Verify compilation (not test, just check)**

Run: `cargo check -p shiotsuchi-core`
Expected: Compiles without errors. The `todo!()` bodies are valid Rust — they compile but panic at runtime.

- [ ] **Step 3: Commit**

```bash
git add core/src/server/handlers.rs
git commit -m "feat(core): add handler signatures with todo!() stubs"
```

---

## Task 6: Health handler — RED

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Write the failing test**

Add to `core/src/server/handlers.rs` (at the bottom of the file):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tempfile::TempDir;
    use tower::ServiceExt;

    /// Build a test router with in-memory DB.
    fn setup_test_router() -> (Router, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = NoteDatabase::open(&db_path).unwrap();
        let tokenizer = match crate::tokenizer::get_tokenizer() {
            Ok(t) => t,
            Err(_) => panic!("Tokenizer model not available — skipping server tests"),
        };
        let state = Arc::new(AppState {
            db: Arc::new(db),
            tokenizer,
            synonyms: HashMap::new(),
            hybrid_alpha: None,
        });
        let router = create_router(state, &ShiotsuchiConfig::default());
        (router, tmp)
    }

    #[tokio::test]
    async fn test_health_returns_ok() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
```

- [ ] **Step 2: Run test to verify it FAILS (RED)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_health_returns_ok`
Expected: FAIL with "not yet implemented" (panic from `todo!()`).

**Verify:** The test fails because `handle_health` has `todo!()`, NOT because of a typo or compilation error. If it fails for a different reason, fix that first.

- [ ] **Step 3: Commit (RED state)**

```bash
git add core/src/server/handlers.rs
git commit -m "test(core): add health handler test (RED)"
```

---

## Task 7: Health handler — GREEN

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Implement handle_health**

Replace the `handle_health` function body:

```rust
/// Health check endpoint.
pub async fn handle_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
```

- [ ] **Step 2: Run test to verify it PASSES (GREEN)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_health_returns_ok`
Expected: PASS.

- [ ] **Step 3: Run all existing tests to verify no regressions**

Run: `cargo test -p shiotsuchi-core`
Expected: All tests pass.

- [ ] **Step 4: Commit (GREEN state)**

```bash
git add core/src/server/handlers.rs
git commit -m "feat(core): implement health handler (GREEN)"
```

---

## Task 8: Search handler — RED (validation tests)

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Write failing tests for search validation**

Add to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn test_search_missing_query_param() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_search_empty_query() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search?q=")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_search_invalid_mode() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search?q=test&mode=invalid")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }
```

- [ ] **Step 2: Run tests to verify they FAIL (RED)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_search`
Expected: All 3 tests FAIL with "not yet implemented" (panic from `todo!()`).

**Verify:** Each test fails because `handle_search` has `todo!()`. If any fails for a different reason, fix that first.

- [ ] **Step 3: Commit (RED state)**

```bash
git add core/src/server/handlers.rs
git commit -m "test(core): add search validation tests (RED)"
```

---

## Task 9: Search handler — GREEN (validation)

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Implement handle_search with validation only**

Replace the `handle_search` function body:

```rust
/// Search endpoint.
pub async fn handle_search(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Validate query
    let query = params.q.trim().to_string();
    if query.is_empty() {
        return Err(ApiError::BadRequest(
            "query parameter 'q' is required".to_string(),
        ));
    }

    // Parse search mode
    let _mode = match params.mode.as_str() {
        "fts" => crate::models::SearchMode::Fts,
        "vec" => crate::models::SearchMode::Vec,
        "hybrid" => crate::models::SearchMode::Hybrid,
        other => {
            return Err(ApiError::BadRequest(format!(
                "invalid mode '{}': must be 'fts', 'vec', or 'hybrid'",
                other
            )));
        }
    };

    // TODO: actual search implementation
    todo!("search implementation")
}
```

- [ ] **Step 2: Run validation tests to verify they PASS (GREEN)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_search_missing_query_param server::handlers::tests::test_search_empty_query server::handlers::tests::test_search_invalid_mode`
Expected: All 3 tests PASS.

- [ ] **Step 3: Commit (GREEN state)**

```bash
git add core/src/server/handlers.rs
git commit -m "feat(core): implement search validation (GREEN)"
```

---

## Task 10: Search handler — RED (results test)

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Write failing test for search results format**

Add to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn test_search_returns_results_format() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search?q=test")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("results").is_some());
        assert!(json.get("count").is_some());
        assert!(json["count"].is_number());
    }

    #[tokio::test]
    async fn test_search_with_limit() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search?q=test&limit=5")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
```

- [ ] **Step 2: Run tests to verify they FAIL (RED)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_search_returns_results_format server::handlers::tests::test_search_with_limit`
Expected: Both tests FAIL with "not yet implemented" (panic from `todo!("search implementation")`).

- [ ] **Step 3: Commit (RED state)**

```bash
git add core/src/server/handlers.rs
git commit -m "test(core): add search results format test (RED)"
```

---

## Task 11: Search handler — GREEN (results implementation)

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Implement full handle_search**

Replace the `handle_search` function body with the complete implementation:

```rust
/// Search endpoint.
pub async fn handle_search(
    State(state): State<Arc<AppState>>,
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Validate query
    let query = params.q.trim().to_string();
    if query.is_empty() {
        return Err(ApiError::BadRequest(
            "query parameter 'q' is required".to_string(),
        ));
    }

    // Parse search mode
    let mode = match params.mode.as_str() {
        "fts" => crate::models::SearchMode::Fts,
        "vec" => crate::models::SearchMode::Vec,
        "hybrid" => crate::models::SearchMode::Hybrid,
        other => {
            return Err(ApiError::BadRequest(format!(
                "invalid mode '{}': must be 'fts', 'vec', or 'hybrid'",
                other
            )));
        }
    };

    let results = crate::search::search(
        &state.db,
        &state.tokenizer,
        &query,
        params.limit,
        mode,
        None, // no embedder (text-only search)
        None, // no min_score
        params.vault.as_deref(),
        params.tag.as_deref(),
        params.since.as_deref(),
        &[],                          // user_dictionary
        &state.synonyms,              // synonyms from config
        false,                        // fuzzy
        state.hybrid_alpha,           // alpha from config
        false,                        // mmr
        0.7,                          // lambda
        false,                        // backlink_scoring
    )
    .map_err(|e| ApiError::Internal(format!("search failed: {}", e)))?;

    let items: Vec<SearchResultItem> = results
        .into_iter()
        .map(|r| SearchResultItem {
            file_path: r.file_path,
            title: r.title,
            parent_header: r.parent_header,
            snippet: crate::search::extract_snippet(&r.content, &query, 5, 200),
            score: r.score,
            vault_name: r.vault_name,
        })
        .collect();

    let count = items.len();
    Ok(Json(serde_json::json!({
        "results": items,
        "count": count,
    })))
}
```

- [ ] **Step 2: Run all search tests to verify they PASS (GREEN)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_search`
Expected: All search tests PASS (validation + results).

- [ ] **Step 3: Run all existing tests to verify no regressions**

Run: `cargo test -p shiotsuchi-core`
Expected: All tests pass.

- [ ] **Step 4: Commit (GREEN state)**

```bash
git add core/src/server/handlers.rs
git commit -m "feat(core): implement search handler (GREEN)"
```

---

## Task 12: Stats handler — RED

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Write failing test for stats**

Add to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn test_stats_returns_expected_fields() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/stats")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("total_files").is_some());
        assert!(json.get("total_chunks").is_some());
        assert!(json.get("db_path").is_some());
    }
```

- [ ] **Step 2: Run test to verify it FAILS (RED)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_stats_returns_expected_fields`
Expected: FAIL with "not yet implemented" (panic from `todo!()`).

- [ ] **Step 3: Commit (RED state)**

```bash
git add core/src/server/handlers.rs
git commit -m "test(core): add stats handler test (RED)"
```

---

## Task 13: Stats handler — GREEN

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Implement handle_stats**

Replace the `handle_stats` function body:

```rust
/// Stats endpoint.
pub async fn handle_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let stats = state
        .db
        .stats()
        .map_err(|e| ApiError::Internal(format!("failed to get stats: {}", e)))?;

    Ok(Json(serde_json::json!({
        "total_files": stats.total_files,
        "total_chunks": stats.total_chunks,
        "total_size_bytes": stats.total_size_bytes,
        "last_indexed_at": stats.last_indexed_at,
        "db_path": stats.db_path.to_string_lossy(),
        "embedder_status": stats.embedder_status,
        "top_tags": stats.top_tags,
    })))
}
```

- [ ] **Step 2: Run test to verify it PASSES (GREEN)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_stats_returns_expected_fields`
Expected: PASS.

- [ ] **Step 3: Run all existing tests to verify no regressions**

Run: `cargo test -p shiotsuchi-core`
Expected: All tests pass.

- [ ] **Step 4: Commit (GREEN state)**

```bash
git add core/src/server/handlers.rs
git commit -m "feat(core): implement stats handler (GREEN)"
```

---

## Task 14: List handler — RED

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Write failing test for list**

Add to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn test_list_returns_empty_when_no_files() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/list")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["count"], 0);
        assert_eq!(json["files"], serde_json::json!([]));
    }
```

- [ ] **Step 2: Run test to verify it FAILS (RED)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_list_returns_empty_when_no_files`
Expected: FAIL with "not yet implemented" (panic from `todo!()`).

- [ ] **Step 3: Commit (RED state)**

```bash
git add core/src/server/handlers.rs
git commit -m "test(core): add list handler test (RED)"
```

---

## Task 15: List handler — GREEN

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Implement handle_list**

Replace the `handle_list` function body:

```rust
/// List indexed files endpoint.
pub async fn handle_list(
    State(state): State<Arc<AppState>>,
    config: axum::extract::Extension<ShiotsuchiConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut files = Vec::new();
    for (vault_name, _vault_path) in config.resolved_vaults() {
        match state.db.list_cached_paths(&vault_name) {
            Ok(paths) => {
                for path in paths {
                    files.push(FileItem {
                        path,
                        vault_name: vault_name.clone(),
                    });
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to list files for vault '{}': {}", vault_name, e);
            }
        }
    }
    let count = files.len();
    Ok(Json(serde_json::json!({
        "files": files,
        "count": count,
    })))
}
```

- [ ] **Step 2: Run test to verify it PASSES (GREEN)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_list_returns_empty_when_no_files`
Expected: PASS.

- [ ] **Step 3: Run all existing tests to verify no regressions**

Run: `cargo test -p shiotsuchi-core`
Expected: All tests pass.

- [ ] **Step 4: Commit (GREEN state)**

```bash
git add core/src/server/handlers.rs
git commit -m "feat(core): implement list handler (GREEN)"
```

---

## Task 16: Error response format — RED

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Write failing test for error format**

Add to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn test_error_response_format() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "BAD_REQUEST");
        assert!(json["error"]["message"].is_string());
    }
```

- [ ] **Step 2: Run test to verify it PASSES (GREEN)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_error_response_format`
Expected: PASS. (This test should already pass because `ApiError::into_response` is implemented in Task 3.)

- [ ] **Step 3: Commit**

```bash
git add core/src/server/handlers.rs
git commit -m "test(core): add error response format test"
```

---

## Task 17: CORS — RED

**Files:**
- Create: `core/src/server/cors.rs`

- [ ] **Step 1: Write failing test for CORS**

Create `core/src/server/cors.rs` with test only:

```rust
use crate::config::ServerConfig;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cors_allows_localhost() {
        let config = ServerConfig::default();
        let _layer = super::create_cors_layer(&config);
        // If this compiles and runs, the CORS layer is configured correctly
    }

    #[test]
    fn test_cors_custom_origins() {
        let config = ServerConfig {
            cors_origins: vec!["http://localhost:3000".to_string()],
            ..Default::default()
        };
        let _layer = super::create_cors_layer(&config);
    }
}
```

- [ ] **Step 2: Run tests to verify they FAIL (RED)**

Run: `cargo test -p shiotsuchi-core server::cors`
Expected: FAIL — `create_cors_layer` function not found.

- [ ] **Step 3: Commit (RED state)**

```bash
git add core/src/server/cors.rs
git commit -m "test(core): add CORS tests (RED)"
```

---

## Task 18: CORS — GREEN

**Files:**
- Modify: `core/src/server/cors.rs`

- [ ] **Step 1: Implement create_cors_layer**

Add to `core/src/server/cors.rs` (before the `#[cfg(test)]` block):

```rust
use axum::http::{HeaderValue, Method};
use tower_http::cors::{AllowHeaders, CorsLayer};

/// Create a CORS layer from server configuration.
pub fn create_cors_layer(server_config: &ServerConfig) -> CorsLayer {
    let origins: Vec<HeaderValue> = server_config
        .cors_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::OPTIONS])
        .allow_headers(AllowHeaders::any())
}
```

- [ ] **Step 2: Run tests to verify they PASS (GREEN)**

Run: `cargo test -p shiotsuchi-core server::cors`
Expected: All CORS tests PASS.

- [ ] **Step 3: Run all existing tests to verify no regressions**

Run: `cargo test -p shiotsuchi-core`
Expected: All tests pass.

- [ ] **Step 4: Commit (GREEN state)**

```bash
git add core/src/server/cors.rs
git commit -m "feat(core): implement CORS layer (GREEN)"
```

---

## Task 19: CORS integration tests — RED

**Files:**
- Modify: `core/src/server/handlers.rs`

- [ ] **Step 1: Write CORS integration tests**

Add to the `#[cfg(test)] mod tests` block in `handlers.rs`:

```rust
    #[tokio::test]
    async fn test_cors_preflight_returns_ok() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/api/v1/search")
            .header("Origin", "http://localhost")
            .header("Access-Control-Request-Method", "GET")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers().contains_key("access-control-allow-origin"),
            "CORS header missing"
        );
    }

    #[tokio::test]
    async fn test_cors_rejects_non_localhost() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/health")
            .header("Origin", "http://evil.com")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        let has_cors = resp.headers().contains_key("access-control-allow-origin");
        assert!(!has_cors || resp.status() == StatusCode::FORBIDDEN);
    }
```

- [ ] **Step 2: Run tests to verify they PASS (GREEN)**

Run: `cargo test -p shiotsuchi-core server::handlers::tests::test_cors`
Expected: Both CORS integration tests PASS. (CORS is already implemented in Task 18.)

- [ ] **Step 3: Commit**

```bash
git add core/src/server/handlers.rs
git commit -m "test(core): add CORS integration tests"
```

---

## Task 20: CLI serve command

**Files:**
- Create: `cli/src/commands/serve.rs`
- Modify: `cli/src/commands/mod.rs`
- Modify: `cli/src/main.rs`

- [ ] **Step 1: Create serve.rs**

Create `cli/src/commands/serve.rs`:

```rust
use clap::Parser;
use shiotsuchi_core::config::ShiotsuchiConfig;
use shiotsuchi_core::db::NoteDatabase;
use shiotsuchi_core::server::handlers::{AppState, create_router};
use shiotsuchi_core::tokenizer::get_tokenizer;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(about = "Start HTTP API server")]
pub struct ServeArgs {
    /// Port to listen on (overrides config)
    #[arg(short, long)]
    pub port: Option<u16>,

    /// Host to bind to (overrides config)
    #[arg(long)]
    pub host: Option<String>,
}

pub async fn run_serve(
    args: &ServeArgs,
    config: &ShiotsuchiConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let port = args.port.unwrap_or(config.server.port);
    let host = args
        .host
        .clone()
        .unwrap_or_else(|| config.server.host.clone());

    let db_path = config.resolved_db_path();
    if !db_path.exists() {
        eprintln!(
            "Error: database not found at {}. Run 'shiotsuchi index' first.",
            db_path.display()
        );
        std::process::exit(1);
    }
    let db = NoteDatabase::open(&db_path)?;

    let tokenizer = get_tokenizer().map_err(|e| {
        eprintln!("Error: tokenizer not available: {}. Run 'shiotsuchi setup'.", e);
        e
    })?;

    let state = Arc::new(AppState {
        db: Arc::new(db),
        tokenizer,
        synonyms: config.synonyms.clone(),
        hybrid_alpha: config.hybrid_alpha,
    });

    let app = create_router(state, config);

    let addr = format!("{}:{}", host, port);
    println!("shiotsuchi server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::AddrInUse {
            format!(
                "Port {} is already in use. Specify a different port with --port",
                port
            )
        } else {
            format!("Failed to bind to {}: {}", addr, e)
        }
    })?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    println!("Server shut down gracefully.");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("Shutdown signal received, starting graceful shutdown...");
}
```

- [ ] **Step 2: Add serve module to mod.rs**

Add to `cli/src/commands/mod.rs`:

```rust
pub mod serve;
```

- [ ] **Step 3: Add Serve variant to Commands enum**

Add to `cli/src/main.rs` Commands enum (after the `Tide` variant):

```rust
    #[command(about = "Start HTTP API server")]
    Serve(commands::serve::ServeArgs),
```

- [ ] **Step 4: Add match arm for Serve command**

Add to `cli/src/main.rs` main function match block:

```rust
Some(Commands::Serve(args)) => {
    if let Err(e) = commands::serve::run_serve(&args, &cfg).await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
```

- [ ] **Step 5: Verify compilation**

Run: `cargo check -p shiotsuchi`
Expected: Compiles without errors.

- [ ] **Step 6: Commit**

```bash
git add cli/src/commands/serve.rs cli/src/commands/mod.rs cli/src/main.rs
git commit -m "feat(cli): add shiotsuchi serve command"
```

---

## Task 21: Full test suite verification

**Files:**
- None (verification only)

- [ ] **Step 1: Run all workspace tests**

Run: `cargo test`
Expected: All tests pass across all crates.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace`
Expected: No warnings or errors.

- [ ] **Step 3: Verify CLI help**

Run: `cargo run -p shiotsuchi -- serve --help`
Expected: Shows help text with `--port` and `--host` options.

- [ ] **Step 4: Commit any fixes**

```bash
git add -A
git commit -m "fix: address clippy warnings for HTTP server"
```
