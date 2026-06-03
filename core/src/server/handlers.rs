use crate::config::ShiotsuchiConfig;
use crate::db::NoteDatabase;
use crate::search::SearchRequest;
use crate::server::types::*;
use axum::extract::State;
use axum::http::header;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use std::collections::HashMap;
use std::sync::Arc;

/// Shared application state.
pub struct AppState {
    pub db: Arc<tokio::sync::Mutex<NoteDatabase>>,
    pub tokenizer: Option<Arc<crate::tokenizer::JapaneseTokenizer>>,
    pub synonyms: HashMap<String, Vec<String>>,
    pub hybrid_alpha: Option<f64>,
    pub config: Option<ShiotsuchiConfig>,
    /// API key for authentication. None = no auth required.
    pub api_key: Option<String>,
}

/// Authentication middleware — checks X-API-Key header against AppState.api_key.
/// Skips auth when api_key is None (localhost or no key configured).
pub async fn auth_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let state = req
        .extensions()
        .get::<Arc<AppState>>()
        .cloned()
        .ok_or_else(|| ApiError::Internal("AppState not found".to_string()))?;

    if let Some(ref expected_key) = state.api_key {
        let provided_key = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .or_else(|| {
                req.headers()
                    .get("X-API-Key")
                    .and_then(|v| v.to_str().ok())
            });

        match provided_key {
            Some(key) if key == expected_key.as_str() => Ok(next.run(req).await),
            _ => Err(ApiError::Unauthorized(
                "Authentication required. Provide a valid API key via X-API-Key header.".to_string(),
            )),
        }
    } else {
        // No API key configured — skip authentication
        Ok(next.run(req).await)
    }
}

/// Health check endpoint.
pub async fn handle_health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// Search endpoint.
pub async fn handle_search(
    State(state): State<Arc<AppState>>,
    params: Result<axum::extract::Query<SearchParams>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let axum::extract::Query(params) =
        params.map_err(|e| ApiError::BadRequest(e.body_text()))?;

    let query = params.q.trim().to_string();
    if query.is_empty() {
        return Err(ApiError::BadRequest(
            "query parameter 'q' is required".to_string(),
        ));
    }

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

    let db = state.db.lock().await;

    let results = if let Some(tokenizer) = &state.tokenizer {
        let request = SearchRequest {
            query: &query,
            limit: params.limit,
            mode,
            embedder: None,
            min_score: None,
            vault_filter: params.vault.as_deref(),
            tag_filter: params.tag.as_deref(),
            since_date: params.since.as_deref(),
            user_dictionary: &[],
            synonyms: &state.synonyms,
            fuzzy: false,
            hybrid_alpha: state.hybrid_alpha,
            mmr: false,
            lambda: 0.7,
            backlink_scoring: false,
        };
        crate::search::search(&db, tokenizer, &request)
            .map_err(|e| ApiError::Internal(format!("search failed: {}", e)))?
    } else {
        let fts5_query = crate::tokenizer::simple_and_query(&query);
        let hits = db.fts_search(&fts5_query, params.limit, params.vault.as_deref())
            .map_err(|e| ApiError::Internal(format!("search failed: {}", e)))?;
        if hits.is_empty() {
            vec![]
        } else {
            crate::search::build_results(&db, hits, crate::models::SearchMode::Fts, None)
                .map_err(|e| ApiError::Internal(format!("search failed: {}", e)))?
        }
    };

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

/// Stats endpoint.
pub async fn handle_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let db = state.db.lock().await;
    let stats = db
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

/// List indexed files endpoint.
pub async fn handle_list(
    State(state): State<Arc<AppState>>,
    config: axum::extract::Extension<ShiotsuchiConfig>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut files = Vec::new();
    let db = state.db.lock().await;
    for (vault_name, _vault_path) in config.resolved_vaults() {
        match db.list_cached_paths(&vault_name) {
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

/// Serve the browser UI.
pub async fn serve_ui() -> axum::response::Html<&'static str> {
    axum::response::Html(include_str!("ui.html"))
}

/// Read a file's content — from disk if available, falling back to DB chunks.
pub async fn handle_read(
    State(state): State<Arc<AppState>>,
    params: Result<axum::extract::Query<ReadParams>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let params = params.map_err(|e| ApiError::BadRequest(e.body_text()))?;

    let vault_name = params.vault.as_deref().unwrap_or("default");
    let file_path = &params.path;

    // Try reading from disk first
    if let Some(config) = &state.config {
        if let Some((_, vault_path)) = config.resolved_vaults().into_iter().find(|(name, _)| name == vault_name) {
            let full_path = vault_path.join(file_path);
            if let Ok(canonical) = full_path.canonicalize() {
                if let Ok(canonical_vault) = vault_path.canonicalize() {
                    if canonical.starts_with(&canonical_vault) {
                        if let Ok(content) = tokio::fs::read_to_string(&canonical).await {
                            return Ok(Json(serde_json::json!({
                                "path": file_path,
                                "vault": vault_name,
                                "content": content,
                                "source": "disk",
                            })));
                        }
                    }
                }
            }
        }
    }

    // Fallback: read from DB chunks
    let db = state.db.lock().await;
    let chunks = db.get_chunks_for_file(vault_name, file_path)
        .map_err(|e| ApiError::Internal(format!("database error: {}", e)))?;

    if chunks.is_empty() {
        return Err(ApiError::NotFound(format!(
            "file '{}' not found in vault '{}'", file_path, vault_name
        )));
    }

    // Reassemble content from chunks in order
    let content: String = chunks.iter()
        .map(|c| c.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(Json(serde_json::json!({
        "path": file_path,
        "vault": vault_name,
        "content": content,
        "source": "index",
    })))
}

/// Create the axum router with all routes.
pub fn create_router(state: Arc<AppState>, config: &ShiotsuchiConfig) -> Router {
    use crate::server::cors::create_cors_layer;

    let cors = create_cors_layer(&config.server);

    // Protected routes (require X-API-Key when api_key is set)
    let protected = Router::new()
        .route("/api/v1/search", get(handle_search))
        .route("/api/v1/stats", get(handle_stats))
        .route("/api/v1/list", get(handle_list))
        .route("/api/v1/read", get(handle_read))
        .layer(axum::middleware::from_fn(auth_middleware));

    // Public routes (no authentication)
    let public = Router::new()
        .route("/ui", get(serve_ui))
        .route("/api/v1/health", get(handle_health));

    Router::new()
        .merge(protected)
        .merge(public)
        .layer(cors)
        .layer(axum::extract::Extension(state.clone()))
        .layer(axum::extract::Extension(config.clone()))
        .with_state(state)
}

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
        let tokenizer = crate::tokenizer::get_tokenizer().ok();
        let state = Arc::new(AppState {
            db: Arc::new(tokio::sync::Mutex::new(db)),
            tokenizer,
            synonyms: HashMap::new(),
            hybrid_alpha: None,
            config: Some(ShiotsuchiConfig::default()),
            api_key: None,
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

    #[tokio::test]
    async fn test_ui_returns_html() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/ui")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let headers = resp.headers().clone();
        let content_type = headers.get("content-type").unwrap().to_str().unwrap();
        assert!(content_type.contains("text/html"), "Expected HTML content type, got: {}", content_type);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let html = String::from_utf8_lossy(&body);
        assert!(html.contains("shiotsuchi search"), "HTML should contain title");
        assert!(html.contains("<input"), "HTML should contain search input");
    }

    // --- Authentication Tests ---

    /// Build a test router with API key authentication enabled.
    fn setup_test_router_with_auth(api_key: &str) -> (Router, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = NoteDatabase::open(&db_path).unwrap();
        let tokenizer = crate::tokenizer::get_tokenizer().ok();
        let state = Arc::new(AppState {
            db: Arc::new(tokio::sync::Mutex::new(db)),
            tokenizer,
            synonyms: HashMap::new(),
            hybrid_alpha: None,
            config: Some(ShiotsuchiConfig::default()),
            api_key: Some(api_key.to_string()),
        });
        let router = create_router(state, &ShiotsuchiConfig::default());
        (router, tmp)
    }

    #[tokio::test]
    async fn test_auth_valid_key_returns_200() {
        let (router, _tmp) = setup_test_router_with_auth("test-key-123");
        let req = Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap();
        // Health endpoint is public — no auth needed
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Protected endpoint with valid key
        let (router2, _tmp2) = setup_test_router_with_auth("test-key-123");
        let req = Request::builder()
            .uri("/api/v1/stats")
            .header("X-API-Key", "test-key-123")
            .body(Body::empty())
            .unwrap();
        let resp = router2.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_no_key_returns_401() {
        let (router, _tmp) = setup_test_router_with_auth("test-key-123");
        let req = Request::builder()
            .uri("/api/v1/stats")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_wrong_key_returns_401() {
        let (router, _tmp) = setup_test_router_with_auth("test-key-123");
        let req = Request::builder()
            .uri("/api/v1/stats")
            .header("X-API-Key", "wrong-key")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_localhost_skips_auth() {
        // Default setup has api_key: None — auth is skipped
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/stats")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_error_response_format() {
        let (router, _tmp) = setup_test_router_with_auth("test-key-123");
        let req = Request::builder()
            .uri("/api/v1/stats")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"]["code"], "UNAUTHORIZED");
        assert!(json["error"]["message"].is_string());
    }

    #[tokio::test]
    async fn test_auth_bearer_header_works() {
        let (router, _tmp) = setup_test_router_with_auth("test-key-123");
        let req = Request::builder()
            .uri("/api/v1/stats")
            .header("Authorization", "Bearer test-key-123")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
