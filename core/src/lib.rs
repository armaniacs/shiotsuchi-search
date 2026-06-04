pub mod build_info;
pub mod chunker;
pub mod config;
pub mod constants;
pub mod db;
pub mod embedder;
pub mod indexer;
pub mod models;
pub mod paths;
pub mod search;
pub mod sensitive;
pub mod sensitive_patterns;
pub mod tokenizer;
pub mod watcher;

pub use db::NoteDatabase;
pub use indexer::IndexResult;
pub use models::{
    Chunk, ChunkSearchResult, EmbedderStatus, NoteMetadata, SearchConfig, SearchMode, VaultStats,
};
pub use tokenizer::{JapaneseTokenizer, TokenizerConfig};
pub use sensitive::SensitiveDataConfig;
