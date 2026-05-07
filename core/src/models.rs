use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

/// Result returned after indexing a file.
#[derive(Debug, Clone, PartialEq)]
pub enum IndexResult {
    /// File was newly inserted.
    Inserted,
    /// File content changed and was updated.
    Updated,
    /// File unchanged (hash matched), skipped.
    Skipped,
    /// Error occurred during indexing.
    Error(String),
}

/// Single search result entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// Relative path of the note.
    pub path: String,
    /// Title of the note.
    pub title: String,
    /// 3-line snippet around the first match.
    pub snippet: String,
    /// BM25 relevance score (lower is more relevant in SQLite FTS5 default rank).
    pub score: f64,
}

/// Statistics about the indexed vault.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VaultStats {
    pub total_notes: usize,
    pub total_size_bytes: usize,
    pub last_indexed_at: Option<i64>,
    pub db_path: PathBuf,
}

/// Configuration for the indexer.
#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// Root directory containing markdown files.
    pub notes_dir: PathBuf,
    /// File extensions to include (e.g., `["md", "markdown"]`).
    pub include_extensions: Vec<String>,
    /// Directory/path patterns to exclude (matched as gitignore-style globs).
    pub exclude_patterns: Vec<String>,
    /// If true, skip directories whose name starts with '.' at the WalkDir level.
    pub auto_exclude_hidden: bool,
    /// If true, follow symbolic links when walking the vault (with vault boundary check).
    pub follow_links: bool,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            notes_dir: PathBuf::from("."),
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            // .git/.obsidian は auto_exclude_hidden により自動除外されるため、
            // exclude_patterns から削除（hidden dir 除外を無効にした場合は
            // ユーザーが明示的に追加する）
            exclude_patterns: vec!["node_modules".to_string()],
            auto_exclude_hidden: true,
            follow_links: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(config.exclude_patterns, vec!["node_modules"]);
        assert!(config.auto_exclude_hidden);
        assert!(config.follow_links);
    }
}
