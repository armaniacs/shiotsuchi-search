pub mod build_info;
pub mod config;
pub mod constants;
pub mod db;
pub mod chunker;
pub mod embedder;
pub mod indexer;
pub mod models;
pub mod paths;
pub mod search;
pub mod tokenizer;
pub mod watcher;

pub use db::NoteDatabase;
pub use indexer::IndexResult;
pub use models::{
    Chunk, ChunkSearchResult, EmbedderStatus, NoteMetadata, SearchConfig, SearchMode, VaultStats,
};
pub use tokenizer::{JapaneseTokenizer, TokenizerConfig};
