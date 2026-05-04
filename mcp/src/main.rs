mod handler;
mod protocol;
mod tools;

use protocol::{McpNotification, McpRequest, McpResponse};
use std::{
    io::{self, BufRead, Write},
    path::Path,
};
use obsidian_shiotsuchi_vault_core::paths::default_db_path as core_default_db_path;

pub fn dispatch(req: McpRequest, notes_dir: &Path, db_path: &Path) -> McpResponse {
    let params = req.params.clone().unwrap_or(serde_json::Value::Null);

    match req.method.as_str() {
        "initialize" => McpResponse::success(
            req.id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "shiotsuchi-mcp", "version": env!("CARGO_PKG_VERSION") }
            }),
        ),
        "tools/list" => {
            let tool_list = tools::tool_list();
            McpResponse::success(req.id, serde_json::json!({ "tools": tool_list }))
        }
        "tools/call" => {
            let name = params["name"].as_str().unwrap_or("");
            let args = &params["arguments"];
            match handler::call_tool(name, args, notes_dir, db_path) {
                Ok(result) => McpResponse::success(req.id, result),
                Err(_) => McpResponse::error(req.id, -32000, "Internal tool execution error"),
            }
        }
        "ping" => McpResponse::success(req.id, serde_json::json!({})),
        _ => McpResponse::error(req.id, -32601, "Method not found"),
    }
}

fn main() {
    env_logger::init();

    let notes_dir = std::env::var("SHIOTSUCHI_NOTES_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let db_path = std::env::var("SHIOTSUCHI_DB_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| core_default_db_path());

    if let Some(parent) = db_path.parent() {
        if !parent.exists() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    let notif = McpNotification::new("notifications/initialized", serde_json::Value::Null);
    writeln!(out, "{}", serde_json::to_string(&notif).unwrap()).ok();
    out.flush().ok();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.is_empty() => l,
            _ => continue,
        };
        let resp = match serde_json::from_str::<McpRequest>(&line) {
            Ok(req) => dispatch(req, &notes_dir, &db_path),
            Err(_) => McpResponse::error(0, -32700, "Parse error"),
        };
        writeln!(out, "{}", serde_json::to_string(&resp).unwrap()).ok();
        out.flush().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_tools_list() {
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(1),
            method: "tools/list".to_string(),
            params: None,
        };
        let resp = dispatch(req, std::path::Path::new("/tmp"), std::path::Path::new("/tmp/db"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("search_vault"));
    }

    #[test]
    fn test_dispatch_unknown_method() {
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(2),
            method: "unknown".to_string(),
            params: None,
        };
        let resp = dispatch(req, std::path::Path::new("/tmp"), std::path::Path::new("/tmp/db"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
    }

    #[test]
    fn test_dispatch_initialize() {
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(1),
            method: "initialize".to_string(),
            params: None,
        };
        let resp = dispatch(req, std::path::Path::new("/tmp"), std::path::Path::new("/tmp/db"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("protocolVersion"));
        assert!(json.contains("shiotsuchi-mcp"));
    }

    #[test]
    fn test_dispatch_ping() {
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(3),
            method: "ping".to_string(),
            params: None,
        };
        let resp = dispatch(req, std::path::Path::new("/tmp"), std::path::Path::new("/tmp/db"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
    }
}
