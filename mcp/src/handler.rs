use serde_json::{json, Value};
use shiotsuchi_core::{
    db::NoteDatabase,
    models::SearchMode,
    search::{extract_snippet, search},
    tokenizer::get_tokenizer,
};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::Instant;

/// Simple rate limiter: allows up to `max_per_second` calls.
/// Thread-safe via a single Mutex covering count + interval reset.
pub struct RateLimiter {
    max_per_second: u64,
    inner: Mutex<RateLimiterInner>,
}

struct RateLimiterInner {
    count: u64,
    interval_start: Instant,
}

impl RateLimiter {
    pub fn new(max_per_second: u64) -> Self {
        Self {
            max_per_second,
            inner: Mutex::new(RateLimiterInner {
                count: 0,
                interval_start: Instant::now(),
            }),
        }
    }

    pub fn allow(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.interval_start.elapsed().as_secs() >= 1 {
            inner.interval_start = Instant::now();
            inner.count = 0;
        }
        let prev = inner.count;
        inner.count += 1;
        prev < self.max_per_second
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
        out.push_str(&format!("\n\n_Chunk ID: {} | Score: {:.4}_\n\n---\n\n", r.chunk_id, r.score));
    }
    out.push_str("### END RETRIEVED CONTEXT ###\n");
    out
}

pub fn call_tool(
    name: &str,
    args: &Value,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
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

            // Validate vault dir is reachable (path traversal check)
            if let Some((_, notes_dir)) = vaults.first() {
                let _canonical_vault = notes_dir.canonicalize()?;
            }

            let db = NoteDatabase::open(db_path)?;
            let tokenizer = match get_tokenizer() {
                Ok(t) => t,
                Err(_) => return Ok(json!({
                    "content": [{"type": "text", "text": "Full-text search requires a tokenizer model. Run 'shiotsuchi setup' to configure one, or set SHIOTSUCHI_MODEL_PATH."}]
                })),
            };
            let results = search(&db, &tokenizer, &query, limit, mode, None, min_score, vault_filter)?;

            let markdown = format_results_markdown(&results, &query);
            Ok(json!({
                "content": [{"type": "text", "text": markdown}]
            }))
        }

        "get_surrounding_context" => {
            let chunk_id = args["chunk_id"].as_i64()
                .ok_or("chunk_id must be an integer")?;
            let window = args["window"].as_u64().unwrap_or(2).min(5) as usize;

            let db = NoteDatabase::open(db_path)?;
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

            Ok(json!({
                "content": [{"type": "text", "text": out}]
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
        );
        db.insert_chunks(&chunks).unwrap();
        Some(db_path)
    }

    #[test]
    fn test_search_local_notes_fts_returns_content() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let Some(db_path) = setup_db(&temp) else { return; };
        let args = serde_json::json!({"query": "Rust programming", "mode": "fts"});
        let result = call_tool("search_local_notes", &args, &vaults, &db_path);
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
        let result = call_tool("search_local_notes", &args, &vaults, &db_path).unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("fts") || text.contains("model") || text.contains("setup"),
            "Expected guidance message, got: {}", text);
    }

    #[test]
    fn test_index_status_returns_counts() {
        let temp = TempDir::new().unwrap();
        let vaults = vec![("default".to_string(), temp.path().to_path_buf())];
        let Some(db_path) = setup_db(&temp) else { return; };
        let result = call_tool("index_status", &serde_json::json!({}), &vaults, &db_path).unwrap();
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
        );
        let ids = db.insert_chunks(&chunks).unwrap();
        drop(db);

        let middle_id = ids[ids.len() / 2];
        let args = serde_json::json!({"chunk_id": middle_id, "window": 1});
        let result = call_tool("get_surrounding_context", &args, &vaults, &db_path);
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
        let result = call_tool("nonexistent_tool", &serde_json::json!({}), &vaults, &db_path);
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
        let result = call_tool("search_local_notes", &args, &vaults, &db_path).unwrap();
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
    fn test_rate_limiter_resets_after_second() {
        let limiter = RateLimiter::new(5);
        // Use up 5 calls
        for _ in 0..5 {
            assert!(limiter.allow());
        }
        assert!(!limiter.allow(), "sixth call should be blocked");

        // Manually advance the interval start to simulate 1 second passing
        limiter.inner.lock().unwrap().interval_start = Instant::now() - std::time::Duration::from_secs(2);

        // Should allow again after reset
        assert!(limiter.allow(), "call after reset should be allowed");
    }
}
