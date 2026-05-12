use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A single chunk split from a Markdown file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chunk {
    /// Set by the DB after INSERT; None before persisting.
    pub id: Option<i64>,
    pub file_path: String,
    pub chunk_index: i64,
    /// Ancestor header path, e.g. "大見出し > 中見出し". None for top-level chunks.
    pub parent_header: Option<String>,
    /// Raw Markdown content (human-readable, used for snippets/display).
    pub content: String,
    /// Vaporetto-tokenized, space-separated text for FTS5 indexing.
    pub tokenized_content: String,
}

/// A search result backed by the new chunk schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkSearchResult {
    pub chunk_id: i64,
    pub file_path: String,
    pub parent_header: Option<String>,
    pub content: String,
    /// Lower is more relevant for FTS (BM25); higher is more relevant for vec (cosine).
    pub score: f64,
    pub search_mode: SearchMode,
}

/// Which retrieval strategy was used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchMode {
    Fts,
    Vec,
    #[default]
    Hybrid,
}

/// Status of the embedder (model availability).
#[derive(Debug, Clone, PartialEq)]
pub enum EmbedderStatus {
    /// Model loaded and ready.
    Ready,
    /// Model file not found — FTS-only mode.
    Unavailable(String),
}

/// Metadata for a single note stored in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteMetadata {
    /// Relative path within the notes directory (forward slashes).
    pub path: String,
    /// SHA-256 hash of the original file content (hex string).
    pub hash: String,
    /// Last modified time (Unix timestamp, seconds).
    pub mtime: i64,
    /// When this record was last indexed (Unix timestamp, seconds).
    pub indexed_at: i64,
    /// Title extracted from frontmatter or filename.
    pub title: String,
}

/// Statistics about the indexed vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_chunks: usize,
    pub total_files: usize,
    pub total_size_bytes: usize,
    pub last_indexed_at: Option<i64>,
    pub db_path: PathBuf,
    pub vec_indexed_chunks: usize,
    pub embedder_status: String,
}

/// Configuration for search result display.
#[derive(Debug, Clone)]
pub struct SearchConfig {
    /// Maximum characters allowed in a snippet (clamped to 128–65 535).
    /// Default: 1000.
    pub max_snippet_chars: usize,
}

const MIN_SNIPPET_CHARS: usize = 128;
const MAX_SNIPPET_CHARS_LIMIT: usize = 65535;
const DEFAULT_SNIPPET_CHARS: usize = 1000;

impl SearchConfig {
    pub fn new(value: usize) -> Self {
        Self {
            max_snippet_chars: value.clamp(MIN_SNIPPET_CHARS, MAX_SNIPPET_CHARS_LIMIT),
        }
    }
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_snippet_chars: DEFAULT_SNIPPET_CHARS,
        }
    }
}

/// Configuration for the indexer.
#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// Root directory containing markdown files.
    pub notes_dir: PathBuf,
    /// File extensions to include (e.g., `["md", "markdown"]`).
    pub include_extensions: Vec<String>,
    /// Directory names to exclude (matched as gitignore-style component globs).
    /// Renamed from `exclude_patterns` — the old key will cause a deserialize error.
    pub exclude_dirs: Vec<String>,
    /// If true, skip directories whose name starts with '.' at the WalkDir level.
    pub auto_exclude_hidden: bool,
    /// If true, follow symbolic links when walking the vault (with vault boundary check).
    pub follow_links: bool,
    /// Minimum number of matching files for a directory to be dynamically detected
    /// as a noise candidate. Defaults to 5.
    pub dynamic_threshold: usize,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            notes_dir: PathBuf::from("."),
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            // .git/.obsidian は auto_exclude_hidden により自動除外されるため、
            // exclude_dirs から削除（hidden dir 除外を無効にした場合は
            // ユーザーが明示的に追加する）
            exclude_dirs: vec!["node_modules".to_string()],
            auto_exclude_hidden: true,
            follow_links: false,
            dynamic_threshold: 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_serde_roundtrip() {
        let chunk = Chunk {
            id: None,
            file_path: "notes/a.md".to_string(),
            chunk_index: 0,
            parent_header: Some("Section > Sub".to_string()),
            content: "Some content".to_string(),
            tokenized_content: "Some content".to_string(),
        };
        let json = serde_json::to_string(&chunk).unwrap();
        let decoded: Chunk = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.content, "Some content");
        assert_eq!(decoded.parent_header.unwrap(), "Section > Sub");
    }

    #[test]
    fn search_mode_default_is_hybrid() {
        let mode = SearchMode::default();
        assert!(matches!(mode, SearchMode::Hybrid));
    }

    #[test]
    fn note_metadata_serde_roundtrip() {
        let meta = NoteMetadata {
            path: "projects/meeting.md".to_string(),
            hash: "abc123".to_string(),
            mtime: 1714320000,
            indexed_at: 1714320000,
            title: "Meeting Notes".to_string(),
        };
        let json = serde_json::to_string(&meta).unwrap();
        let decoded: NoteMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, decoded);
    }

    #[test]
    fn default_index_config() {
        let config = IndexConfig::default();
        assert_eq!(config.include_extensions, vec!["md", "markdown"]);
        assert_eq!(config.exclude_dirs, vec!["node_modules"]);
        assert!(config.auto_exclude_hidden);
        assert!(!config.follow_links);
        assert_eq!(config.dynamic_threshold, 5);
    }
}
