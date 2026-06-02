use axum::http::{HeaderValue, Method};
use tower_http::cors::{AllowHeaders, CorsLayer};

use crate::config::ServerConfig;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cors_allows_localhost() {
        let config = ServerConfig::default();
        let _layer = create_cors_layer(&config);
    }

    #[test]
    fn test_cors_custom_origins() {
        let config = ServerConfig {
            cors_origins: vec!["http://localhost:3000".to_string()],
            ..Default::default()
        };
        let _layer = create_cors_layer(&config);
    }
}
