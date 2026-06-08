use serde_json::{json, Value};
use shiotsuchi_core::{
    models::SearchMode,
    rate_limiter::SlidingWindowRateLimiter,
    search::{extract_snippet, search, SearchRequest},
    tokenizer::get_tokenizer,
};
use std::collections::HashMap;
use std::sync::LazyLock;

use super::ToolContext;

/// Rate limiter for search_local_notes (10 requests/second).
pub(crate) static SEARCH_RATE_LIMITER: LazyLock<SlidingWindowRateLimiter> =
    LazyLock::new(|| SlidingWindowRateLimiter::new(10));

/// Handle the `search_local_notes` MCP tool.
pub(crate) fn handle_search_local_notes(
    ctx: &ToolContext<'_>,
    args: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
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
        if !ctx.vaults.iter().any(|(name, _)| name == vf) {
            let known: Vec<&str> = ctx.vaults.iter().map(|(n, _)| n.as_str()).collect();
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
    if mode_str == "vec" {
        return Ok(json!({
            "content": [{
                "type": "text",
                "text": "Vector search requires a model. Use mode='fts' or run 'shiotsuchi setup' to configure an embedder."
            }]
        }));
    }

    let mode: SearchMode = match mode_str.parse() {
        Ok(m) => m,
        Err(_) => {
            return Ok(json!({
                "content": [{"type": "text", "text": format!("Unknown mode '{}'. Supported modes: fts, hybrid", mode_str)}],
                "isError": true
            }))
        }
    };

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
    let target_name = vault_filter
        .or_else(|| ctx.vaults.first().map(|(n, _)| n.as_str()))
        .unwrap_or("default");
    shiotsuchi_core::resolve_vault_dir(ctx.vaults, target_name)
        .map_err(|e| e.to_string())?;

    let db = ctx.db.lock().unwrap();
    let db_conn = db.read_conn.as_ref().unwrap().borrow();
    let tokenizer = match get_tokenizer() {
        Ok(t) => t,
        Err(_) => {
            return Ok(json!({
                "content": [{"type": "text", "text": "Full-text search requires a tokenizer model. Run 'shiotsuchi setup' to configure one, or set SHIOTSUCHI_MODEL_PATH."}]
            }))
        }
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
        backlink_scoring: ctx.backlink_scoring,
        cursor: None,
    };
    let output = search(&db_conn, &tokenizer, &request)?;

    let markdown = format_results_markdown(&output.results, &query);
    let masked =
        shiotsuchi_core::sensitive::mask_sensitive_data(&markdown, Some(ctx.sensitive_config));
    Ok(json!({
        "content": [{"type": "text", "text": masked}]
    }))
}

/// Format search results into a structured markdown block for MCP response.
pub(crate) fn format_results_markdown(
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
