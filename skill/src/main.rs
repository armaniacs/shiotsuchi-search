mod handler;
mod protocol;

use protocol::{JsonRpcRequest, JsonRpcResponse};
use std::{
    io::{self, BufRead, Write},
    path::Path,
};

pub fn dispatch(req: JsonRpcRequest, notes_dir: &Path, db_path: &Path) -> JsonRpcResponse {
    let params = req.params.unwrap_or(serde_json::Value::Null);

    match req.method.as_str() {
        "search-vault" => {
            let query = params["query"].as_str().unwrap_or("");
            let limit = params["limit"].as_u64().unwrap_or(20) as usize;
            match handler::handle_search_vault(query, notes_dir, db_path, limit) {
                Ok(results) => JsonRpcResponse::success(
                    req.id,
                    serde_json::to_value(results).unwrap(),
                ),
                Err(e) => JsonRpcResponse::error(req.id, -32000, &e.to_string()),
            }
        }
        "read-note" => {
            let path = params["path"].as_str().unwrap_or("");
            match handler::handle_read_note(path, notes_dir) {
                Ok(content) => JsonRpcResponse::success(
                    req.id,
                    serde_json::json!({"content": content}),
                ),
                Err(e) => JsonRpcResponse::error(req.id, -32000, &e.to_string()),
            }
        }
        "vault-status" => match handler::handle_vault_status(db_path) {
            Ok(stats) => JsonRpcResponse::success(req.id, stats),
            Err(e) => JsonRpcResponse::error(req.id, -32000, &e.to_string()),
        },
        _ => JsonRpcResponse::error(req.id, -32601, "Method not found"),
    }
}

fn main() {
    env_logger::init();

    let notes_dir = std::env::var("SHIOTSUCHI_NOTES_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."));
    let db_path = std::env::var("SHIOTSUCHI_DB_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".shiotsuchi")
                .join("db.sqlite3")
        });

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = io::BufWriter::new(stdout.lock());

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.is_empty() => l,
            _ => continue,
        };
        let resp = match serde_json::from_str::<JsonRpcRequest>(&line) {
            Ok(req) => dispatch(req, &notes_dir, &db_path),
            Err(_) => JsonRpcResponse::error(0, -32700, "Parse error"),
        };
        writeln!(out, "{}", serde_json::to_string(&resp).unwrap()).ok();
        out.flush().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatch_unknown_method() {
        let req = crate::protocol::JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: 1,
            method: "unknown-method".to_string(),
            params: None,
        };
        let resp = dispatch(req, std::path::Path::new("/tmp"), std::path::Path::new("/tmp/db"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
    }
}
