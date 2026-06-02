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
    pub tokenizer: Arc<crate::tokenizer::JapaneseTokenizer>,
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
}
