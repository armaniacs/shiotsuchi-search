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
            name: "search_vault".to_string(),
            description: "Search the user's Markdown vault for notes matching a query. Returns paths, snippets, and relevance scores.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Japanese or English search query" }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "read_full_note".to_string(),
            description: "Read the complete Markdown content of a specific note by its relative path within the vault.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Relative path inside vault (e.g., 'projects/meeting.md')" }
                },
                "required": ["path"]
            }),
        },
        ToolDef {
            name: "vault_status".to_string(),
            description: "Get vault indexing statistics: total notes, last updated, database size.".to_string(),
            input_schema: serde_json::json!({ "type": "object", "properties": {} }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_list_has_three_tools() {
        let tools = tool_list();
        assert_eq!(tools.len(), 3);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"search_vault"));
        assert!(names.contains(&"read_full_note"));
        assert!(names.contains(&"vault_status"));
    }
}
