use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::embedder::EmbedderError;

const DEFAULT_TIMEOUT_SECS: u64 = 60;
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
}

impl ApiClient {
    pub(crate) fn new(
        endpoint: String,
        model: String,
        api_key: String,
    ) -> Self {
        Self {
            endpoint,
            model,
            api_key,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            batch_cap: DEFAULT_BATCH_CAP,
        }
    }

    pub(crate) fn model_id(&self) -> String {
        // Stable identifier: hash of endpoint + model
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
            let request_body = EmbeddingRequest {
                model: &self.model,
                input: chunk.to_vec(),
            };

            let body_json = serde_json::to_string(&request_body)
                .map_err(|e| EmbedderError::Inference(format!("JSON serialize error: {}", e)))?;

            let response = ureq::post(&self.endpoint)
                .header("Authorization", &format!("Bearer {}", self.api_key))
                .header("Content-Type", "application/json")
                .config()
                .timeout_global(Some(self.timeout))
                .build()
                .send(&body_json)
                .map_err(|e| EmbedderError::Inference(format!("API request failed: {}", e)))?;

            if response.status().as_u16() >= 300 {
                let status = response.status();
                let body = response.into_body()
                    .read_to_string()
                    .unwrap_or_default();
                return Err(EmbedderError::Inference(
                    format!("API error: {} — {}", status, body)
                ));
            }

            let body_str = response.into_body()
                .read_to_string()
                .map_err(|e| EmbedderError::Inference(format!("API response read error: {}", e)))?;

            let parsed: EmbeddingResponse = serde_json::from_str(&body_str)
                .map_err(|e| EmbedderError::Inference(format!("invalid API response: {} — body: {}", e, body_str)))?;

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_model_id_stable() {
        let c1 = ApiClient::new(
            "https://api.ai.sakura.ad.jp/v1/embeddings".to_string(),
            "multilingual-e5-large".to_string(),
            "key".to_string(),
        );
        let c2 = ApiClient::new(
            "https://api.ai.sakura.ad.jp/v1/embeddings".to_string(),
            "multilingual-e5-large".to_string(),
            "different-key".to_string(),
        );
        // Different API keys should not affect model_id
        assert_eq!(c1.model_id(), c2.model_id());
    }

    #[test]
    fn test_api_client_model_id_differs_by_endpoint() {
        let c1 = ApiClient::new(
            "https://a.example.com/v1/embeddings".to_string(),
            "model".to_string(),
            "key".to_string(),
        );
        let c2 = ApiClient::new(
            "https://b.example.com/v1/embeddings".to_string(),
            "model".to_string(),
            "key".to_string(),
        );
        assert_ne!(c1.model_id(), c2.model_id());
    }

    #[test]
    fn test_embed_batch_empty_returns_empty() {
        let client = ApiClient::new(
            "https://example.com".to_string(),
            "model".to_string(),
            "key".to_string(),
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
}
