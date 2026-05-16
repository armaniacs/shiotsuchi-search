use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

pub fn tool_list() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "search_local_notes".to_string(),
            description: "Search the user's local Markdown vault. \
                Use this when the user asks about their notes, past writing, or knowledge base. \
                mode: 'hybrid' (default, highest accuracy), 'fts' (keyword-only, works without model), \
                'vec' (semantic, requires model). \
                Returns file path, parent heading hierarchy, content snippet, and relevance score."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Japanese or English search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (1–50)",
                        "default": 10,
                        "minimum": 1,
                        "maximum": 50
                    },
                    "min_score": {
                        "type": "number",
                        "description": "Minimum relevance score threshold (optional)"
                    },
                    "mode": {
                        "type": "string",
                        "enum": ["fts", "vec", "hybrid"],
                        "description": "Search mode. Default: hybrid",
                        "default": "hybrid"
                    }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "get_surrounding_context".to_string(),
            description: "Retrieve chunks immediately before and after a given chunk. \
                Use this after search_local_notes when you need more context around a result. \
                chunk_id comes from a search_local_notes result."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "chunk_id": {
                        "type": "integer",
                        "description": "ID of the chunk to expand context around"
                    },
                    "window": {
                        "type": "integer",
                        "description": "Number of chunks before and after to retrieve (1–5)",
                        "default": 2,
                        "minimum": 1,
                        "maximum": 5
                    }
                },
                "required": ["chunk_id"]
            }),
        },
        ToolDef {
            name: "index_status".to_string(),
            description: "Return indexing statistics: total files, total chunks, \
                how many chunks have vector embeddings, and the embedder model in use. \
                Note: this reflects the state at query time and may be slightly stale \
                if background indexing is running."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolDef {
            name: "rebuild_index".to_string(),
            description: "Trigger a full re-index of the vault (delete all chunks and reindex). \
                This runs in the background and may take several minutes for large vaults. \
                Use only when the index seems corrupted or after a major vault restructure."
                .to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_list_has_four_tools() {
        let tools = tool_list();
        assert_eq!(tools.len(), 4);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"search_local_notes"));
        assert!(names.contains(&"get_surrounding_context"));
        assert!(names.contains(&"index_status"));
        assert!(names.contains(&"rebuild_index"));
        // Old tools must be gone
        assert!(!names.contains(&"search_vault"));
        assert!(!names.contains(&"read_full_note"));
        assert!(!names.contains(&"vault_status"));
    }

    #[test]
    fn test_all_tools_have_descriptions() {
        for tool in tool_list() {
            assert!(!tool.description.is_empty(), "Tool {} has empty description", tool.name);
        }
    }
}
