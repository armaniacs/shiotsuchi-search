use serde_json::Value;
use shiotsuchi_core::{db::NoteDatabase, search::search, tokenizer::get_tokenizer};
use std::{fs, path::Path};

fn text_content(text: impl Into<String>) -> Value {
    serde_json::json!({ "content": [{ "type": "text", "text": text.into() }] })
}

pub fn call_tool(
    name: &str,
    args: &Value,
    notes_dir: &Path,
    db_path: &Path,
) -> Result<Value, Box<dyn std::error::Error>> {
    match name {
        "search_vault" => {
            let query = args["query"].as_str().unwrap_or("");
            let db = NoteDatabase::open(db_path)?;
            let tokenizer = get_tokenizer()?;
            let results = search(&db, &tokenizer, notes_dir, query, 20)?;
            let text = serde_json::to_string_pretty(&results)?;
            Ok(text_content(text))
        }
        "read_full_note" => {
            let path = args["path"].as_str().unwrap_or("");
            if path.starts_with('/') || path.contains("..") {
                return Err("Invalid path: must be relative and within vault".into());
            }
            let full_path = notes_dir.join(path);
            let canonical = full_path.canonicalize()?;
            let vault_canonical = notes_dir.canonicalize()?;
            if !canonical.starts_with(&vault_canonical) {
                return Err("Path escapes vault directory".into());
            }
            let content = fs::read_to_string(&canonical)?;
            Ok(text_content(content))
        }
        "vault_status" => {
            let db = NoteDatabase::open(db_path)?;
            let stats = db.stats()?;
            let text = format!(
                "Total notes: {}\nDB size: {} bytes\nLast indexed: {}",
                stats.total_notes,
                stats.total_size_bytes,
                stats
                    .last_indexed_at
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "never".to_string()),
            );
            Ok(text_content(text))
        }
        _ => Err(format!("Unknown tool: {}", name).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn indexed_vault() -> (TempDir, std::path::PathBuf) {
        let temp = TempDir::new().unwrap();
        fs::write(
            temp.path().join("note.md"),
            "# Hello\n\nMCP integration test.",
        )
        .unwrap();
        let db = temp.path().join("test.db");
        use shiotsuchi_core::{
            db::NoteDatabase, indexer::index_directory, models::IndexConfig,
            tokenizer::get_tokenizer,
        };
        let ndb = NoteDatabase::open(&db).unwrap();
        let tok =
            get_tokenizer().unwrap_or_else(|_| panic!("SHIOTSUCHI_MODEL_PATH を設定してください"));
        let cfg = IndexConfig {
            notes_dir: temp.path().to_path_buf(),
            ..Default::default()
        };
        index_directory(&ndb, &tok, &cfg).unwrap();
        (temp, db)
    }

    #[test]
    fn test_call_search_vault() {
        let (temp, db) = indexed_vault();
        let args = serde_json::json!({"query": "MCP integration"});
        let result = call_tool("search_vault", &args, temp.path(), &db).unwrap();
        let content = &result["content"];
        assert!(content.is_array());
        assert!(!content.as_array().unwrap().is_empty());
    }

    #[test]
    fn test_call_vault_status() {
        let (_temp, db) = indexed_vault();
        let result = call_tool(
            "vault_status",
            &serde_json::Value::Null,
            std::path::Path::new("/tmp"),
            &db,
        )
        .unwrap();
        assert!(result["content"][0]["text"].as_str().unwrap().contains("1"));
    }

    #[test]
    fn test_call_read_full_note() {
        let (temp, db) = indexed_vault();
        let args = serde_json::json!({"path": "note.md"});
        let result = call_tool("read_full_note", &args, temp.path(), &db).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Hello"));
    }

    #[test]
    fn test_path_traversal_rejected() {
        let (temp, db) = indexed_vault();
        let args = serde_json::json!({"path": "../secret.txt"});
        let result = call_tool("read_full_note", &args, temp.path(), &db);
        assert!(result.is_err());
    }
}
