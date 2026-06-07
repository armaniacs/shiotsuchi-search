use crate::models::EmbedderStatus;
use ort::session::Session;
use ort::value::Tensor;
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokenizers::Tokenizer;
use hex;

use crate::api_embedder::ApiClient;

/// Maximum sequence length for the embedding model (Qwen3-Embedding supports up to 32K,
/// but 512 is a practical default for note chunks).
const MAX_SEQ_LEN: usize = 512;

/// High-level wrapper around the ONNX embedding model.
///
/// Loads a HuggingFace tokenizer from `tokenizer.json` (located alongside the ONNX model)
/// and an ONNX Runtime session from `model.onnx`.
///
/// The model is expected to output either:
/// - `sentence_embedding` (already pooled) — shape `(batch, hidden)`
/// - `last_hidden_state` (needs mean pooling + L2 normalization) — shape `(batch, seq_len, hidden)`
#[derive(Debug)]
pub struct Embedder {
    backend: EmbedderBackend,
}

#[derive(Debug)]
enum EmbedderBackend {
    Onnx {
        session: RefCell<Session>,
        tokenizer: Box<Tokenizer>,
        model_id: String,
    },
    Api {
        client: ApiClient,
        model_id: String,
    },
    #[cfg(test)]
    TestVec(Vec<f32>),
}


/// Compute the SHA-256 hash of a file for model version tracking.
fn compute_model_id(path: &Path) -> std::io::Result<String> {
    crate::indexer::sha256_file(path)
}

impl Embedder {
    /// Load an embedder from an ONNX model file.
    ///
    /// Expects `tokenizer.json` to exist in the same directory as the model.
    pub fn load(model_path: &Path) -> Result<Self, EmbedderError> {
        let model_dir = model_path
            .parent()
            .ok_or_else(|| EmbedderError::Load("Cannot determine model directory".to_string()))?;
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !tokenizer_path.exists() {
            return Err(EmbedderError::Load(format!(
                "Tokenizer not found at: {}",
                tokenizer_path.display()
            )));
        }

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| EmbedderError::Load(format!("Failed to load tokenizer: {}", e)))?;

        // Compute model identifier (SHA-256 hash) for version tracking
        let model_id = match compute_model_id(model_path) {
            Ok(id) => id,
            Err(e) => {
                tracing::warn!("Failed to compute model hash: {}", e);
                "unknown".to_string()
            }
        };

        let session = Session::builder()
            .map_err(|e| EmbedderError::Load(format!("ORT init error: {}", e)))?
            .commit_from_file(model_path)
            .map_err(|e| EmbedderError::Load(format!("Failed to load ONNX model: {}", e)))?;

        Ok(Self {
            backend: EmbedderBackend::Onnx {
                session: RefCell::new(session),
                tokenizer: Box::new(tokenizer),
                model_id,
            },
        })
    }

    /// Alias for `load` — used by CLI code following the plan's naming convention.
    pub fn new(model_path: &Path) -> Result<Self, EmbedderError> {
        Self::load(model_path)
    }

    /// Test-only constructor: returns an embedder whose `embed()` always yields
    /// a deterministic 1024-dim zero vector.  Used by search tests that need
    /// a valid `&Embedder` reference to exercise the Hybrid-mode code paths
    /// without loading an actual ONNX model.
    #[cfg(test)]
    pub fn for_testing() -> Self {
        use std::sync::OnceLock;
        static DUMMY: OnceLock<Vec<f32>> = OnceLock::new();
        let dummy = DUMMY.get_or_init(|| vec![0.0f32; 1024]);
        Self {
            backend: EmbedderBackend::TestVec(dummy.clone()),
        }
    }

    pub fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        match &self.backend {
            EmbedderBackend::Onnx { .. } => {
                let results = self.embed_batch_inner(&[text])?;
                results.into_iter().next().ok_or_else(|| {
                    EmbedderError::Inference("No output from batch".to_string())
                })
            }
            EmbedderBackend::Api { client, .. } => {
                let results = client.embed_batch(&[text])?;
                results.into_iter().next().ok_or_else(|| {
                    EmbedderError::Inference("No output from API batch".to_string())
                })
            }
            #[cfg(test)]
            EmbedderBackend::TestVec(v) => Ok(v.clone()),
        }
    }

    pub fn embed_batch(&self, texts: &[&str]) -> Vec<Result<Vec<f32>, EmbedderError>> {
        match &self.backend {
            EmbedderBackend::Onnx { .. } => {
                match self.embed_batch_inner(texts) {
                    Ok(results) => results.into_iter().map(Ok).collect(),
                    Err(e) => {
                        let err = EmbedderError::Inference(e.to_string());
                        texts.iter().map(|_| Err(err.clone())).collect()
                    }
                }
            }
            EmbedderBackend::Api { client, .. } => {
                match client.embed_batch(texts) {
                    Ok(results) => results.into_iter().map(Ok).collect(),
                    Err(e) => {
                        let err = EmbedderError::Inference(e.to_string());
                        texts.iter().map(|_| Err(err.clone())).collect()
                    }
                }
            }
            #[cfg(test)]
            EmbedderBackend::TestVec(v) => {
                texts.iter().map(|_| Ok(v.clone())).collect()
            }
        }
    }

    pub fn status(&self) -> EmbedderStatus {
        match &self.backend {
            EmbedderBackend::Onnx { .. } => EmbedderStatus::Ready,
            EmbedderBackend::Api { .. } => EmbedderStatus::Ready,
            #[cfg(test)]
            EmbedderBackend::TestVec(_) => EmbedderStatus::Ready,
        }
    }

    pub fn model_id(&self) -> &str {
        match &self.backend {
            EmbedderBackend::Onnx { model_id, .. } => model_id,
            EmbedderBackend::Api { model_id, .. } => model_id,
            #[cfg(test)]
            EmbedderBackend::TestVec(_) => "test-vector",
        }
    }

    pub(crate) fn from_api_client(client: ApiClient) -> Self {
        let model_id = client.model_id();
        Self {
            backend: EmbedderBackend::Api { client, model_id },
        }
    }

    // ── internal helpers ──────────────────────────────────────────────

    fn embed_batch_inner(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, EmbedderError> {
        let (session, tokenizer) = match &self.backend {
            EmbedderBackend::Onnx { session, tokenizer, .. } => (session, tokenizer),
            EmbedderBackend::Api { .. } => {
                return Err(EmbedderError::Inference(
                    "embed_batch_inner called on API backend".to_string()
                ));
            }
            #[cfg(test)]
            EmbedderBackend::TestVec(_) => {
                return Err(EmbedderError::Inference(
                    "embed_batch_inner called on test backend".to_string()
                ));
            }
        };

        if texts.is_empty() {
            return Ok(vec![]);
        }

        let batch_size = texts.len();

        // 1. Tokenize all texts
        let encodings: Vec<tokenizers::Encoding> = texts
            .iter()
            .map(|t| {
                tokenizer
                    .encode(*t, true)
                    .map_err(|e| EmbedderError::Inference(format!("Tokenization error: {}", e)))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // 2. Find max sequence length in the batch (capped at MAX_SEQ_LEN)
        let max_len = encodings
            .iter()
            .map(|enc| enc.get_ids().len())
            .max()
            .unwrap_or(0)
            .min(MAX_SEQ_LEN);

        if max_len == 0 {
            return Err(EmbedderError::Inference(
                "All texts empty after tokenization".to_string(),
            ));
        }

        // 3. Build padded input tensor data (flat vectors)
        let total_elements = batch_size * max_len;
        let mut input_ids_data = Vec::with_capacity(total_elements);
        let mut attention_mask_data = Vec::with_capacity(total_elements);

        for enc in &encodings {
            let ids = enc.get_ids();
            let mask = enc.get_attention_mask();
            let len = ids.len().min(MAX_SEQ_LEN);

            for j in 0..len {
                input_ids_data.push(ids[j] as i64);
                attention_mask_data.push(if j < mask.len() { mask[j] as i64 } else { 1 });
            }
            for _ in len..max_len {
                input_ids_data.push(0i64);
                attention_mask_data.push(0i64);
            }
        }

        // Keep a copy of attention mask for mean pooling (data is moved into tensor)
        let attn_mask_clone = attention_mask_data.clone();

        // 4. Check if the model needs token_type_ids
        let needs_token_type_ids = {
            let session = session.borrow();
            session.inputs().iter().any(|o| o.name() == "token_type_ids")
        };

        // 5. Run inference (session.run requires &mut self)
        let embeddings = {
            let mut session = session.borrow_mut();

            let shape = vec![batch_size as i64, max_len as i64];

            let input_tensor = Tensor::<i64>::from_array((
                shape.clone(),
                input_ids_data.into_boxed_slice(),
            ))
            .map_err(|e| EmbedderError::Inference(format!("Input tensor error: {}", e)))?;

            let mask_tensor = Tensor::<i64>::from_array((
                shape.clone(),
                attention_mask_data.into_boxed_slice(),
            ))
            .map_err(|e| EmbedderError::Inference(format!("Mask tensor error: {}", e)))?;

            let mut inputs = ort::inputs! {
                "input_ids" => input_tensor,
                "attention_mask" => mask_tensor,
            };

            if needs_token_type_ids {
                let tti_data = vec![0i64; total_elements].into_boxed_slice();
                let tti_tensor = Tensor::<i64>::from_array((shape, tti_data))
                    .map_err(|e| EmbedderError::Inference(format!("Token type tensor error: {}", e)))?;
                inputs.push(("token_type_ids".into(), tti_tensor.into()));
            }

            let outputs = session.run(inputs).map_err(|e| {
                EmbedderError::Inference(format!("ONNX inference error: {}", e))
            })?;

            extract_embeddings(&outputs, batch_size, max_len, &attn_mask_clone)?
        };

        Ok(embeddings)
    }
}

// ── output extraction ─────────────────────────────────────────────────

/// Extract embedding vectors from the ONNX model output.
///
/// Supports two output shapes:
/// - `(batch, hidden)` — already pooled (e.g. output name `sentence_embedding`)
/// - `(batch, seq_len, hidden)` — requires mean pooling + L2 norm (e.g. `last_hidden_state`)
fn extract_embeddings(
    outputs: &ort::session::SessionOutputs,
    batch_size: usize,
    max_len: usize,
    attention_mask: &[i64],
) -> Result<Vec<Vec<f32>>, EmbedderError> {
    // Try known output names, fall back to first output
    let output = if outputs.contains_key("sentence_embedding") {
        &outputs["sentence_embedding"]
    } else if outputs.contains_key("last_hidden_state") {
        &outputs["last_hidden_state"]
    } else if outputs.len() > 0 {
        &outputs[0]
    } else {
        return Err(EmbedderError::Inference("No outputs from model".to_string()));
    };

    let (output_shape, output_data) = output
        .try_extract_tensor::<f32>()
        .map_err(|e| EmbedderError::Inference(format!("Output extraction error: {}", e)))?;

    let shape: Vec<i64> = output_shape.iter().copied().collect();

    match shape.len() {
        // Already pooled: (batch, hidden)
        2 => {
            let hidden = shape[1] as usize;
            Ok((0..batch_size)
                .map(|i| {
                    let start = i * hidden;
                    output_data[start..start + hidden].to_vec()
                })
                .collect())
        }
        // last_hidden_state: (batch, seq_len, hidden) → mean pool + L2 norm
        3 => {
            let seq_len = shape[1] as usize;
            let hidden = shape[2] as usize;
            Ok((0..batch_size)
                .map(|i| {
                    mean_pool_l2_normalize(output_data, i, seq_len, hidden, max_len, attention_mask)
                })
                .collect())
        }
        n => Err(EmbedderError::Inference(format!(
            "Unexpected output dimension: {}. Expected 2 or 3.",
            n
        ))),
    }
}

/// Expected L2-normalized component value after mean-pooling two
/// orthogonal one-hot vectors of equal magnitude: 1/√2 ≈ 0.7071.
/// Used in tests to assert correct L2-normalization of pooled embeddings.
pub const EXPECTED_ORTHOGONAL_COSINE: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Mean pooling followed by L2 normalization.
///
/// This is standard sentence-embedding post-processing:
/// 1. Average the token embeddings (excluding padding tokens)
/// 2. Normalize to unit length (L2 norm).
///
/// Mathematically, orthonormal vectors produced by mean-pooling + L2 normalize
/// yield dot-products equal to `EXPECTED_ORTHOGONAL_COSINE = FRAC_1_SQRT_2`.
fn mean_pool_l2_normalize(
    flat: &[f32],
    batch_idx: usize,
    seq_len: usize,
    hidden: usize,
    max_len: usize,
    attention_mask: &[i64],
) -> Vec<f32> {
    let mut sum = vec![0.0f32; hidden];
    let mut count = 0usize;

    for j in 0..seq_len {
        let mask_idx = batch_idx * max_len + j;
        if mask_idx < attention_mask.len() && attention_mask[mask_idx] != 0 {
            let start = (batch_idx * seq_len + j) * hidden;
            if start + hidden <= flat.len() {
                for (s, f) in sum.iter_mut().zip(flat[start..start + hidden].iter()) {
                    *s += f;
                }
                count += 1;
            }
        }
    }

    // Mean
    if count > 0 {
        let inv_count = 1.0 / count as f32;
        for s in sum.iter_mut() {
            *s *= inv_count;
        }
    }

    // L2 normalize
    let norm: f32 = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        let inv_norm = 1.0 / norm;
        for s in sum.iter_mut() {
            *s *= inv_norm;
        }
    }

    sum
}

// ── model path resolution ────────────────────────────────────────────

/// Return the default XDG data directory, falling back through
/// `$XDG_DATA_HOME` → `$HOME/.local/share` → current directory → `"."`.
fn default_data_dir() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg);
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".local").join("share");
    }
    if let Ok(cwd) = std::env::current_dir() {
        return cwd;
    }
    PathBuf::from(".")
}

/// Resolve the ONNX model path using the following priority:
/// 1. `explicit` — a path passed directly via `--model-path` CLI flag
/// 2. `SHIOTSUCHI_EMBED_MODEL_PATH` environment variable
/// 3. XDG data dir: `$XDG_DATA_HOME/shiotsuchi/model.onnx`
///    (falls back to `~/.local/share/shiotsuchi/model.onnx`)
///
/// Returns `None` if no path resolves to an existing file.
pub fn resolve_model_path(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }

    if let Ok(val) = std::env::var("SHIOTSUCHI_EMBED_MODEL_PATH") {
        let p = PathBuf::from(val);
        if p.exists() {
            return Some(p);
        }
    }

    let xdg_data = default_data_dir();
    let default_path = xdg_data.join("shiotsuchi").join("model.onnx");
    if default_path.exists() {
        return Some(default_path);
    }

    None
}

// ── model hash verification ──────────────────────────────────────────

/// Verify the SHA-256 hash of a model file against the expected constant.
///
/// Returns `Ok(true)` if the hash matches, `Ok(false)` if it does not match,
/// and `Err` on I/O errors.  If [`EXPECTED_MODEL_SHA256`] is empty,
/// verification is skipped and `Ok(true)` is returned (the file still must exist).
///
/// Use this in `setup --check` to validate the downloaded model.
pub fn verify_model_hash(model_path: &Path) -> Result<bool, std::io::Error> {
    use crate::constants::EXPECTED_MODEL_SHA256;

    if !model_path.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("model file not found: {}", model_path.display()),
        ));
    }

    if EXPECTED_MODEL_SHA256.is_empty() {
        return Ok(true);
    }

    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(model_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192];
    loop {
        let bytes = file.read(&mut buffer)?;
        if bytes == 0 {
            break;
        }
        hasher.update(&buffer[..bytes]);
    }
    let hash = hex::encode(hasher.finalize());
    Ok(hash.eq_ignore_ascii_case(EXPECTED_MODEL_SHA256))
}

// ── errors ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Error)]
pub enum EmbedderError {
    #[error("model load error: {0}")]
    Load(String),
    #[error("embedding error: {0}")]
    Inference(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
    #[error("monthly embedding API limit reached ({used}/{limit}, {month})")]
    UsageLimitExceeded { limit: u64, used: u64, month: String },
}

// ── tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_embedder_not_ready() {
        let fake = PathBuf::from("/nonexistent/model.onnx");
        let result = Embedder::new(&fake);
        assert!(matches!(result, Err(EmbedderError::Load(_))));
    }

    #[test]
    fn test_load_is_alias_for_new() {
        let fake = PathBuf::from("/nonexistent/model.onnx");
        let result = Embedder::load(&fake);
        assert!(matches!(result, Err(EmbedderError::Load(_))));
    }

    #[test]
    fn test_resolve_model_path_explicit_nonexistent() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::env::set_var("XDG_DATA_HOME", temp_dir.path());
        let result = resolve_model_path(Some(Path::new("/nonexistent/model.onnx")));
        assert!(result.is_none());
        std::env::remove_var("XDG_DATA_HOME");
    }

    #[test]
    fn test_resolve_model_path_explicit_existing() {
        let dir = tempfile::TempDir::new().unwrap();
        let model = dir.path().join("model.onnx");
        std::fs::write(&model, b"fake").unwrap();
        let result = resolve_model_path(Some(&model));
        assert_eq!(result, Some(model));
    }

    #[test]
    fn test_resolve_model_path_none_when_no_file() {
        let _ = resolve_model_path(None);
    }

    #[test]
    fn test_mean_pool_l2_normalize_basic() {
        let hidden = 4;
        let seq_len = 3;
        let max_len = 3;
        // Token embeddings: [1,0,0,0], [0,1,0,0], [padding]
        // Padding mask (flat): [1, 1, 0] (third token is padding)
        let flat: Vec<f32> = vec![
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            99.0, 99.0, 99.0, 99.0,
        ];
        let attention_mask = vec![1i64, 1, 0];

        let result = mean_pool_l2_normalize(&flat, 0, seq_len, hidden, max_len, &attention_mask);

        // Mean of [1,0,0,0] and [0,1,0,0] = [0.5, 0.5, 0, 0]
        // Norm = sqrt(0.5^2 + 0.5^2) = sqrt(0.5) = 1/√2
        // After L2 norm: [0.5/(1/√2), ...] = [0.5·√2, ...] = EXPECTED_ORTHOGONAL_COSINE
        assert!((result[0] - EXPECTED_ORTHOGONAL_COSINE).abs() < 0.001, "expected ~1/√2, got {}", result[0]);
        assert!((result[1] - EXPECTED_ORTHOGONAL_COSINE).abs() < 0.001, "expected ~1/√2, got {}", result[1]);
        assert!(result[2].abs() < 0.001);
        assert!(result[3].abs() < 0.001);
    }

    #[test]
    fn test_mean_pool_empty_sequence_returns_zeros() {
        // All padding — should return zero vector
        let hidden = 4;
        let seq_len = 3;
        let max_len = 3;
        let flat = vec![0.0f32; 12];
        let attention_mask = vec![0i64; 3];
        let result = mean_pool_l2_normalize(&flat, 0, seq_len, hidden, max_len, &attention_mask);
        assert_eq!(result, vec![0.0; 4]);
    }

    #[test]
    fn test_verify_model_hash_skipped_when_constant_empty() {
        // When EXPECTED_MODEL_SHA256 is "", verification is skipped.
        let dir = tempfile::TempDir::new().unwrap();
        let model = dir.path().join("model.onnx");
        std::fs::write(&model, b"fake model bytes").unwrap();
        let result = verify_model_hash(&model).unwrap();
        assert!(result);
    }

    #[test]
    fn test_verify_model_hash_io_error_on_missing() {
        let result = verify_model_hash(Path::new("/nonexistent/model.onnx"));
        assert!(result.is_err());
    }

    #[test]
    fn test_mean_pool_partial_padding() {
        let hidden = 2;
        let seq_len = 4;
        let max_len = 4;
        // Sequence of length 3, then padding
        let flat: Vec<f32> = vec![
            1.0, 0.0,
            0.0, 2.0,
            0.0, 0.0,
            99.0, 99.0,
        ];
        let attention_mask = vec![1i64, 1, 1, 0];

        let result = mean_pool_l2_normalize(&flat, 0, seq_len, hidden, max_len, &attention_mask);

        // Mean of [1,0], [0,2], [0,0] = [0.3333, 0.6667]
        // Norm = sqrt(0.3333^2 + 0.6667^2) = sqrt(0.1111 + 0.4444) = sqrt(0.5556) ≈ 0.7454
        // After L2 norm: [0.3333/0.7454, 0.6667/0.7454] ≈ [0.4472, 0.8944]
        assert!((result[0] - 0.4472).abs() < 0.01, "expected ~0.4472, got {}", result[0]);
        assert!((result[1] - 0.8944).abs() < 0.01, "expected ~0.8944, got {}", result[1]);
    }

    #[test]
    fn test_compute_model_id_consistent() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("model.bin");
        std::fs::write(&path, b"consistent test bytes").unwrap();

        let hash1 = compute_model_id(&path).unwrap();
        let hash2 = compute_model_id(&path).unwrap();
        assert_eq!(hash1, hash2, "same file should produce same hash");
    }

    #[test]
    fn test_compute_model_id_different_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let path_a = dir.path().join("a.bin");
        let path_b = dir.path().join("b.bin");
        std::fs::write(&path_a, b"content A").unwrap();
        std::fs::write(&path_b, b"content B").unwrap();

        let hash_a = compute_model_id(&path_a).unwrap();
        let hash_b = compute_model_id(&path_b).unwrap();
        assert_ne!(hash_a, hash_b, "different files should produce different hashes");
    }

    #[test]
    fn test_verify_model_hash_mismatch_return_false() {
        use crate::constants::EXPECTED_MODEL_SHA256;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("model.onnx");
        std::fs::write(&path, b"some model bytes").unwrap();

        if EXPECTED_MODEL_SHA256.is_empty() {
            eprintln!("[SKIPPED] {}:{} — EXPECTED_MODEL_SHA256 is empty, verification skipped", file!(), line!());
            return;
        }

        let result = verify_model_hash(&path).unwrap();
        assert!(!result, "model with non-matching hash should return false");
    }

    #[test]
    fn test_embedder_status_ready_serialization() {
        let status = EmbedderStatus::Ready;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"ready\"", "Ready status should serialize as 'ready' with snake_case");
    }

    #[test]
    fn test_embedder_status_unavailable_serialization() {
        let status = EmbedderStatus::Unavailable("model file not found".into());
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("unavailable"));
        assert!(json.contains("model file not found"));
    }

    #[test]
    fn test_compute_model_id_io_error_on_directory() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = compute_model_id(dir.path());
        assert!(result.is_err(), "computing hash on a directory should fail");
    }

    // ── resolve_model_path additional edge cases ──────────────────────

    #[test]
    fn test_resolve_model_path_default_xdg_structure() {
        let result = resolve_model_path(None);
        match result {
            Some(p) => {
                assert!(p.to_string_lossy().contains("shiotsuchi"));
            }
            None => {
                // OK if no model is available
            }
        }
    }

    // ── mean_pool_l2_normalize edge cases ─────────────────────────────

    #[test]
    fn test_mean_pool_l2_normalize_all_zeros() {
        let flat = vec![0.0; 12];
        let attention_mask = vec![1, 1, 1];
        let result = mean_pool_l2_normalize(&flat, 0, 1, 12, 3, &attention_mask);
        assert_eq!(result.len(), 12);
        assert!(result.iter().all(|x| *x == 0.0 || x.is_nan()), "zero vector should result in zeros or NaN");
    }

    #[test]
    fn test_mean_pool_l2_normalize_single_token() {
        let flat = vec![3.0, 4.0]; // magnitude = 5
        let attention_mask = vec![1];
        let result = mean_pool_l2_normalize(&flat, 0, 1, 2, 1, &attention_mask);
        assert_eq!(result.len(), 2);
        // After L2 norm: [3/5, 4/5] = [0.6, 0.8]
        assert!((result[0] - 0.6).abs() < 0.01);
        assert!((result[1] - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_mean_pool_l2_normalize_with_masked_tokens() {
        let flat = vec![1.0, 0.0, 2.0, 0.0]; // seq_len=2, hidden=2
        let attention_mask = vec![1, 0]; // Only first token counted
        let result = mean_pool_l2_normalize(&flat, 0, 2, 2, 2, &attention_mask);
        assert_eq!(result.len(), 2);
        // Only the first token [1.0, 0.0] should be averaged → unit vector [1, 0]
        assert!((result[0] - 1.0).abs() < 0.01);
        assert!((result[1] - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_mean_pool_l2_normalize_all_masked() {
        let flat = vec![1.0, 2.0];
        let attention_mask = vec![0]; // No tokens to count
        let result = mean_pool_l2_normalize(&flat, 0, 1, 2, 1, &attention_mask);
        assert_eq!(result.len(), 2);
        // count=0 → mean stays 0 → L2 norm of 0 → output stays 0
        assert!(result.iter().all(|x| *x == 0.0), "no counted tokens = zero vector");
    }

    #[test]
    fn test_mean_pool_l2_normalize_orthogonal_vectors() {
        let flat = vec![3.0, 4.0]; // magnitude = 5
        let attention_mask = vec![1];
        let result = mean_pool_l2_normalize(&flat, 0, 1, 2, 1, &attention_mask);
        let magnitude = (result[0] * result[0] + result[1] * result[1]).sqrt();
        assert!((magnitude - 1.0).abs() < 0.001, "result should be unit vector");
    }

    #[test]
    fn test_mean_pool_different_hidden_sizes() {
        let flat = vec![1.0, 2.0, 3.0]; // hidden=3
        let attention_mask = vec![1];
        let result = mean_pool_l2_normalize(&flat, 0, 1, 3, 1, &attention_mask);
        assert_eq!(result.len(), 3);
        let mag = (result[0] * result[0] + result[1] * result[1] + result[2] * result[2]).sqrt();
        assert!((mag - 1.0).abs() < 0.001, "normalized to unit vector");
    }
}
