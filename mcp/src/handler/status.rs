use serde_json::{json, Value};

use super::ToolContext;

/// Handle the `index_status` MCP tool.
pub(crate) fn handle_index_status(
    ctx: &ToolContext<'_>,
    _args: &Value,
) -> Result<Value, Box<dyn std::error::Error>> {
    let db = ctx.db.lock().unwrap();
    let stats = db.stats()?;
    const BYTES_PER_MB: f64 = 1_048_576.0;
    let text = format!(
        "Indexed files: {}\nTotal chunks: {}\nVector-indexed chunks: {}\nDB size: {:.1} MB\n\
         Note: this status may be slightly stale if background indexing is running.",
        stats.total_files,
        stats.total_chunks,
        stats.vec_indexed_chunks,
        stats.total_size_bytes as f64 / BYTES_PER_MB
    );
    Ok(json!({"content": [{"type": "text", "text": text}]}))
}
