use crate::config::ShiotsuchiConfig;
use crate::db::NoteDatabase;
use crate::server::types::*;
use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Shared application state.
pub struct AppState {
    pub db: Arc<Mutex<NoteDatabase>>,
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
