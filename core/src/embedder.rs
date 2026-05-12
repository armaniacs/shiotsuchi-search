use crate::models::EmbedderStatus;
use std::path::Path;

/// High-level wrapper around the ONNX embedding model.
///
/// The actual session loading and inference will be implemented in a later task.
/// Right now this is a placeholder so that the rest of the codebase can
/// import and type-check against `Embedder`.
#[derive(Debug)]
pub struct Embedder {
    // In the future: ort::Session
    _private: (),
}

impl Embedder {
    /// Construct an `Embedder` from a model file path.
    ///
    /// Currently returns `Err(EmbedderError::Unavailable)` to indicate that
    /// the embedding pipeline is not yet functional.  The `setup` command
    /// will eventually download the model and this constructor will be
    /// completed.
    pub fn new(_model_path: &Path) -> Result<Self, EmbedderError> {
        Err(EmbedderError::Unavailable("embedder not yet implemented".to_string()))
    }

    /// Embed a single text and return a 1024-dimensional float vector.
    ///
    /// Placeholder — returns `unimplemented!()`.
    pub fn embed(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
        unimplemented!("embed() will be implemented after model download and session wiring")
    }

    /// Get current embedder status.
    pub fn status(&self) -> EmbedderStatus {
        EmbedderStatus::Unavailable("model not loaded".to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EmbedderError {
    #[error("model load error: {0}")]
    Load(String),
    #[error("embedding error: {0}")]
    Inference(String),
    #[error("unavailable: {0}")]
    Unavailable(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_embedder_not_ready() {
        let fake = PathBuf::from("/nonexistent/model.onnx");
        let result = Embedder::new(&fake);
        assert!(matches!(result, Err(EmbedderError::Unavailable(_))));
    }
}
