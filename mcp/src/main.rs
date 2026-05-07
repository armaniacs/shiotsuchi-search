mod handler;
mod protocol;
mod tools;

use clap::Parser;
use protocol::{McpNotification, McpRequest, McpResponse};
use serde::{Deserialize, Serialize};
use shiotsuchi_core::paths::default_db_path as core_default_db_path;
use std::{
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct McpConfig {
    notes_dir: PathBuf,
    db_path: PathBuf,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            notes_dir: PathBuf::from("."),
            db_path: core_default_db_path(),
        }
    }
}

impl McpConfig {
    fn load(path: &Path) -> Self {
        match config::Config::builder()
            .add_source(config::File::from(path))
            .build()
            .and_then(|c| c.try_deserialize::<McpConfig>())
        {
            Ok(cfg) => cfg,
            Err(e) => {
                eprintln!(
                    "Warning: failed to load config from {}: {}. Using defaults.",
                    path.display(),
                    e
                );
                Self::default()
            }
        }
    }

    fn default_config_path() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".config")
            })
            .join("shiotsuchi")
            .join("config.toml")
    }

    fn load_default() -> Self {
        let default_path = Self::default_config_path();
        if default_path.exists() {
            Self::load(&default_path)
        } else {
            Self::default()
        }
    }
}

#[derive(Parser)]
#[command(name = "shiotsuchi-mcp", about = "Shiotsuchi MCP server")]
struct Cli {
    /// Path to config file (TOML). Defaults to ~/.config/shiotsuchi/config.toml
    #[arg(long)]
    config: Option<PathBuf>,
}

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

    let cli = Cli::parse();
    let cfg = match cli.config {
        Some(ref path) => McpConfig::load(path),
        None => McpConfig::load_default(),
    };

    let notes_dir = cfg.notes_dir;
    let db_path = cfg.db_path;

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
    use std::fs;
    use tempfile::TempDir;

    fn write_config(dir: &TempDir, content: &str) -> PathBuf {
        let path = dir.path().join("config.toml");
        fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn test_config_load_notes_dir_and_db_path() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            &tmp,
            r#"
notes_dir = "/tmp/my-notes"
db_path   = "/tmp/my-notes/search.db"
"#,
        );
        let cfg = McpConfig::load(&path);
        assert_eq!(cfg.notes_dir, PathBuf::from("/tmp/my-notes"));
        assert_eq!(cfg.db_path, PathBuf::from("/tmp/my-notes/search.db"));
    }

    #[test]
    fn test_config_defaults_when_file_missing() {
        let cfg = McpConfig::load(Path::new("/nonexistent/path/config.toml"));
        let default = McpConfig::default();
        assert_eq!(cfg.notes_dir, default.notes_dir);
        assert_eq!(cfg.db_path, default.db_path);
    }

    #[test]
    fn test_config_partial_override_uses_defaults_for_missing_fields() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(
            &tmp,
            r#"
notes_dir = "/tmp/partial-notes"
"#,
        );
        let cfg = McpConfig::load(&path);
        assert_eq!(cfg.notes_dir, PathBuf::from("/tmp/partial-notes"));
        assert_eq!(cfg.db_path, McpConfig::default().db_path);
    }

    #[test]
    fn test_config_invalid_toml_falls_back_to_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(&tmp, "this is not valid toml ][[[");
        let cfg = McpConfig::load(&path);
        let default = McpConfig::default();
        assert_eq!(cfg.notes_dir, default.notes_dir);
        assert_eq!(cfg.db_path, default.db_path);
    }

    #[test]
    fn test_config_load_default_returns_defaults_when_no_file() {
        // XDG_CONFIG_HOME を存在しないディレクトリに向けることで
        // ~/.config/shiotsuchi/config.toml を回避しデフォルト値が返ることを確認
        // 注意: 並列テスト実行時の環境変数競合を避けるため直接パスをテスト
        let path = McpConfig::default_config_path();
        if !path.exists() {
            let cfg = McpConfig::load_default();
            let default = McpConfig::default();
            assert_eq!(cfg.notes_dir, default.notes_dir);
            assert_eq!(cfg.db_path, default.db_path);
        }
    }

    #[test]
    fn test_dispatch_tools_list() {
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(1),
            method: "tools/list".to_string(),
            params: None,
        };
        let resp = dispatch(
            req,
            std::path::Path::new("/tmp"),
            std::path::Path::new("/tmp/db"),
        );
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
        let resp = dispatch(
            req,
            std::path::Path::new("/tmp"),
            std::path::Path::new("/tmp/db"),
        );
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
        let resp = dispatch(
            req,
            std::path::Path::new("/tmp"),
            std::path::Path::new("/tmp/db"),
        );
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
        let resp = dispatch(
            req,
            std::path::Path::new("/tmp"),
            std::path::Path::new("/tmp/db"),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
    }
}
