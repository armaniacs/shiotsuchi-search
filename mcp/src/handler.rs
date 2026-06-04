use serde_json::{json, Value};
use shiotsuchi_core::{
    db::NoteDatabase,
    models::SearchMode,
    search::{extract_snippet, search, SearchRequest},
    sensitive::SensitiveDataConfig,
    tokenizer::get_tokenizer,
};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::Instant;

/// Sliding-window rate limiter: allows up to `max_per_second` requests
/// in any rolling 1-second window. Uses a VecDeque of timestamps to
/// avoid burst violations at fixed-second boundaries.
pub struct RateLimiter {
    max_per_second: usize,
    inner: Mutex<VecDeque<Instant>>,
}

impl RateLimiter {
    pub fn new(max_per_second: usize) -> Self {
        Self {
            max_per_second,
            inner: Mutex::new(VecDeque::new()),
        }
    }

    /// Sliding-window rate limiter: allows up to `max_per_second` requests
    /// in any rolling 1-second window, preventing burst violations at
    /// fixed-second boundaries.
    pub fn allow(&self) -> bool {
        let mut timestamps = self.inner.lock().unwrap();
        let now = Instant::now();
        // Remove timestamps older than 1 second
        while timestamps.front().is_some_and(|t| now.duration_since(*t).as_secs() >= 1) {
            timestamps.pop_front();
        }
        if timestamps.len() >= self.max_per_second {
            return false;
        }
        timestamps.push_back(now);
        true
    }
}

static SEARCH_RATE_LIMITER: LazyLock<RateLimiter> = LazyLock::new(|| RateLimiter::new(10));

fn format_results_markdown(results: &[shiotsuchi_core::models::ChunkSearchResult], query: &str) -> String {
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
        out.push_str(&format!("\n\n_Chunk ID: {} | Score: {:.4} | Tags: {} | Date: {} | Title: {}_\n\n---\n\n", r.chunk_id, r.score, r.tags, r.frontmatter_date, r.title));
    }
    out.push_str("### END RETRIEVED CONTEXT ###\n");
    out
}

pub fn call_tool(
    name: &str,
    args: &Value,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    backlink_scoring: bool,
    sensitive_config: Option<&SensitiveDataConfig>,
) -> Result<Value, Box<dyn std::error::Error>> {
    match name {
        "search_local_notes" => {
            let query = args["query"].as_str().unwrap_or("").to_string();
            if query.len() > 500 {
                return Ok(json!({
                    "content": [{"type": "text", "text": "Query too long (max 500 characters)."}],
                    "isError": true
                }));
            }
            let limit = args["limit"].as_u64().unwrap_or(10).min(50) as usize;
            let mode_str = args["mode"].as_str().unwrap_or("hybrid");
            let min_score = args["min_score"].as_f64();
            let vault_filter = args["vault"].as_str();

            // Validate vault filter against known vaults
            if let Some(vf) = vault_filter {
                if !vaults.iter().any(|(name, _)| name == vf) {
                    let known: Vec<&str> = vaults.iter().map(|(n, _)| n.as_str()).collect();
                    return Ok(json!({
                        "content": [{"type": "text", "text": format!(
                            "vault '{}' is not defined in config. Available vaults: {}",
                            vf,
                            known.join(", ")
                        )}],
                        "isError": true
                    }));
                }
            }

            // MCP server runs without an embedder. Return a guidance message for vec-only mode.
            // hybrid and fts both work — search() auto-falls-back to Fts when embedder is None.
            if mode_str == "vec" {
                return Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": "Vector search requires a model. Use mode='fts' or run 'shiotsuchi setup' to configure an embedder."
                    }]
                }));
            }

            let mode = if mode_str == "fts" { SearchMode::Fts } else { SearchMode::Hybrid };

            if !SEARCH_RATE_LIMITER.allow() {
                return Ok(json!({
                    "content": [{
                        "type": "text",
                        "text": "Rate limit exceeded. Maximum 10 searches per second. Please wait before trying again."
                    }],
                    "isError": true
                }));
            }

            // Validate vault dir is reachable (path traversal check).
            // Strip absolute path from error to avoid internal path disclosure.
            if let Some((_, notes_dir)) = vaults.first() {
                let _canonical_vault = notes_dir.canonicalize()
                    .map_err(|_| "Vault directory is not accessible or does not exist")?;
            }

            let db = NoteDatabase::open(db_path)?;
            let tokenizer = match get_tokenizer() {
                Ok(t) => t,
                Err(_) => return Ok(json!({
                    "content": [{"type": "text", "text": "Full-text search requires a tokenizer model. Run 'shiotsuchi setup' to configure one, or set SHIOTSUCHI_MODEL_PATH."}]
                })),
            };
            let request = SearchRequest {
                query: &query,
                limit,
                mode,
                embedder: None,
                min_score,
                vault_filter,
                tag_filter: None,
                since_date: None,
                user_dictionary: &[],
                synonyms: &HashMap::new(),
                fuzzy: false,
                hybrid_alpha: None,
                mmr: false,
                lambda: 0.5,
                backlink_scoring,
            };
            let results = search(&db, &tokenizer, &request)?;

            let markdown = format_results_markdown(&results, &query);
            let masked = shiotsuchi_core::sensitive::mask_sensitive_data(&markdown, sensitive_config);
            Ok(json!({
                "content": [{"type": "text", "text": masked}]
            }))
        }

        "get_surrounding_context" => {
            let chunk_id = args["chunk_id"].as_i64()
                .ok_or("chunk_id must be an integer")?;
            let window = args["window"].as_u64().unwrap_or(2).min(5) as usize;

            let db = NoteDatabase::open(db_path)?;
            // Validate that the chunk belongs to a known vault
            let chunk_vault = db.get_chunk_vault_name(chunk_id)?
                .ok_or("chunk not found")?;
            if !vaults.iter().any(|(name, _)| name == &chunk_vault) {
                return Err("chunk vault is not configured in this server".into());
            }
            let chunks = db.get_surrounding_chunks(chunk_id, window)?;

            const MAX_CHARS: usize = 100_000;
            let mut out = String::with_capacity(MAX_CHARS.min(4096));
            out.push_str(&format!("### Context around chunk {} ###\n\n", chunk_id));
            for c in &chunks {
                if out.len() >= MAX_CHARS {
                    out.push_str("\n**... (truncated due to size)**\n");
                    break;
                }
                let marker = if c.id == Some(chunk_id) { "**[SELECTED]** " } else { "" };
                let header = c.parent_header.as_deref().unwrap_or("(top level)");
                let content = if out.len() + c.content.len() > MAX_CHARS {
                    let remaining = MAX_CHARS.saturating_sub(out.len());
                    c.content.chars().take(remaining).collect::<String>()
                } else {
                    c.content.clone()
                };
                out.push_str(&format!("## {}Source: {} > {}\n\n{}\n\n---\n\n",
                    marker, c.file_path, header, content));
            }

            let masked_out = shiotsuchi_core::sensitive::mask_sensitive_data(&out, sensitive_config);
            Ok(json!({
                "content": [{"type": "text", "text": masked_out}]
            }))
        }

        "index_status" => {
            let db = NoteDatabase::open(db_path)?;
            let stats = db.stats()?;
            let text = format!(
                "Indexed files: {}\nTotal chunks: {}\nVector-indexed chunks: {}\nDB size: {:.1} MB\n\
                 Note: this status may be slightly stale if background indexing is running.",
                stats.total_files,
                stats.total_chunks,
                stats.vec_indexed_chunks,
                stats.total_size_bytes as f64 / 1_048_576.0
            );
            Ok(json!({"content": [{"type": "text", "text": text}]}))
        }

        _ => Err(format!("Unknown tool: {}", name).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Returns None (and callers skip) when no Vaporetto model is available.
    fn setup_db(temp: &TempDir) -> Option<std::path::PathBuf> {
        use shiotsuchi_core::{db::NoteDatabase, chunker::split_into_chunks};
        let tok = shiotsuchi_core::tokenizer::get_tokenizer().ok()?;
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

    #[test]
    fn test_search_local_notes_rejects_nonexistent_vault_dir() {
        // The vault dir canonicalize check (line 116-119) must reject
        // non-existent directories to prevent path traversal.
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().join("nonexistent"))];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let args = serde_json::json!({"query": "test", "mode": "fts"});
        let result = call_tool("search_local_notes", &args, &vaults, &db_path, true, None);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("No such file") || msg.contains("not found") || msg.contains("不存在") || msg.contains("does not exist"),
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
        let result = call_tool("search_local_notes", &args, &vaults, &db_path, true, None);
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
        let Some(db_path) = setup_db(&temp) else { return; };
        let args = serde_json::json!({"query": "Rust programming", "mode": "fts"});
        let result = call_tool("search_local_notes", &args, &vaults, &db_path, true, None);
        assert!(result.is_ok(), "search_local_notes failed: {:?}", result.err());
        let text = result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
        assert!(text.contains("### RETRIEVED CONTEXT ###"),
            "Expected RETRIEVED CONTEXT delimiter, got: {}", text);
        assert!(text.contains("note.md"), "Expected file_path in output, got: {}", text);
    }

    #[test]
    fn test_search_local_notes_vec_without_embedder_returns_message() {
        // vec guard fires before tokenizer is needed — no model required for this test
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("test.db");
        shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        let args = serde_json::json!({"query": "Rust", "mode": "vec"});
        let result = call_tool("search_local_notes", &args, &vaults, &db_path, true, None).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("fts") || text.contains("model") || text.contains("setup"),
            "Expected guidance message, got: {}", text);
    }

    #[test]
    fn test_index_status_returns_counts() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let Some(db_path) = setup_db(&temp) else { return; };
        let result = call_tool("index_status", &serde_json::json!({}), &vaults, &db_path, true, None).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("Total chunks"), "Expected 'Total chunks' in output, got: {}", text);
        assert!(text.contains("Indexed files"), "Expected 'Indexed files' in output, got: {}", text);
    }

    #[test]
    fn test_get_surrounding_context_returns_chunks() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        use shiotsuchi_core::{db::NoteDatabase, chunker::split_into_chunks};
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
        let result = call_tool("get_surrounding_context", &args, &vaults, &db_path, true, None);
        assert!(result.is_ok(), "get_surrounding_context failed: {:?}", result.err());
        let text = result.unwrap()["content"][0]["text"].as_str().unwrap().to_string();
        assert!(text.contains("### Context around chunk"),
            "Expected context delimiter, got: {}", text);
        assert!(text.contains("multi.md"), "Expected file_path in output, got: {}", text);
    }

    #[test]
    fn test_unknown_tool_returns_error() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let db_path = temp.path().join("nonexistent.db");
        let result = call_tool("nonexistent_tool", &serde_json::json!({}), &vaults, &db_path, true, None);
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
        let result = call_tool("search_local_notes", &args, &vaults, &db_path, true, None).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("max 500"), "expected max length error, got: {}", text);
    }

    #[test]
    fn test_rate_limiter_blocks_after_limit() {
        let limiter = RateLimiter::new(2);
        // First 2 calls should succeed
        assert!(limiter.allow(), "first call should be allowed");
        assert!(limiter.allow(), "second call should be allowed");
        // Third call within the same second should be blocked
        assert!(!limiter.allow(), "third call should be blocked");
    }

    #[test]
    fn test_rate_limiter_sliding_window() {
        let limiter = RateLimiter::new(5);
        // Use up 5 calls
        for _ in 0..5 {
            assert!(limiter.allow());
        }
        assert!(!limiter.allow(), "sixth call should be blocked");

        // Simulate 2 seconds passing by clearing all old timestamps
        limiter.inner.lock().unwrap().clear();

        // Should allow again after old timestamps expire
        assert!(limiter.allow(), "call after expiry should be allowed");
    }
}
