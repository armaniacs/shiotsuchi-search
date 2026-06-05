use serde_json::{json, Value};
use shiotsuchi_core::db::NoteDatabase;

use super::ToolContext;

/// Handle the `get_surrounding_context` MCP tool.
pub(crate) fn handle_get_surrounding_context(
    ctx: &ToolContext<'_>,
    args: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let chunk_id = args["chunk_id"]
        .as_i64()
        .ok_or("chunk_id must be an integer")?;
    let window = args["window"].as_u64().unwrap_or(2).min(5) as usize;

    let db = NoteDatabase::open(ctx.db_path)?;
    // Validate that the chunk belongs to a known vault
    let chunk_vault = db
        .get_chunk_vault_name(chunk_id)?
        .ok_or("chunk not found or inaccessible")?;
    if !ctx.vaults.iter().any(|(name, _)| name == &chunk_vault) {
        return Err("chunk not found or inaccessible".into());
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
        let marker = if c.id == Some(chunk_id) {
            "**[SELECTED]** "
        } else {
            ""
        };
        let header = c.parent_header.as_deref().unwrap_or("(top level)");
        let content = if out.len() + c.content.len() > MAX_CHARS {
            let remaining = MAX_CHARS.saturating_sub(out.len());
            c.content.chars().take(remaining).collect::<String>()
        } else {
            c.content.clone()
        };
        out.push_str(&format!(
            "## {}Source: {} > {}\n\n{}\n\n---\n\n",
            marker, c.file_path, header, content
        ));
    }

    let masked_out =
        shiotsuchi_core::sensitive::mask_sensitive_data(&out, ctx.sensitive_config);
    Ok(json!({
        "content": [{"type": "text", "text": masked_out}]
    }))
}
