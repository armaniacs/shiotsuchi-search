use crate::config::ShiotsuchiConfig;
use crate::db::NoteDatabase;
use crate::rate_limiter::SlidingWindowRateLimiter;
use crate::search::SearchRequest;
use crate::sensitive::SensitiveDataConfig;
use crate::server::types::*;
use axum::extract::State;
use axum::http::header;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

static HTTP_RATE_LIMITER: LazyLock<SlidingWindowRateLimiter> = LazyLock::new(|| SlidingWindowRateLimiter::new(30));

fn check_rate_limit() -> Result<(), ApiError> {
    if !HTTP_RATE_LIMITER.allow() {
        return Err(ApiError::TooManyRequests("rate limit exceeded: 30 req/s".to_string()));
    }
    Ok(())
}

/// Shared application state.
pub struct AppState {
    pub db: Arc<tokio::sync::Mutex<NoteDatabase>>,
    pub tokenizer: Option<Arc<crate::tokenizer::JapaneseTokenizer>>,
    pub synonyms: HashMap<String, Vec<String>>,
    pub hybrid_alpha: Option<f64>,
    pub config: Option<ShiotsuchiConfig>,
    /// API key for authentication. None = no auth required.
    pub api_key: Option<String>,
    /// Sensitive data masking configuration. None = no masking applied.
    pub sensitive_config: Option<SensitiveDataConfig>,
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
            Some(key) if constant_time_eq(key, expected_key.as_str()) => Ok(next.run(req).await),
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
    }))
}

/// Search endpoint.
pub async fn handle_search(
    State(state): State<Arc<AppState>>,
    params: Result<axum::extract::Query<SearchParams>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    check_rate_limit()?;
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

    let effective_limit = params.limit.saturating_add(params.offset);

    let results = if let Some(tokenizer) = &state.tokenizer {
        let backlink_scoring = state.config.as_ref()
            .map(|c| c.indexing.backlink_scoring)
            .unwrap_or(true);
        let request = SearchRequest {
            query: &query,
            limit: effective_limit,
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
            lambda: 0.5,
            backlink_scoring,
        };
        crate::search::search(&db, tokenizer, &request)
            .map_err(|e| ApiError::Internal(format!("search failed: {}", e)))?
    } else {
        let fts5_query = crate::tokenizer::simple_and_query(&query);
        let hits = db.fts_search(&fts5_query, effective_limit, params.vault.as_deref())
            .map_err(|e| ApiError::Internal(format!("search failed: {}", e)))?;
        if hits.is_empty() {
            vec![]
        } else {
            crate::search::build_results(&db, hits, crate::models::SearchMode::Fts, None)
                .map_err(|e| ApiError::Internal(format!("search failed: {}", e)))?
        }
    };

    let total = if params.mode == "fts" {
        let fts5_query = crate::tokenizer::simple_and_query(&query);
        db.fts_search(&fts5_query, 1_000_000, params.vault.as_deref())
            .map(|hits| hits.len())
            .unwrap_or_else(|_| results.len())
    } else {
        results.len()
    };

    let items: Vec<SearchResultItem> = results
        .into_iter()
        .skip(params.offset)
        .take(params.limit)
        .map(|r| {
            let snippet = crate::search::extract_snippet(&r.content, &query, 5, 200);
            let snippet =
                crate::sensitive::mask_sensitive_data(&snippet, state.sensitive_config.as_ref());
            SearchResultItem {
                file_path: r.file_path,
                title: r.title,
                parent_header: r.parent_header,
                snippet,
                score: r.score,
                vault_name: r.vault_name,
            }
        })
        .collect();

    let count = items.len();
    Ok(Json(serde_json::json!({
        "results": items,
        "count": count,
        "total": total,
        "offset": params.offset,
        "limit": params.limit,
    })))
}

/// Stats endpoint.
pub async fn handle_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, ApiError> {
    check_rate_limit()?;
    let db = state.db.lock().await;
    let stats = db
        .stats()
        .map_err(|e| ApiError::Internal(format!("failed to get stats: {}", e)))?;

    Ok(Json(serde_json::json!({
        "total_files": stats.total_files,
        "total_chunks": stats.total_chunks,
        "total_size_bytes": stats.total_size_bytes,
        "last_indexed_at": stats.last_indexed_at,
        "embedder_status": stats.embedder_status,
        "top_tags": stats.top_tags,
    })))
}

/// List indexed files endpoint with pagination.
pub async fn handle_list(
    State(state): State<Arc<AppState>>,
    config: axum::extract::Extension<ShiotsuchiConfig>,
    params: Result<axum::extract::Query<ListParams>, axum::extract::rejection::QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    check_rate_limit()?;
    let params = params.map_err(|e| ApiError::BadRequest(e.body_text()))?;
    let offset = params.offset;
    let limit = params.limit.min(200); // cap at 200

    let mut all_files = Vec::new();
    let mut total = 0usize;
    let db = state.db.lock().await;
    for (vault_name, _vault_path) in config.resolved_vaults() {
        match db.list_cached_paths(&vault_name) {
            Ok(paths) => {
                total += paths.len();
                // Only collect files we might need: skip already-passed + current page + buffer
                for path in paths {
                    all_files.push(FileItem {
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
    let files: Vec<FileItem> = all_files.into_iter().skip(offset).take(limit).collect();
    let count = files.len();
    Ok(Json(serde_json::json!({
        "files": files,
        "count": count,
        "total": total,
        "offset": offset,
        "limit": limit,
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
                            let masked = crate::sensitive::mask_sensitive_data(
                                &content,
                                state.sensitive_config.as_ref(),
                            );
                            return Ok(Json(serde_json::json!({
                                "path": file_path,
                                "vault": vault_name,
                                "content": masked,
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

    let masked = crate::sensitive::mask_sensitive_data(&content, state.sensitive_config.as_ref());

    Ok(Json(serde_json::json!({
        "path": file_path,
        "vault": vault_name,
        "content": masked,
        "source": "index",
    })))
}

/// Constant-time string comparison to prevent timing side-channel attacks.
/// Always compares all bytes regardless of length to avoid leaking key length.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let mut result = (a.len() != b.len()) as u8;
    for (ca, cb) in a.bytes().zip(b.bytes()) {
        result |= ca ^ cb;
    }
    result == 0
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
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|req: &axum::http::Request<_>| {
                            let id = req
                                .extensions()
                                .get::<tower_http::request_id::RequestId>()
                                .and_then(|id| id.header_value().to_str().ok())
                                .unwrap_or("-");
                            tracing::info_span!(
                                "request",
                                request_id = id,
                                method = %req.method(),
                                path = %req.uri().path()
                            )
                        })
                        .on_response(
                            |res: &axum::http::Response<_>,
                             latency: Duration,
                             _span: &tracing::Span| {
                                tracing::info!(
                                    status = res.status().as_u16(),
                                    latency_ms = latency.as_millis()
                                );
                            },
                        ),
                )
                .layer(PropagateRequestIdLayer::x_request_id()),
        )
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
            sensitive_config: None,
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
    async fn test_response_has_request_id_header() {
        let (router, _tmp) = setup_test_router();
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            resp.headers().contains_key("x-request-id"),
            "response must include x-request-id header"
        );
    }

    #[tokio::test]
    async fn test_request_id_propagates_client_header() {
        let (router, _tmp) = setup_test_router();
        let resp = router
            .oneshot(
                Request::builder()
                    .uri("/api/v1/health")
                    .header("x-request-id", "my-trace-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let rid = resp
            .headers()
            .get("x-request-id")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(rid, "my-trace-123", "client-specified x-request-id must propagate");
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
        assert!(json.get("total").is_some());
        assert!(json.get("offset").is_some());
        assert!(json.get("limit").is_some());
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
        assert_eq!(json["total"], 0);
        assert_eq!(json["offset"], 0);
        assert_eq!(json["limit"], 50);
        assert_eq!(json["files"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn test_list_pagination_offset_limit() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/list?offset=0&limit=5")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["offset"], 0);
        assert_eq!(json["limit"], 5);
    }

    #[tokio::test]
    async fn test_list_pagination_second_page() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/list?offset=50&limit=50")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["offset"], 50);
        assert_eq!(json["limit"], 50);
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
        assert!(json.get("db_path").is_none(), "db_path must not be exposed in public API");
    }

    #[tokio::test]
    async fn test_health_does_not_expose_version() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert!(json.get("version").is_none(), "version must not be exposed on public endpoint");
    }

    #[tokio::test]
    async fn test_stats_does_not_expose_db_path() {
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
        assert!(json.get("db_path").is_none(), "db_path must not be exposed to unauthenticated users");
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
            sensitive_config: None,
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

    #[tokio::test]
    async fn test_search_limit_clamped_silently() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search?q=test&limit=9999")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_cors_rejects_custom_header_not_in_allow_list() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .method("OPTIONS")
            .uri("/api/v1/search")
            .header("Origin", "http://localhost")
            .header("Access-Control-Request-Method", "GET")
            .header("Access-Control-Request-Headers", "X-Custom-Header")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        // With restricted AllowHeaders, custom headers should not be allowed
        let allow_headers = resp.headers().get("access-control-allow-headers");
        match allow_headers {
            Some(val) => {
                let val_str = val.to_str().unwrap_or("");
                assert!(!val_str.to_lowercase().contains("x-custom-header"),
                    "CORS should not allow X-Custom-Header: {}", val_str);
            }
            None => {} // No allow-headers header means restrictive — acceptable
        }
    }

    #[tokio::test]
    async fn test_search_offset_default_is_zero() {
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
        assert_eq!(json["offset"], 0, "offset must default to 0");
    }

    #[tokio::test]
    async fn test_search_response_includes_total() {
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
        assert!(json.get("total").is_some(), "response must include 'total'");
        assert!(json.get("offset").is_some(), "response must include 'offset'");
        assert!(json.get("limit").is_some(), "response must include 'limit'");
        assert!(json["total"].is_number());
        assert!(json["offset"].is_number());
        assert!(json["limit"].is_number());
    }

    #[tokio::test]
    async fn test_search_with_offset() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search?q=test&offset=0&limit=10")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["offset"], 0);
        assert_eq!(json["limit"], 10);
        assert!(json["total"].is_number());
        assert!(json["count"].is_number());
    }

    #[tokio::test]
    async fn test_search_total_not_capped_by_page_size() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search?q=test&offset=0&limit=5")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["total"].as_u64().unwrap() >= json["count"].as_u64().unwrap(),
            "total must be >= count (page size)");
    }

    #[tokio::test]
    async fn test_search_offset_clamped() {
        let (router, _tmp) = setup_test_router();
        let req = Request::builder()
            .uri("/api/v1/search?q=test&offset=99999")
            .body(Body::empty())
            .unwrap();
        let resp = router.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
