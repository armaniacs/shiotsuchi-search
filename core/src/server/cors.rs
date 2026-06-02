use crate::config::ServerConfig;
use tower_http::cors::CorsLayer;

/// Create a CORS layer from server configuration.
pub fn create_cors_layer(_server_config: &ServerConfig) -> CorsLayer {
    CorsLayer::permissive()
}
