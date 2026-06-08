mod handler;
mod protocol;
mod tools;

use clap::Parser;
use protocol::{McpNotification, McpRequest, McpResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;
use shiotsuchi_core::config::{DatabaseConfig, VaultEntry};
use shiotsuchi_core::db::NoteDatabase;
use shiotsuchi_core::sensitive::SensitiveDataConfig;
use shiotsuchi_core::paths::default_db_path as core_default_db_path;
use std::{
    collections::HashMap,
    ffi::OsString,
    io::{self, BufRead},
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
            // Reject any path with '..' traversal (both relative and absolute).
            // Absolute paths like /home/user/../config could bypass vault boundaries
            // when combined with symbolic links.
            if p.to_string_lossy().contains("..") {
                tracing::warn!(
                    "{} contains '..' (path traversal), using config default",
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
    database: DatabaseConfig,
    vaults: HashMap<String, VaultEntry>,
    vault: Option<VaultEntry>,       // legacy
    notes_dir: PathBuf,              // legacy flat field (old config format)
    db_path: PathBuf,                // legacy flat field (old config format)
    #[serde(default = "default_backlink_scoring")]
    backlink_scoring: bool,
    /// Sensitive data detection and masking configuration.
    #[serde(default)]
    sensitive_data: SensitiveDataConfig,
}

fn default_backlink_scoring() -> bool {
    true
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            vaults: HashMap::new(),
            vault: None,
            notes_dir: PathBuf::from("."),
            db_path: core_default_db_path(),
            backlink_scoring: true,
            sensitive_data: SensitiveDataConfig::default(),
        }
    }
}

impl McpConfig {
    /// Convert this MCP-specific config into the core `ShiotsuchiConfig`.
    ///
    /// This single bridge point replaces the previously duplicated `resolved_vaults`,
    /// `resolved_db_path`, and `backlink_scoring` access.  All new config fields
    /// added to `ShiotsuchiConfig` only need to be wired once here.
    fn to_core_config(&self) -> shiotsuchi_core::config::ShiotsuchiConfig {
        let mut cfg = shiotsuchi_core::config::ShiotsuchiConfig {
            database: self.database.clone(),
            vaults: self.vaults.clone(),
            vault: self.vault.clone(),
            sensitive_data: self.sensitive_data.clone(),
            indexing: shiotsuchi_core::config::IndexingConfig {
                backlink_scoring: self.backlink_scoring,
                ..Default::default()
            },
            ..Default::default()
        };

        // Backward-compat: flat `notes_dir` field (old MCP config format)
        let default_notes = PathBuf::from(".");
        if self.notes_dir != default_notes {
            cfg.vaults
                .entry("default".to_string())
                .or_insert_with(|| shiotsuchi_core::config::VaultEntry {
                    notes_dir: Some(self.notes_dir.clone()),
                    db_path: None,
                });
        }

        // Backward-compat: flat `db_path` field
        let default_db = core_default_db_path();
        if self.db_path != default_db && cfg.database.db_path.is_none() {
            cfg.database.db_path = Some(self.db_path.clone());
        }

        cfg
    }

    fn load(path: &Path) -> Self {
        match config::Config::builder()
            .add_source(config::File::from(path))
            .build()
            .and_then(|c| c.try_deserialize::<McpConfig>())
        {
            Ok(cfg) => cfg,
            Err(e) => {
                tracing::warn!(
                    "failed to load config from {}: {}. Using defaults.",
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

pub fn dispatch(req: McpRequest, vaults: &[(String, PathBuf)], db: &Mutex<NoteDatabase>, backlink_scoring: bool, sensitive_config: &SensitiveDataConfig) -> McpResponse {
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
            match handler::call_tool(name, args, vaults, db, backlink_scoring, sensitive_config) {
                Ok(result) => McpResponse::success(req.id, result),
                Err(e) => {
                    tracing::error!(tool = %name, error = %e, "MCP tool execution failed");
                    McpResponse::error(req.id, -32000, "Internal tool execution error")
                }
            }
        }
        "ping" => McpResponse::success(req.id, serde_json::json!({})),
        _ => McpResponse::error(req.id, -32601, "Method not found"),
    }
}

/// Spawn a background task that calls `shiotsuchi_core::indexer::index_directory`
/// and sends MCP `notifications/progress` notifications on stdout.
fn spawn_rebuild(
    config: shiotsuchi_core::models::IndexConfig,
    db_path: &Path,
    stdout: &Arc<Mutex<dyn io::Write + Send>>,
    _args: &serde_json::Value,
    progress_token: Option<u64>,
) {
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
                tracing::error!("Rebuild: failed to open DB {}: {}", d_path.display(), e);
                return;
            }
        };

        // Get tokenizer
        let tokenizer = match shiotsuchi_core::tokenizer::get_tokenizer() {
            Ok(t) => t,
            Err(_) => {
                tracing::error!("Rebuild: no tokenizer model available — cannot index without one");
                if let Some(pt) = progress_token {
                    emit_progress(&out, pt, 0, Some(1));
                }
                return;
            }
        };

        // Set up progress callback
        let progress: Option<shiotsuchi_core::indexer::IndexProgress> = progress_token.map(|pt| {
            Box::new(move |current: usize, _total: Option<usize>| {
                emit_progress(&out, pt, current as u64, None);
            }) as shiotsuchi_core::indexer::IndexProgress
        });

        // Run the indexer (no embedder = FTS-only)
        match shiotsuchi_core::indexer::index_directory(&db, &tokenizer, &config, None, progress) {
            Ok((results, _invalid, _excluded)) => {
                let inserted = results
                    .iter()
                    .filter(|(_, _, r)| matches!(r, shiotsuchi_core::IndexResult::Inserted))
                    .count();
                let updated = results
                    .iter()
                    .filter(|(_, _, r)| matches!(r, shiotsuchi_core::IndexResult::Updated))
                    .count();
                let skipped = results
                    .iter()
                    .filter(|(_, _, r)| matches!(r, shiotsuchi_core::IndexResult::Skipped))
                    .count();
                let errors = results.len() - inserted - updated - skipped;
                tracing::info!(
                    "Rebuild complete: {} inserted, {} updated, {} skipped, {} errors",
                    inserted,
                    updated,
                    skipped,
                    errors
                );
            }
            Err(e) => {
                tracing::error!("Rebuild failed: {}", e);
            }
        }
    });
}

/// Helper: serialize a progress notification and write it to stdout.
fn emit_progress(stdout: &Arc<Mutex<dyn io::Write + Send>>, progress_token: u64, progress: u64, total: Option<u64>) {
    let notif = McpNotification::progress(progress_token, progress, total);
    if let Ok(line) = serde_json::to_string(&notif) {
        if let Ok(mut locked) = stdout.lock() {
            let _ = writeln!(&mut *locked, "{}", line);
            let _ = locked.flush();
        }
    }
}

#[tokio::main]
async fn main() {
    tracing_log::LogTracer::init().ok();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let mcp_cfg = match cli.config {
        Some(ref path) => McpConfig::load(path),
        None => McpConfig::load_default(),
    };
    let cfg = mcp_cfg.to_core_config();

    let mut vaults = cfg.resolved_vaults();
    let notes_dir_override = resolve_path_env("SHIOTSUCHI_NOTES_DIR", PathBuf::new());
    if notes_dir_override != PathBuf::new() {
        if let Some(first) = vaults.first_mut() {
            first.1 = notes_dir_override;
        }
    }
    let db_path = resolve_path_env("SHIOTSUCHI_DB_PATH", cfg.resolved_db_path());

    if let Some(parent) = db_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)
                .unwrap_or_else(|e| tracing::warn!("failed to create parent dir: {}", e));
        }
    }

    let sensitive_config = cfg.sensitive_data.clone();
    let stdin = io::stdin();
    let stdout: Arc<Mutex<dyn io::Write + Send>> = Arc::new(Mutex::new(io::stdout()));
    let backlink_scoring = cfg.indexing.backlink_scoring;

    // Open the database once at startup. All 3 shared handlers (search,
    // get_surrounding_context, index_status) use this pooled instance.
    // `spawn_rebuild` still opens its own NoteDatabase for background indexing.
    let db = match NoteDatabase::open(&db_path) {
        Ok(db) => Arc::new(Mutex::new(db)),
        Err(e) => {
            eprintln!(
                "Error: Failed to open database at {}:\n  {}\n\n\
                 Make sure the database path is accessible and writable.\n\
                 To create a new database, run: shiotsuchi scan --notes-dir <PATH>\n\
                 Or set SHIOTSUCHI_DB_PATH to an existing or creatable path.",
                db_path.display(),
                e
            );
            std::process::exit(1);
        }
    };

    // Send notifications/initialized on startup
    {
        let notif = McpNotification::new("notifications/initialized", serde_json::Value::Null);
        let mut locked = stdout.lock().unwrap();
        writeln!(&mut *locked, "{}", serde_json::to_string(&notif).unwrap()).ok();
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
                        if !handler::check_rebuild_rate_limit() {
                            McpResponse::success(req.id, handler::rate_limit_error())
                        } else {
                            let args = &params["arguments"];
                            let progress_token = params["_meta"]["progressToken"].as_u64();
                            let rebuild_config = shiotsuchi_core::models::IndexConfig::from_cli_configs(
                                vaults.clone(),
                                &cfg.indexing,
                                &cfg.vlm,
                            );
                            spawn_rebuild(rebuild_config, &db_path, &stdout, args, progress_token);
                            McpResponse::success(
                                req.id,
                                json!({
                                    "content": [{
                                        "type": "text",
                                        "text": "Rebuild started. Progress notifications will be sent via the MCP progress protocol."
                                    }]
                                }),
                            )
                        }
                    } else {
                        dispatch(req, &vaults, &db, backlink_scoring, &sensitive_config)
                    }
                } else {
                    dispatch(req, &vaults, &db, backlink_scoring, &sensitive_config)
                }
            }
            Err(_) => McpResponse::error(0, -32700, "Parse error"),
        };

        let mut locked = stdout.lock().unwrap();
        writeln!(&mut *locked, "{}", serde_json::to_string(&resp).unwrap()).ok();
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
    fn test_resolve_path_env_rejects_absolute_path_with_dotdot() {
        // Absolute paths with .. must also be rejected for security
        std::env::set_var("SHIOTSUCHI_TEST_ABSOLUTE_DOTDOT", "/home/user/../config");
        let result = resolve_path_env("SHIOTSUCHI_TEST_ABSOLUTE_DOTDOT", PathBuf::from("default"));
        assert_eq!(result, PathBuf::from("default"), "absolute path with .. should be rejected");
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
        let mcp_cfg = McpConfig::load(&path);
        assert_eq!(mcp_cfg.notes_dir, PathBuf::from("/tmp/my-notes"));
        let core_cfg = mcp_cfg.to_core_config();
        let vaults = core_cfg.resolved_vaults();
        assert_eq!(vaults, vec![("default".to_string(), PathBuf::from("/tmp/my-notes"))]);
        assert_eq!(core_cfg.resolved_db_path(), PathBuf::from("/tmp/my-notes/search.db"));
    }

    #[test]
    fn test_config_defaults_when_file_missing() {
        let mcp_cfg = McpConfig::load(Path::new("/nonexistent/path/config.toml"));
        let default = McpConfig::default();
        assert_eq!(mcp_cfg.notes_dir, default.notes_dir);
        assert_eq!(mcp_cfg.db_path, default.db_path);
        let core_cfg = mcp_cfg.to_core_config();
        let default_core = default.to_core_config();
        assert_eq!(core_cfg.resolved_vaults(), default_core.resolved_vaults());
        assert_eq!(core_cfg.resolved_db_path(), default_core.resolved_db_path());
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
        let mcp_cfg = McpConfig::load(&path);
        assert_eq!(mcp_cfg.notes_dir, PathBuf::from("/tmp/partial-notes"));
        let core_cfg = mcp_cfg.to_core_config();
        assert_eq!(core_cfg.resolved_db_path(), McpConfig::default().db_path);
        let vaults = core_cfg.resolved_vaults();
        assert_eq!(vaults, vec![("default".to_string(), PathBuf::from("/tmp/partial-notes"))]);
    }

    #[test]
    fn test_config_invalid_toml_falls_back_to_defaults() {
        let tmp = TempDir::new().unwrap();
        let path = write_config(&tmp, "this is not valid toml ][[[");
        let mcp_cfg = McpConfig::load(&path);
        let default = McpConfig::default();
        assert_eq!(mcp_cfg.notes_dir, default.notes_dir);
        assert_eq!(mcp_cfg.db_path, default.db_path);
        let core_cfg = mcp_cfg.to_core_config();
        let default_core = default.to_core_config();
        assert_eq!(core_cfg.resolved_vaults(), default_core.resolved_vaults());
        assert_eq!(core_cfg.resolved_db_path(), default_core.resolved_db_path());
    }

    #[test]
    fn test_config_load_default_returns_defaults_when_no_file() {
        let path = McpConfig::default_config_path();
        if !path.exists() {
            let mcp_cfg = McpConfig::load_default();
            let default = McpConfig::default();
            assert_eq!(mcp_cfg.notes_dir, default.notes_dir);
            assert_eq!(mcp_cfg.db_path, default.db_path);
            let core_cfg = mcp_cfg.to_core_config();
            let default_core = default.to_core_config();
            assert_eq!(core_cfg.resolved_vaults(), default_core.resolved_vaults());
            assert_eq!(core_cfg.resolved_db_path(), default_core.resolved_db_path());
        }
    }

    #[test]
    fn test_dispatch_tools_list() {
        let vaults = vec![("default".to_string(), PathBuf::from("/tmp"))];
        let db = Mutex::new(NoteDatabase::open_in_memory().unwrap());
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(1),
            method: "tools/list".to_string(),
            params: None,
        };
        let resp = dispatch(
            req,
            &vaults,
            &db,
            true, &SensitiveDataConfig::default(),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("search_local_notes"));
    }

    #[test]
    fn test_dispatch_unknown_method() {
        let vaults = vec![("default".to_string(), PathBuf::from("/tmp"))];
        let db = Mutex::new(NoteDatabase::open_in_memory().unwrap());
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(2),
            method: "unknown".to_string(),
            params: None,
        };
        let resp = dispatch(
            req,
            &vaults,
            &db,
            true, &SensitiveDataConfig::default(),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
    }

    #[test]
    fn test_dispatch_initialize() {
        let vaults = vec![("default".to_string(), PathBuf::from("/tmp"))];
        let db = Mutex::new(NoteDatabase::open_in_memory().unwrap());
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(1),
            method: "initialize".to_string(),
            params: None,
        };
        let resp = dispatch(
            req,
            &vaults,
            &db,
            true, &SensitiveDataConfig::default(),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("protocolVersion"));
        assert!(json.contains("shiotsuchi-mcp"));
    }

    #[test]
    fn test_dispatch_ping() {
        let vaults = vec![("default".to_string(), PathBuf::from("/tmp"))];
        let db = Mutex::new(NoteDatabase::open_in_memory().unwrap());
        let req = crate::protocol::McpRequest {
            jsonrpc: "2.0".to_string(),
            id: serde_json::json!(3),
            method: "ping".to_string(),
            params: None,
        };
        let resp = dispatch(
            req,
            &vaults,
            &db,
            true, &SensitiveDataConfig::default(),
        );
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"result\""));
    }

    #[test]
    fn test_spawn_rebuild_indexes_vault() {
        use std::time::Duration;
        use tokio::runtime::Runtime;

        let temp = TempDir::new().unwrap();
        let vault = temp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();

        // Create a few markdown files
        std::fs::write(vault.join("note1.md"), "# Note 1\n\nContent for note one.").unwrap();
        std::fs::write(vault.join("note2.md"), "# Note 2\n\nContent for note two.").unwrap();

        let db_path = temp.path().join("test.db");

        // Set up model path so tokenizer can load
        let model_path = std::env::var("SHIOTSUCHI_MODEL_PATH")
            .unwrap_or_else(|_| {
                let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
                manifest_dir.parent()
                    .unwrap()
                    .join("models/bccwj-suw+unidic_pos+kana.model.zst")
                    .to_string_lossy()
                    .into_owned()
            });

        std::env::set_var("SHIOTSUCHI_MODEL_PATH", &model_path);

        // Clone for the closure
        let vaults = vec![("default".to_string(), vault.clone())];
        let db_path_clone = db_path.clone();

        // Need tokio runtime for spawn_rebuild (it calls tokio::spawn)
        let rt = Runtime::new().unwrap();
        rt.block_on(async move {
            // Wrap output as dyn Write
            let writer: Arc<Mutex<dyn io::Write + Send>> = Arc::new(Mutex::new(Vec::new()));

            let args = serde_json::json!({});
            let progress_token = Some(42u64);

            let config = shiotsuchi_core::models::IndexConfig::with_vaults(vaults);
            spawn_rebuild(config, &db_path_clone, &writer, &args, progress_token);

            // Wait for rebuild to complete (poll DB). The timeout is generous
            // because ONNX embedder model loading can take 60+ seconds.
            let deadline = std::time::Instant::now() + Duration::from_secs(120);
            let mut indexed = false;
            while std::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if db_path_clone.exists() {
                    if let Ok(db) = shiotsuchi_core::db::NoteDatabase::open(&db_path_clone) {
                        if let Ok(stats) = db.stats() {
                            if stats.total_files >= 2 {
                                indexed = true;
                                break;
                            }
                        }
                    }
                }
            }

            assert!(indexed, "rebuild should index 2 files within 30s");
        });
    }
}
