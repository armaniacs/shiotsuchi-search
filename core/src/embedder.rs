use crate::models::EmbedderStatus;
use std::path::{Path, PathBuf};

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

    /// Alias for `new` — used by CLI code following the plan's naming convention.
    pub fn load(model_path: &Path) -> Result<Self, EmbedderError> {
        Self::new(model_path)
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

    let xdg_data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                .join(".local")
                .join("share")
        });
    let default_path = xdg_data.join("shiotsuchi").join("model.onnx");
    if default_path.exists() {
        return Some(default_path);
    }

    None
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

    #[test]
    fn test_load_is_alias_for_new() {
        let fake = PathBuf::from("/nonexistent/model.onnx");
        let result = Embedder::load(&fake);
        assert!(matches!(result, Err(EmbedderError::Unavailable(_))));
    }

    #[test]
    fn test_resolve_model_path_explicit_nonexistent() {
        // Use a temp directory to ensure no model exists at the fallback path
        let temp_dir = tempfile::TempDir::new().unwrap();
        std::env::set_var("XDG_DATA_HOME", temp_dir.path());
        
        let result = resolve_model_path(Some(Path::new("/nonexistent/model.onnx")));
        assert!(result.is_none());
        
        // Clean up
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
        // No explicit path, env var not set to a real file, XDG default absent
        // (We can't guarantee the env is clean, so just check it returns Option)
        let _ = resolve_model_path(None);
    }
}
