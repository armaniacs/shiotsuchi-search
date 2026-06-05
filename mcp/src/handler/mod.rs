mod context;
mod search;
mod status;

use serde_json::Value;
use shiotsuchi_core::{
    rate_limiter::SlidingWindowRateLimiter,
    search::extract_snippet,
    sensitive::SensitiveDataConfig,
};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

pub(crate) use context::handle_get_surrounding_context;
pub(crate) use search::handle_search_local_notes;
pub(crate) use status::handle_index_status;

static SEARCH_RATE_LIMITER: LazyLock<SlidingWindowRateLimiter> =
    LazyLock::new(|| SlidingWindowRateLimiter::new(10));

/// Shared context passed to all tool handlers.
pub(crate) struct ToolContext<'a> {
    pub vaults: &'a [(String, PathBuf)],
    pub db_path: &'a Path,
    pub backlink_scoring: bool,
    pub sensitive_config: Option<&'a SensitiveDataConfig>,
}

fn format_results_markdown(
    results: &[shiotsuchi_core::models::ChunkSearchResult],
    query: &str,
) -> String {
    if results.is_empty() {
        return "No results found.".to_string();
    }

    let mut out = String::from("### RETRIEVED CONTEXT ###\n\n");
    for (i, r) in results.iter().enumerate() {
        let header = r.parent_header.as_deref().unwrap_or("(top level)");
        out.push_str(&format!(
            "## Source {}: {} > {}\n\n",
            i + 1,
            r.file_path,
            header
        ));
        // extract_snippet(text, query, max_lines, max_chars)
        let snippet = extract_snippet(&r.content, query, 3, 800);
        out.push_str(&snippet);
        out.push_str(&format!(
            "\n\n_Chunk ID: {} | Score: {:.4} | Tags: {} | Date: {} | Title: {}_\n\n---\n\n",
            r.chunk_id, r.score, r.tags, r.frontmatter_date, r.title
        ));
    }
    out.push_str("### END RETRIEVED CONTEXT ###\n");
    out
}

/// Dispatch a tool call to the appropriate handler.
pub fn call_tool(
    name: &str,
    args: &Value,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    backlink_scoring: bool,
    sensitive_config: Option<&SensitiveDataConfig>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let ctx = ToolContext {
        vaults,
        db_path,
        backlink_scoring,
        sensitive_config,
    };
    match name {
        "search_local_notes" => handle_search_local_notes(&ctx, args),
        "get_surrounding_context" => handle_get_surrounding_context(&ctx, args),
        "index_status" => handle_index_status(&ctx, args),
        _ => Err(format!("Unknown tool: {}", name).into()),
    }
}

#[cfg(test)]
pub(crate) mod test_helpers {
    use super::ToolContext;
    use shiotsuchi_core::{
        chunker::split_into_chunks, db::NoteDatabase, tokenizer::get_tokenizer,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Sets up an in-memory test database with a single indexed note.
    /// Returns None (and callers skip) when no Vaporetto model is available.
    pub(crate) fn setup_db(temp: &TempDir) -> Option<std::path::PathBuf> {
        let tok = get_tokenizer().ok()?;
        let db_path = temp.path().join("test.db");
        let db = NoteDatabase::open(&db_path).unwrap();
        let chunks = split_into_chunks(
            "# Title\n\nThis is a searchable note about Rust programming.",
            &tok,
            "note.md",
            "default",
            &[],
        );
        db.insert_chunks(&chunks).unwrap();
        Some(db_path)
    }

    pub(crate) fn make_test_ctx<'a>(
        _temp: &'a TempDir,
        vaults: &'a [(String, PathBuf)],
        db_path: &'a std::path::Path,
    ) -> ToolContext<'a> {
        ToolContext {
            vaults,
            db_path,
            backlink_scoring: true,
            sensitive_config: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handler::test_helpers::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn test_search_local_notes_rejects_nonexistent_vault_dir() {
        // The vault dir canonicalize check must reject non-existent directories
        // to prevent path traversal.
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().join("nonexistent"))];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let args = serde_json::json!({"query": "test", "mode": "fts"});
        let result =
            call_tool("search_local_notes", &args, &vaults, &db_path, true, None);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("No such file")
                || msg.contains("not found")
                || msg.contains("不存在")
                || msg.contains("does not exist"),
            "expected directory-not-found error, got: {}",
            msg
        );
    }

    #[test]
    fn test_search_local_notes_rejects_nonexistent_vault_id() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("work".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let args = serde_json::json!({"query": "test", "mode": "fts", "vault": "hobby"});
        let result =
            call_tool("search_local_notes", &args, &vaults, &db_path, true, None);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp["isError"], true);
        let text = resp["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("vault 'hobby' is not defined"));
        assert!(text.contains("work"));
    }

    #[test]
    fn test_search_local_notes_fts_returns_content() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let Some(db_path) = setup_db(&temp) else {
            return;
        };
        let args = serde_json::json!({"query": "Rust programming", "mode": "fts"});
        let result =
            call_tool("search_local_notes", &args, &vaults, &db_path, true, None);
        assert!(result.is_ok(), "search_local_notes failed: {:?}", result.err());
        let text = result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            text.contains("### RETRIEVED CONTEXT ###"),
            "Expected RETRIEVED CONTEXT delimiter, got: {}",
            text
        );
        assert!(
            text.contains("note.md"),
            "Expected file_path in output, got: {}",
            text
        );
    }

    #[test]
    fn test_search_local_notes_vec_without_embedder_returns_message() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let args = serde_json::json!({"query": "Rust", "mode": "vec"});
        let result =
            call_tool("search_local_notes", &args, &vaults, &db_path, true, None).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("fts") || text.contains("model") || text.contains("setup"),
            "Expected guidance message, got: {}",
            text
        );
    }

    #[test]
    fn test_index_status_returns_counts() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let Some(db_path) = setup_db(&temp) else {
            return;
        };
        let result =
            call_tool("index_status", &serde_json::json!({}), &vaults, &db_path, true, None)
                .unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(
            text.contains("Total chunks"),
            "Expected 'Total chunks' in output, got: {}",
            text
        );
        assert!(
            text.contains("Indexed files"),
            "Expected 'Indexed files' in output, got: {}",
            text
        );
    }

    #[test]
    fn test_get_surrounding_context_returns_chunks() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        use shiotsuchi_core::{chunker::split_into_chunks, db::NoteDatabase};
        let tok = match shiotsuchi_core::tokenizer::get_tokenizer() {
            Ok(t) => t,
            Err(_) => return, // skip when no model
        };
        let db_path = temp.path().join("test.db");
        let db = NoteDatabase::open(&db_path).unwrap();
        let chunks = split_into_chunks(
            "# Intro\n\nFirst chunk.\n\n# Body\n\nSecond chunk.\n\n# End\n\nThird chunk.",
            &tok,
            "multi.md",
            "default",
            &[],
        );
        let ids = db.insert_chunks(&chunks).unwrap();
        drop(db);

        let middle_id = ids[ids.len() / 2];
        let args = serde_json::json!({"chunk_id": middle_id, "window": 1});
        let result =
            call_tool("get_surrounding_context", &args, &vaults, &db_path, true, None);
        assert!(
            result.is_ok(),
            "get_surrounding_context failed: {:?}",
            result.err()
        );
        let text = result.unwrap()["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            text.contains("### Context around chunk"),
            "Expected context delimiter, got: {}",
            text
        );
        assert!(
            text.contains("multi.md"),
            "Expected file_path in output, got: {}",
            text
        );
    }

    #[test]
    fn test_unknown_tool_returns_error() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("nonexistent.db");
        let result =
            call_tool("nonexistent_tool", &serde_json::json!({}), &vaults, &db_path, true, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_search_query_max_length_truncated() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let long_query = "x".repeat(501);
        let args = serde_json::json!({"query": long_query, "mode": "fts"});
        let result =
            call_tool("search_local_notes", &args, &vaults, &db_path, true, None).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("max 500"), "expected max length error, got: {}", text);
    }

    #[test]
    fn test_rate_limiter_blocks_after_limit() {
        let limiter = SlidingWindowRateLimiter::new(2);
        assert!(limiter.allow(), "first call should be allowed");
        assert!(limiter.allow(), "second call should be allowed");
        assert!(!limiter.allow(), "third call should be blocked");
    }

    #[test]
    fn test_rate_limiter_sliding_window() {
        let limiter = SlidingWindowRateLimiter::new(5);
        for _ in 0..5 {
            assert!(limiter.allow());
        }
        assert!(!limiter.allow(), "sixth call should be blocked");

        // Simulate 2 seconds passing by clearing all old timestamps
        limiter.clear();

        // Should allow again after old timestamps expire
        assert!(limiter.allow(), "call after expiry should be allowed");
    }

    // --- Direct handler tests (via ToolContext) ---

    #[test]
    fn test_handle_search_local_notes_rejects_long_query() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let ctx = make_test_ctx(&temp, &vaults, &db_path);
        let args = json!({"query": &"x".repeat(501), "mode": "fts"});
        let result = handle_search_local_notes(&ctx, &args).unwrap();
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("max 500"));
    }

    #[test]
    fn test_handle_search_local_notes_rejects_invalid_vault() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("work".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let ctx = make_test_ctx(&temp, &vaults, &db_path);
        let args = json!({"query": "test", "mode": "fts", "vault": "hobby"});
        let result = handle_search_local_notes(&ctx, &args).unwrap();
        assert_eq!(result["isError"], true);
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("vault 'hobby' is not defined"));
    }

    #[test]
    fn test_handle_search_local_notes_vec_mode_returns_guidance() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let ctx = make_test_ctx(&temp, &vaults, &db_path);
        let args = json!({"query": "test", "mode": "vec"});
        let result = handle_search_local_notes(&ctx, &args).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("fts") || text.contains("model"));
    }

    #[test]
    fn test_handle_get_surrounding_context_requires_chunk_id() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let ctx = make_test_ctx(&temp, &vaults, &db_path);
        let args = json!({}); // missing chunk_id
        let result = handle_get_surrounding_context(&ctx, &args);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("chunk_id"));
    }

    #[test]
    fn test_handle_index_status_returns_counts() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let Some(db_path) = setup_db(&temp) else {
            return;
        };
        let ctx = make_test_ctx(&temp, &vaults, &db_path);
        let result = handle_index_status(&ctx, &json!({})).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Total chunks"));
        assert!(text.contains("Indexed files"));
    }
}
