# Design: HTTP Server Mode (`shiotsuchi serve`)

PBI: `pbi/2026-06-03-37-feat-serve-http-server.md`

## Overview

Add an HTTP API server to shiotsuchi-search, enabling Obsidian plugins and browser-based UIs to use the search engine over HTTP. The server exposes a versioned REST API (`/api/v1/...`) with structured JSON responses, CORS support, and graceful shutdown.

## Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Architecture | Handler functions in `core/src/server/` | Consistent with MCP handler pattern; testable without server startup |
| TDD granularity | Unit tests on handler functions | Fast, no mocking needed, `NoteDatabase::open_in_memory()` + `axum::test` |
| API versioning | `/api/v1/` prefix | Future-proof for breaking changes |
| Error format | `{"error": {"code": "...", "message": "..."}}` | Structured, client-parsable |
| State sharing | `Arc<NoteDatabase>` + `Arc<JapaneseTokenizer>` | WAL mode allows concurrent reads; `Arc` for axum `State` |
| CORS | Configurable `cors_origins` array | Supports multiple frontend origins |

## Architecture

### File Layout

```
core/src/
  server/
    mod.rs          # AppState, create_router(), public re-exports
    handlers.rs     # handle_search, handle_stats, handle_list, handle_health
    types.rs        # SearchParams, SearchResponse, StatsResponse, ListResponse, ApiError
    cors.rs         # create_cors_layer()
  config.rs         # Existing ShiotsuchiConfig + new ServerConfig

cli/src/
  commands/
    serve.rs        # ServeArgs, run_serve() — server startup
  main.rs           # Commands enum + Serve variant
```

### Dependency Flow

```
cli/src/commands/serve.rs
    ↓ uses
core/src/server/mod.rs (AppState, create_router)
    ↓ uses
core/src/server/handlers.rs (handle_*)
    ↓ uses
core/src/search.rs, core/src/db.rs (existing)
```

### AppState

```rust
pub struct AppState {
    pub db: Arc<NoteDatabase>,
    pub tokenizer: Arc<JapaneseTokenizer>,
}
```

- `Arc` shared via axum `State`
- WAL mode supports concurrent readers (multiple search requests)
- Tokenizer obtained via `get_tokenizer()` (OnceLock-cached), wrapped in `Arc` for explicit sharing

## Data Types (types.rs)

### Response Types

```rust
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
    pub snippet: String,       // extract_snippet() output
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

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}
```

### Query Parameters

```rust
#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,                           // Required
    #[serde(default = "default_limit")]
    pub limit: usize,                        // Default: 20
    #[serde(default = "default_mode")]
    pub mode: String,                        // "fts" | "vec" | "hybrid" — default: "hybrid"
    pub vault: Option<String>,               // Vault filter
    pub tag: Option<String>,                 // Tag filter
    pub since: Option<String>,               // Date filter (YYYY-MM-DD)
}

fn default_limit() -> usize { 20 }
fn default_mode() -> String { "hybrid".to_string() }
```

**Note:** `mode` uses `default_mode` (not `#[serde(default)]`) because `#[serde(default)]` deserializes to empty string `""`, not `"hybrid"`.

### Error Type

```rust
#[derive(Debug)]
pub enum ApiError {
    BadRequest(String),       // 400
    NotFound(String),         // 404
    Internal(String),         // 500
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, "NOT_FOUND", msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR", msg),
        };
        let body = Json(json!({
            "error": { "code": code, "message": message }
        }));
        (status, body).into_response()
    }
}
```

### search() Parameter Mapping

The existing `search()` function has 17 parameters. The HTTP API maps them as follows:

| search() param | Source | Default |
|---|---|---|
| `db` | `state.db` | — |
| `tokenizer` | `state.tokenizer` | — |
| `query` | `params.q` | Required |
| `limit` | `params.limit` | 20 |
| `mode` | `params.mode` → parse to `SearchMode` | `Hybrid` |
| `embedder` | `None` | Text-only search |
| `min_score` | `None` | No threshold |
| `vault_filter` | `params.vault` | `None` |
| `tag_filter` | `params.tag` | `None` |
| `since_date` | `params.since` | `None` |
| `user_dictionary` | `&[]` | Empty |
| `synonyms` | `&config.synonyms` | From config file |
| `fuzzy` | `false` | Standard search |
| `alpha` | `config.hybrid_alpha` | Config value |
| `mmr` | `false` | No MMR |
| `lambda` | `0.7` | Fixed |
| `backlink_scoring` | `false` | Disabled |

## API Endpoints

### Endpoints

| Method | Path | Response | Description |
|--------|------|----------|-------------|
| GET | `/api/v1/health` | `HealthResponse` | Server liveness check |
| GET | `/api/v1/search?q=...&limit=...&mode=...&vault=...&tag=...&since=...` | `SearchResponse` | Full-text search |
| GET | `/api/v1/stats` | `StatsResponse` | Index statistics |
| GET | `/api/v1/list` | `ListResponse` | Indexed file list |

### Router

```rust
pub fn create_router(state: Arc<AppState>, config: &ShiotsuchiConfig) -> Router {
    let cors = create_cors_layer(&config.server);
    Router::new()
        .route("/api/v1/health", get(handlers::handle_health))
        .route("/api/v1/search", get(handlers::handle_search))
        .route("/api/v1/stats", get(handlers::handle_stats))
        .route("/api/v1/list", get(handlers::handle_list))
        .layer(cors)
        .with_state(state)
}
```

### Handler Behavior

**handle_health:**
- Returns `{"status": "ok", "version": "<crate_version>"}` immediately
- No DB connection check (fast response)

**handle_search:**
- Returns `ApiError::BadRequest` if `q` is empty or missing
- Returns `ApiError::BadRequest` if `mode` is invalid (not fts/vec/hybrid)
- Calls `search()` with parameters from `SearchParams` + defaults
- Generates snippets via `extract_snippet()` (max 5 lines, 200 chars)
- Returns `SearchResponse` with results array

**handle_stats:**
- Calls `db.stats()` → maps `VaultStats` to `StatsResponse`
- Returns `ApiError::Internal` on DB error

**handle_list:**
- Gets all vault names from config via `config.resolved_vaults()`
- Calls `db.list_cached_paths(vault_name)` for each vault
- Merges results into a single `FileItem` array with `vault_name` field
- Returns `ListResponse` with file paths and count

### CORS Configuration

```rust
fn create_cors_layer(server_config: &ServerConfig) -> CorsLayer {
    CorsLayer::new()
        .allow_origin(server_config.cors_origins.iter().map(|o| o.parse().unwrap()))
        .allow_methods([Method::GET, Method::OPTIONS])
        .allow_headers(Any)
}
```

- Default: `["http://localhost"]`
- Configurable via `[server] cors_origins = ["http://localhost:3000"]`

### Server Startup Flow

```
run_serve(args, config)
    ↓
Resolve ServerConfig (CLI args > config file > defaults)
    ↓
NoteDatabase::open(db_path)
    ↓
get_tokenizer() → Arc<JapaneseTokenizer>
    ↓
Arc<AppState> { db, tokenizer }
    ↓
create_router(state, &config)
    ↓
TcpListener::bind("127.0.0.1:7171")
    ↓
axum::serve(listener, app)
    ↓
tokio::select! { server, ctrl_c => graceful_shutdown }
```

## Config Extension

Add `server: ServerConfig` field to existing `ShiotsuchiConfig` struct in `core/src/config.rs`.

```toml
[server]
port = 7171
host = "127.0.0.1"
cors_origins = ["http://localhost"]
```

### ServerConfig struct

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
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

### ShiotsuchiConfig change

```rust
pub struct ShiotsuchiConfig {
    // ... existing fields ...
    pub server: ServerConfig,   // NEW
}
```

The `#[serde(default)]` attribute on `server` ensures backward compatibility — existing config files without `[server]` section will use defaults.

## TDD Test Strategy

### Test Pyramid

```
         /\
        /  \        E2E (1-2 tests)
       /    \       Server startup + HTTP request
      /------\
     /        \     Integration (4-6 tests)
    /          \    Router + multiple handlers
   /------------\
  /              \  Unit (12-15 tests)
 /                \ Handler function tests
/------------------\
```

### Unit Tests (core/src/server/handlers.rs)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    fn setup() -> (Arc<AppState>, TempDir) { /* ... */ }

    // --- handle_health ---
    #[tokio::test]
    async fn test_health_returns_ok() { /* 200 + JSON */ }

    // --- handle_search ---
    #[tokio::test]
    async fn test_search_missing_query_param() { /* 400 */ }

    #[tokio::test]
    async fn test_search_empty_query() { /* 400 */ }

    #[tokio::test]
    async fn test_search_returns_results_format() { /* 200 + correct JSON */ }

    #[tokio::test]
    async fn test_search_with_limit() { /* limit parameter works */ }

    #[tokio::test]
    async fn test_search_invalid_mode() { /* 400 */ }

    // --- handle_stats ---
    #[tokio::test]
    async fn test_stats_returns_expected_fields() { /* 200 + field check */ }

    // --- handle_list ---
    #[tokio::test]
    async fn test_list_returns_empty_when_no_files() { /* 200 + empty */ }

    // --- ApiError ---
    #[tokio::test]
    async fn test_error_response_format() { /* correct JSON structure */ }

    // --- CORS ---
    #[tokio::test]
    async fn test_cors_headers_present() { /* OPTIONS preflight */ }

    #[tokio::test]
    async fn test_cors_rejects_non_localhost() { /* denied */ }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_server_starts_and_responds() {
    // Actual server startup + reqwest request
}

#[tokio::test]
async fn test_port_conflict_returns_error() {
    // Port conflict error message
}
```

### TDD Implementation Order

1. **config.rs** — Add `ServerConfig` to `ShiotsuchiConfig` (needed by handlers for synonyms/alpha)
2. **types.rs** — Define `ApiError`, response types (compiles only)
3. **handlers.rs** — Write tests (RED)
4. **handlers.rs** — Implement handlers (GREEN)
5. **handlers.rs** — Refactor (error handling, snippet generation)
6. **mod.rs** — `create_router()` implementation
7. **cors.rs** — CORS middleware
8. **serve.rs** (CLI) — Server startup command

## Security

- Default bind address: `127.0.0.1` only (no external access)
- CORS: localhost only by default
- No authentication (local use only)
- Read-only DB access (WAL mode)

## Compatibility with MCP

- `shiotsuchi serve` and `shiotsuchi mcp` coexist
- Both read from the same SQLite DB (read-only, WAL mode)
- serve: UI frontend backend; mcp: AI assistant interface

## Estimation

**10-12 story points** (increased from 8 due to test coverage, type definitions, API versioning)
