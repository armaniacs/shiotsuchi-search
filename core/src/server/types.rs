use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;

/// Structured API error type.
#[derive(Debug)]
pub enum ApiError {
    /// 400 — invalid request parameters
    BadRequest(String),
    /// 401 — authentication required or invalid
    Unauthorized(String),
    /// 404 — resource not found
    NotFound(String),
    /// 500 — internal server error
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", msg),
            ApiError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", msg),
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

// --- Query Parameters ---

#[derive(Deserialize)]
pub struct ReadParams {
    /// File path relative to vault (required)
    pub path: String,
    /// Vault name (default: "default")
    pub vault: Option<String>,
}

#[derive(Deserialize)]
pub struct SearchParams {
    /// Search query (required)
    pub q: String,
    /// Maximum results to return (default: 20, max: 200)
    #[serde(default = "default_limit", deserialize_with = "deserialize_clamped_limit")]
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

fn deserialize_clamped_limit<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: Deserializer<'de>,
{
    let val = usize::deserialize(deserializer)?;
    Ok(val.min(200))
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
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Deserialize)]
pub struct ListParams {
    #[serde(default)]
    pub offset: usize,
    #[serde(default = "default_list_limit")]
    pub limit: usize,
}

fn default_list_limit() -> usize {
    50
}

#[derive(Serialize)]
pub struct FileItem {
    pub path: String,
    pub vault_name: String,
}
