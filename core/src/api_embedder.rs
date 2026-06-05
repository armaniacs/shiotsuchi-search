use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::embedder::EmbedderError;

/// Default timeout for HTTP requests to embedding API (60 seconds).
/// Some providers may have slower inference; can be overridden via config.
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Default batch size cap for embedding requests (100 texts per request).
/// Prevents hitting API payload limits and improves reliability.
const DEFAULT_BATCH_CAP: usize = 100;

#[derive(Debug, Serialize)]
struct EmbeddingRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

/// Internal HTTP client for OpenAI-compatible embedding APIs.
#[derive(Debug, Clone)]
pub(crate) struct ApiClient {
    endpoint: String,
    model: String,
    api_key: String,
    timeout: Duration,
    batch_cap: usize,
    usage_tracker: Option<crate::usage_tracker::UsageTracker>,
}

impl ApiClient {
    pub(crate) fn new(
        endpoint: String,
        model: String,
        api_key: String,
        usage_tracker: Option<crate::usage_tracker::UsageTracker>,
    ) -> Self {
        Self {
            endpoint,
            model,
            api_key,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            batch_cap: DEFAULT_BATCH_CAP,
            usage_tracker,
        }
    }

    /// Returns a stable identifier for the model configuration.
    /// Intentionally excludes api_key to ensure caching works correctly
    /// regardless of how the key was provided (env var vs config).
    pub(crate) fn model_id(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.endpoint.as_bytes());
        hasher.update(self.model.as_bytes());
        format!("api:{}", hex::encode(hasher.finalize()))
    }

    pub(crate) fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());

        for chunk in texts.chunks(self.batch_cap) {
            if let Some(tracker) = &self.usage_tracker {
                tracker.check_and_increment()?;
            }
            let request_body = EmbeddingRequest {
                model: &self.model,
                input: chunk.to_vec(),
            };

            let body_json = serde_json::to_string(&request_body)
                .map_err(|e| EmbedderError::Inference(format!("JSON serialize error: {}", e)))?;

            // Send request with retry on transient failures
            let body_str = self.send_with_retry(&body_json)?;

            let parsed: EmbeddingResponse = serde_json::from_str(&body_str)
                .map_err(|e| EmbedderError::Inference(format!("invalid API response: {}", e)))?;

            if parsed.data.len() != chunk.len() {
                return Err(EmbedderError::Inference(
                    format!("API returned {} embeddings for {} inputs", parsed.data.len(), chunk.len())
                ));
            }

            for d in parsed.data {
                all_embeddings.push(d.embedding);
            }
        }

        Ok(all_embeddings)
    }

    /// Sends HTTP POST request with exponential backoff retry on transient failures.
    fn send_with_retry(&self, body_json: &str) -> Result<String, EmbedderError> {
        let mut delay_ms = 100u64;
        let mut last_error = String::new();

        for attempt in 0..4 {
            match ureq::post(&self.endpoint)
                .header("Authorization", &format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .config()
                .timeout_global(Some(self.timeout))
                .build()
                .send(body_json)
            {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if status >= 500 && attempt < 3 {
                        // Transient server error - retry with backoff
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                        delay_ms *= 2;
                        continue;
                    }

                    if status >= 300 {
                        let body = resp.into_body().read_to_string().unwrap_or_default();
                        return Err(EmbedderError::Inference(
                            format!("API error: {} — {}", status, self.sanitize_error_body(&body))
                        ));
                    }

                    return resp.into_body().read_to_string()
                        .map_err(|e| EmbedderError::Inference(format!("API response read error: {}", e)));
                }
                Err(e) => {
                    let err_str = e.to_string();
                    // Connection/timeout errors are worth retrying
                    if attempt < 3 && (err_str.contains("connection") || err_str.contains("timeout")) {
                        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                        delay_ms *= 2;
                        last_error = err_str;
                        continue;
                    }
                    return Err(EmbedderError::Inference(format!("API request failed: {}", e)));
                }
            }
        }

        Err(EmbedderError::Inference(format!("API request failed after retries: {}", last_error)))
    }

    /// Sanitizes error body to prevent leaking sensitive information.
    fn sanitize_error_body(&self, body: &str) -> String {
        // Truncate very long error bodies and remove potential API keys
        let truncated: String = body.chars().take(200).collect();
        // Remove potential API key patterns
        truncated
            .replace(&format!("Bearer {}", self.api_key), "Bearer [REDACTED]")
            .replace(&self.api_key, "[REDACTED]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_model_id_stable() {
        // model_id intentionally excludes api_key - different keys should not affect caching
        let c1 = ApiClient::new(
            "https://api.ai.sakura.ad.jp/v1/embeddings".to_string(),
            "multilingual-e5-large".to_string(),
            "key1".to_string(),
            None,
        );
        let c2 = ApiClient::new(
            "https://api.ai.sakura.ad.jp/v1/embeddings".to_string(),
            "multilingual-e5-large".to_string(),
            "key2".to_string(),
            None,
        );
        assert_eq!(c1.model_id(), c2.model_id(), "model_id should be stable regardless of API key");
    }

    #[test]
    fn test_api_client_model_id_differs_by_endpoint() {
        let c1 = ApiClient::new(
            "https://a.example.com/v1/embeddings".to_string(),
            "model".to_string(),
            "key".to_string(),
            None,
        );
        let c2 = ApiClient::new(
            "https://b.example.com/v1/embeddings".to_string(),
            "model".to_string(),
            "key".to_string(),
            None,
        );
        assert_ne!(c1.model_id(), c2.model_id());
    }

    #[test]
    fn test_embed_batch_empty_returns_empty() {
        let client = ApiClient::new(
            "https://example.com".to_string(),
            "model".to_string(),
            "key".to_string(),
            None,
        );
        let result = client.embed_batch(&[]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_openai_response() {
        let json = r#"{"data":[{"embedding":[0.1,0.2,0.3]},{"embedding":[0.4,0.5,0.6]}]}"#;
        let resp: EmbeddingResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].embedding, vec![0.1_f32, 0.2_f32, 0.3_f32]);
    }

    #[test]
    fn test_sanitize_error_body_redacts_api_key() {
        let client = ApiClient::new(
            "https://example.com".to_string(),
            "model".to_string(),
            "sk-secret-key-12345".to_string(),
            None,
        );
        let body = r#"{"error": "Invalid API key", "key": "sk-secret-key-12345"}"#;
        let sanitized = client.sanitize_error_body(body);
        assert!(!sanitized.contains("sk-secret-key-12345"), "API key should be redacted");
        assert!(sanitized.contains("[REDACTED]"), "Should contain redaction marker");
    }

    #[test]
    fn test_sanitize_error_body_truncates_long_body() {
        let client = ApiClient::new(
            "https://example.com".to_string(),
            "model".to_string(),
            "key".to_string(),
            None,
        );
        let long_body = "x".repeat(500);
        let sanitized = client.sanitize_error_body(&long_body);
        assert!(sanitized.len() <= 200, "Error body should be truncated to 200 chars");
    }

    #[test]
    fn test_api_client_with_usage_tracker() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tracker = crate::usage_tracker::UsageTracker::new(tmp.path(), true, Some(1));
        let client = ApiClient::new(
            "https://example.com".to_string(),
            "model".to_string(),
            "key".to_string(),
            Some(tracker),
        );
        let _ = client.embed_batch(&["test"]);
        let result = client.embed_batch(&["test2"]);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("limit") || err_msg.contains("上限"));
    }
}