mod handler;
mod protocol;
mod tools;

use clap::Parser;
use protocol::{McpNotification, McpRequest, McpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use shiotsuchi_core::paths::default_db_path as core_default_db_path;
use std::{
    ffi::OsString,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

// Re-import Write in inner blocks for stdout writes.

/// Resolve a path from an environment variable with traversal validation.
/// Falls back to `default` if the variable is unset or contains `..` traversal.
fn resolve_path_env(var: &str, default: PathBuf) -> PathBuf {
    let val: Option<OsString> = std::env::var_os(var).filter(|v| !v.is_empty());
    match val {
        Some(v) => {
            let p = PathBuf::from(&v);
            // Only reject relative paths with '..' traversal.
            // Absolute paths like /home/user/../config are allowed.
            if !p.is_absolute() && p.to_string_lossy().contains("..") {
                eprintln!(
                    "Warning: {} contains '..' (path traversal), using config default",
                    var
                );
                default
            } else {
                p
            }
        }
        None => default,
    }
}

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

/// Spawn a background task that calls `shiotsuchi_core::indexer::index_directory`
/// and sends MCP `notifications/progress` notifications on stdout.
fn spawn_rebuild(
    notes_dir: &Path,
    db_path: &Path,
    stdout: &Arc<Mutex<io::Stdout>>,
    _args: &serde_json::Value,
    progress_token: Option<u64>,
) {
    let n_dir = notes_dir.to_path_buf();
    let d_path = db_path.to_path_buf();
    let out = Arc::clone(stdout);

    tokio::spawn(async move {
        // Notify: rebuild started
        if let Some(pt) = progress_token {
            emit_progress(&out, pt, 0, None);
        }

        // Open the database
        let db = match shiotsuchi_core::db::NoteDatabase::open(&d_path) {
            Ok(db) => db,
            Err(e) => {
                log::error!("Rebuild: failed to open DB {}: {}", d_path.display(), e);
                return;
            }
        };

        // Get tokenizer
        let tokenizer = match shiotsuchi_core::tokenizer::get_tokenizer() {
            Ok(t) => t,
            Err(_) => {
                log::error!("Rebuild: no tokenizer model available — cannot index without one");
                if let Some(pt) = progress_token {
                    emit_progress(&out, pt, 0, Some(1));
                }
                return;
            }
        };

        // Build IndexConfig (defaults: .md/.markdown, exclude node_modules, auto-exclude hidden)
        let config = shiotsuchi_core::models::IndexConfig {
            notes_dir: n_dir,
            ..Default::default()
        };

        // Set up progress callback
        let progress: Option<shiotsuchi_core::indexer::IndexProgress> = progress_token.map(|pt| {
            Box::new(move |current: usize, total: usize| {
                emit_progress(&out, pt, current as u64, Some(total as u64));
            }) as shiotsuchi_core::indexer::IndexProgress
        });

        // Run the indexer (no embedder = FTS-only)
        match shiotsuchi_core::indexer::index_directory(&db, &tokenizer, &config, None, progress) {
            Ok((results, _invalid)) => {
                let inserted = results
                    .iter()
                    .filter(|(_, r)| matches!(r, shiotsuchi_core::IndexResult::Inserted))
                    .count();
                let updated = results
                    .iter()
                    .filter(|(_, r)| matches!(r, shiotsuchi_core::IndexResult::Updated))
                    .count();
                let skipped = results
                    .iter()
                    .filter(|(_, r)| matches!(r, shiotsuchi_core::IndexResult::Skipped))
                    .count();
                let errors = results.len() - inserted - updated - skipped;
                log::info!(
                    "Rebuild complete: {} inserted, {} updated, {} skipped, {} errors",
                    inserted,
                    updated,
                    skipped,
                    errors
                );
            }
            Err(e) => {
                log::error!("Rebuild failed: {}", e);
            }
        }
    });
}

/// Helper: serialize a progress notification and write it to stdout.
fn emit_progress(stdout: &Arc<Mutex<io::Stdout>>, progress_token: u64, progress: u64, total: Option<u64>) {
    let notif = McpNotification::progress(progress_token, progress, total);
    if let Ok(line) = serde_json::to_string(&notif) {
        if let Ok(mut locked) = stdout.lock() {
            let _ = writeln!(&*locked, "{}", line);
            let _ = locked.flush();
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let cli = Cli::parse();
    let cfg = match cli.config {
        Some(ref path) => McpConfig::load(path),
        None => McpConfig::load_default(),
    };

    let notes_dir = resolve_path_env("SHIOTSUCHI_NOTES_DIR", cfg.notes_dir);
    let db_path = resolve_path_env("SHIOTSUCHI_DB_PATH", cfg.db_path);

    if let Some(parent) = db_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| eprintln!("Warning: failed to create parent dir: {}", e));
        }
    }

    let stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));

    // Send notifications/initialized on startup
    {
        let notif = McpNotification::new("notifications/initialized", serde_json::Value::Null);
        let mut locked = stdout.lock().unwrap();
        writeln!(&*locked, "{}", serde_json::to_string(&notif).unwrap()).ok();
        locked.flush().ok();
    }

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) if !l.is_empty() => l,
            _ => continue,
        };

        let resp = match serde_json::from_str::<McpRequest>(&line) {
            Ok(req) => {
                // Special-case rebuild_index to spawn a background task
                if req.method == "tools/call" {
                    let params = req.params.clone().unwrap_or(serde_json::Value::Null);
                    let name = params["name"].as_str().unwrap_or("");
                    if name == "rebuild_index" {
                        let args = &params["arguments"];
                        let progress_token = params["_meta"]["progressToken"].as_u64();
                        spawn_rebuild(&notes_dir, &db_path, &stdout, args, progress_token);
                        McpResponse::success(
                            req.id,
                            json!({
                                "content": [{
                                    "type": "text",
                                    "text": "Rebuild started. Progress notifications will be sent via the MCP progress protocol."
                                }]
                            }),
                        )
                    } else {
                        dispatch(req, &notes_dir, &db_path)
                    }
                } else {
                    dispatch(req, &notes_dir, &db_path)
                }
            }
            Err(_) => McpResponse::error(0, -32700, "Parse error"),
        };

        let mut locked = stdout.lock().unwrap();
        writeln!(&*locked, "{}", serde_json::to_string(&resp).unwrap()).ok();
        locked.flush().ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ---------------------------------------------------------------------------
    // resolve_path_env tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_resolve_path_env_uses_env_var_when_set() {
        std::env::set_var("SHIOTSUCHI_TEST_ABSOLUTE", "/tmp/notes");
        let result = resolve_path_env("SHIOTSUCHI_TEST_ABSOLUTE", PathBuf::from("default"));
        assert_eq!(result, PathBuf::from("/tmp/notes"));
        std::env::remove_var("SHIOTSUCHI_TEST_ABSOLUTE");
    }

    #[test]
    fn test_resolve_path_env_falls_back_when_unset() {
        std::env::remove_var("SHIOTSUCHI_TEST_NONEXISTENT");
        let result = resolve_path_env("SHIOTSUCHI_TEST_NONEXISTENT", PathBuf::from("default"));
        assert_eq!(result, PathBuf::from("default"));
    }

    #[test]
    fn test_resolve_path_env_rejects_dotdot_traversal() {
        std::env::set_var("SHIOTSUCHI_TEST_DOTDOT", "../outside");
        let result = resolve_path_env("SHIOTSUCHI_TEST_DOTDOT", PathBuf::from("default"));
        assert_eq!(
            result,
            PathBuf::from("default"),
            "should fall back when .. detected"
        );
        std::env::remove_var("SHIOTSUCHI_TEST_DOTDOT");
    }

    #[test]
    fn test_resolve_path_env_rejects_multiple_dotdot_traversal() {
        std::env::set_var("SHIOTSUCHI_TEST_NESTED_DOTDOT", "../../etc/passwd");
        let result = resolve_path_env("SHIOTSUCHI_TEST_NESTED_DOTDOT", PathBuf::from("default"));
        assert_eq!(
            result,
            PathBuf::from("default"),
            "should fall back when .. detected"
        );
        std::env::remove_var("SHIOTSUCHI_TEST_NESTED_DOTDOT");
    }

    #[test]
    fn test_resolve_path_env_accepts_relative_path_without_dotdot() {
        std::env::set_var("SHIOTSUCHI_TEST_RELATIVE", "notes");
        let result = resolve_path_env("SHIOTSUCHI_TEST_RELATIVE", PathBuf::from("default"));
        assert_eq!(result, PathBuf::from("notes"));
        std::env::remove_var("SHIOTSUCHI_TEST_RELATIVE");
    }

    #[test]
    fn test_resolve_path_env_accepts_absolute_path_with_dotdot() {
        // Absolute paths with .. are allowed (e.g., /home/user/../config)
        std::env::set_var("SHIOTSUCHI_TEST_ABSOLUTE_DOTDOT", "/home/user/../config");
        let result = resolve_path_env("SHIOTSUCHI_TEST_ABSOLUTE_DOTDOT", PathBuf::from("default"));
        assert_eq!(result, PathBuf::from("/home/user/../config"));
        std::env::remove_var("SHIOTSUCHI_TEST_ABSOLUTE_DOTDOT");
    }

    #[test]
    fn test_resolve_path_env_falls_back_on_empty_var() {
        std::env::set_var("SHIOTSUCHI_TEST_EMPTY", "");
        let result = resolve_path_env("SHIOTSUCHI_TEST_EMPTY", PathBuf::from("default"));
        assert_eq!(result, PathBuf::from("default"));
        std::env::remove_var("SHIOTSUCHI_TEST_EMPTY");
    }

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
        assert!(json.contains("search_local_notes"));
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
