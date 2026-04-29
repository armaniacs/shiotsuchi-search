use obsidian_shiotsuchi_vault_core::{
    db::NoteDatabase,
    models::SearchResult,
    search::search,
    tokenizer::{JapaneseTokenizer, TokenizerConfig},
};
use serde_json::Value;
use std::{fs, path::Path};

pub fn handle_search_vault(
    query: &str,
    notes_dir: &Path,
    db_path: &Path,
    limit: usize,
) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    let tokenizer = JapaneseTokenizer::new(TokenizerConfig::default())?;
    Ok(search(&db, &tokenizer, notes_dir, query, limit)?)
}

pub fn handle_vault_status(db_path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    let stats = db.stats()?;
    Ok(serde_json::json!({
        "total_notes": stats.total_notes,
        "total_size_bytes": stats.total_size_bytes,
        "last_indexed_at": stats.last_indexed_at,
    }))
}

pub fn handle_read_note(
    path: &str,
    notes_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>> {
    if path.starts_with('/') || path.contains("..") {
        return Err("Invalid path: must be relative and within vault".into());
    }
    let full_path = notes_dir.join(path);
    let canonical = full_path.canonicalize()?;
    let vault_canonical = notes_dir.canonicalize()?;
    if !canonical.starts_with(&vault_canonical) {
        return Err("Path escapes vault directory".into());
    }
    Ok(fs::read_to_string(&canonical)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_vault_with_db() -> (TempDir, std::path::PathBuf) {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("note.md"), "# Hello\n\nThis is a skill test.").unwrap();
        let db = temp.path().join("test.db");
        use obsidian_shiotsuchi_vault_core::{
            db::NoteDatabase,
            indexer::index_directory,
            models::IndexConfig,
            tokenizer::{JapaneseTokenizer, TokenizerConfig},
        };
        let ndb = NoteDatabase::open(&db).unwrap();
        let tok = JapaneseTokenizer::new(TokenizerConfig::default())
            .unwrap_or_else(|_| panic!("SHIOTSUCHI_MODEL_PATH を設定してください"));
        let cfg = IndexConfig { notes_dir: temp.path().to_path_buf(), ..Default::default() };
        index_directory(&ndb, &tok, &cfg).unwrap();
        (temp, db)
    }

    #[test]
    fn test_handle_search_vault() {
        let (temp, db) = make_vault_with_db();
        let result = handle_search_vault("skill test", temp.path(), &db, 10).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_handle_vault_status() {
        let (_temp, db) = make_vault_with_db();
        let stats = handle_vault_status(&db).unwrap();
        assert_eq!(stats["total_notes"], 1);
    }

    #[test]
    fn test_handle_read_note() {
        let (temp, db) = make_vault_with_db();
        let _ = db;
        let content = handle_read_note("note.md", temp.path()).unwrap();
        assert!(content.contains("Hello"));
    }

    #[test]
    fn test_handle_read_note_path_traversal_rejected() {
        let temp = TempDir::new().unwrap();
        let result = handle_read_note("../secret.txt", temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_handle_read_note_absolute_path_rejected() {
        let temp = TempDir::new().unwrap();
        let result = handle_read_note("/etc/passwd", temp.path());
        assert!(result.is_err());
    }
}
