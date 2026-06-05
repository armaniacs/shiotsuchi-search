use serde_json::{json, Value};
use shiotsuchi_core::{
    db::NoteDatabase,
    models::SearchMode,
    search::{search, SearchRequest},
    tokenizer::get_tokenizer,
};
use std::collections::HashMap;

use super::ToolContext;
use crate::handler::SEARCH_RATE_LIMITER;

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

    let mode = if mode_str == "fts" {
        SearchMode::Fts
    } else {
        SearchMode::Hybrid
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
    if let Some((_, notes_dir)) = ctx.vaults.first() {
        let _canonical_vault = notes_dir
            .canonicalize()
            .map_err(|_| "Vault directory is not accessible or does not exist")?;
    }

    let db = NoteDatabase::open(ctx.db_path)?;
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
    };
    let results = search(&db, &tokenizer, &request)?;

    let markdown = super::format_results_markdown(&results, &query);
    let masked =
        shiotsuchi_core::sensitive::mask_sensitive_data(&markdown, ctx.sensitive_config);
    Ok(json!({
        "content": [{"type": "text", "text": masked}]
    }))
}
