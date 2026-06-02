/// Whether the binary was compiled with semantic search support.
/// When `false`, the `Embedder` type and all vector-search functionality
/// are stubs that return errors at runtime.
pub const SEMANTIC_ENABLED: bool = cfg!(feature = "semantic");

pub mod build_info;
pub mod config;
pub mod constants;
pub mod db;
pub mod chunker;
pub mod frontmatter;
pub mod indexer;
pub mod models;
pub mod paths;
pub mod search;
pub mod tokenizer;
pub mod watcher;

pub mod pdf;
pub mod server;

#[cfg(feature = "vlm")]
pub mod vlm;

#[cfg(feature = "semantic")]
pub mod embedder;

#[cfg(feature = "semantic")]
mod api_embedder;

#[cfg(not(feature = "semantic"))]
pub mod embedder {
    //! Stub module — compiled only when the `semantic` feature is disabled.
    //! Provides minimal type stubs so that all other modules (search, indexer,
    //! watcher, CLI commands) compile without changes.  Every method returns
    //! an error or a no-op value.
    use crate::models::EmbedderStatus;

    /// Stub embedder — always returns errors / no-ops.
    pub struct Embedder;

    impl Embedder {
        pub fn load(_path: &std::path::Path) -> Result<Self, EmbedderError> {
            Err(EmbedderError::Unavailable(
                "compiled without the 'semantic' feature".into(),
            ))
        }
        pub fn new(_path: &std::path::Path) -> Result<Self, EmbedderError> {
            Err(EmbedderError::Unavailable(
                "compiled without the 'semantic' feature".into(),
            ))
        }
        pub fn embed(&self, _text: &str) -> Result<Vec<f32>, EmbedderError> {
            Err(EmbedderError::Unavailable(
                "compiled without the 'semantic' feature".into(),
            ))
        }
        pub fn embed_batch(&self, _texts: &[&str]) -> Vec<Result<Vec<f32>, EmbedderError>> {
            vec![]
        }
        pub fn status(&self) -> EmbedderStatus {
            EmbedderStatus::Unavailable("compiled without the 'semantic' feature".into())
        }
        pub fn model_id(&self) -> &str {
            "none"
        }
    }

    /// Stub error — only the `Unavailable` variant is ever produced.
    #[derive(Debug, thiserror::Error)]
    pub enum EmbedderError {
        #[error("load error: {0}")]
        Load(String),
        #[error("inference error: {0}")]
        Inference(String),
        #[error("unavailable: {0}")]
        Unavailable(String),
    }

    pub fn resolve_model_path(
        _explicit: Option<&std::path::Path>,
    ) -> Option<std::path::PathBuf> {
        None
    }

    pub fn verify_model_hash(_model_path: &std::path::Path) -> Result<bool, std::io::Error> {
        Ok(true)
    }
}

pub use db::NoteDatabase;
pub use indexer::IndexResult;
pub use models::{
    Chunk, ChunkSearchResult, EmbedderStatus, NoteMetadata, SearchConfig, SearchMode, Task, VaultStats,
};
pub use tokenizer::{JapaneseTokenizer, TokenizerConfig};
