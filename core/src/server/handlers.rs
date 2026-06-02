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
    pub db: Arc<tokio::sync::Mutex<NoteDatabase>>,
    pub tokenizer: Option<Arc<crate::tokenizer::JapaneseTokenizer>>,
    pub synonyms: HashMap<String, Vec<String>>,
    pub hybrid_alpha: Option<f64>,
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
    axum::extract::Query(params): axum::extract::Query<SearchParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
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
        crate::search::search(
            &db,
            tokenizer,
            &query,
            params.limit,
            mode,
            None,
            None,
            params.vault.as_deref(),
            params.tag.as_deref(),
            params.since.as_deref(),
            &[],
            &state.synonyms,
            false,
            state.hybrid_alpha,
            false,
            0.7,
            false,
        )
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
}
