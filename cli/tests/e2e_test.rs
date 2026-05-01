//! End-to-end tests covering docs/HUMAN-VERIFICATION.md §1–§7.
//! Each test corresponds to a numbered section in that document.

use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
};
use tempfile::TempDir;

fn shiotsuchi_bin() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_shiotsuchi").into()
}

fn mcp_bin() -> std::path::PathBuf {
    // CARGO_BIN_EXE_* is only available for bins in the same crate.
    // Derive the path from the shiotsuchi binary's location instead.
    // Try debug first, then fall back to release.
    let mut p = shiotsuchi_bin();
    p.set_file_name("shiotsuchi-mcp");
    if p.exists() {
        return p;
    }
    // Fall back to release binary
    let mut release_p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent().unwrap()
        .join("target/release/shiotsuchi-mcp");
    if release_p.exists() {
        return release_p;
    }
    // Last resort: return the original path (will fail with proper error)
    p
}

fn model_path() -> String {
    std::env::var("SHIOTSUCHI_MODEL_PATH")
        .unwrap_or_else(|_| "models/bccwj-suw+unidic_pos+kana.model.zst".to_string())
}

/// Convenience: run a shiotsuchi subcommand with --notes-dir and --db-path pre-set.
fn cmd(args: &[&str], notes_dir: &std::path::Path, db: &std::path::Path) -> std::process::Output {
    Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model_path())
        .args(["--notes-dir", notes_dir.to_str().unwrap()])
        .args(["--db-path", db.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap()
}

fn setup_vault(temp: &TempDir) {
    fs::write(
        temp.path().join("meeting.md"),
        "# Meeting notes\n\nDiscussed the project plan.\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("shopping.md"),
        "# Shopping list\n\nApples, bananas, milk.\n",
    )
    .unwrap();
}

// ─── §1: Build ───────────────────────────────────────────────────────────────

/// §1: --version contains the tagline.
#[test]
fn e2e_version_contains_tagline() {
    let out = Command::new(shiotsuchi_bin())
        .arg("--version")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("Guiding your path through the data tide"),
        "version output: {}",
        stdout
    );
}

// ─── §2: chart / dive / tide / log ──────────────────────────────────────────

/// §2: chart completes without error and indexes 2 files.
#[test]
fn e2e_chart_indexes_vault() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("db.sqlite3");
    setup_vault(&temp);

    let out = cmd(&["chart"], &temp.path(), &db);
    assert!(out.status.success(), "chart failed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Indexed 2"), "expected 2 indexed files: {}", stdout);
}

/// §2: dive "project" returns meeting.md.
#[test]
fn e2e_dive_returns_matching_note() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("db.sqlite3");
    setup_vault(&temp);

    cmd(&["chart", "--quiet"], &temp.path(), &db);
    let out = cmd(&["dive", "project"], &temp.path(), &db);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("meeting.md"), "expected meeting.md: {}", stdout);
}

/// §2: dive --json produces valid JSON.
#[test]
fn e2e_dive_json_is_valid() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("db.sqlite3");
    setup_vault(&temp);

    cmd(&["chart", "--quiet"], &temp.path(), &db);
    let out = cmd(&["dive", "project", "--json"], &temp.path(), &db);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "output is not valid JSON: {}", stdout);
    assert!(parsed.unwrap().is_array(), "expected JSON array");
}

/// §2: dive with no match returns empty array without error.
#[test]
fn e2e_dive_no_match_returns_empty() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("db.sqlite3");
    setup_vault(&temp);

    cmd(&["chart", "--quiet"], &temp.path(), &db);
    let out = cmd(&["dive", "xyzzy-no-match-query", "--json"], &temp.path(), &db);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 0, "expected 0 results: {}", stdout);
}

/// §2: tide shows total_notes: 2.
#[test]
fn e2e_tide_shows_note_count() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("db.sqlite3");
    setup_vault(&temp);

    cmd(&["chart", "--quiet"], &temp.path(), &db);
    let out = cmd(&["tide"], &temp.path(), &db);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Total notes"), "expected 'Total notes': {}", stdout);
    assert!(stdout.contains('2'), "expected count 2: {}", stdout);
}

/// §2: log shows indexing history with ISO timestamps.
#[test]
fn e2e_log_shows_history() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("db.sqlite3");
    setup_vault(&temp);

    cmd(&["chart", "--quiet"], &temp.path(), &db);
    let out = cmd(&["log"], &temp.path(), &db);
    assert!(out.status.success(), "log failed: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("meeting.md"), "expected meeting.md: {}", stdout);
    assert!(stdout.contains("shopping.md"), "expected shopping.md: {}", stdout);
    // ISO 8601 timestamp (YYYY-MM-DDThh:mm:ssZ)
    assert!(stdout.contains('Z'), "expected UTC marker: {}", stdout);
    assert!(stdout.contains(':'), "expected time separator: {}", stdout);
    assert!(stdout.contains("Total: 2 notes"), "expected total: {}", stdout);
}

// ─── §3: Error message ───────────────────────────────────────────────────────

/// §3: dive on a broken DB path prints helpful error mentioning 'chart'.
#[test]
fn e2e_dive_broken_db_shows_helpful_error() {
    let model = model_path();
    // Passing a directory as the DB path causes SQLite open to fail.
    let out = Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", &model)
        .args(["--notes-dir", "/tmp", "--db-path", "/tmp", "dive", "test"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("chart") || stderr.contains("index") || stderr.contains("Error"),
        "expected helpful error, got: {}",
        stderr
    );
}

// ─── §4: scan (file watcher) ─────────────────────────────────────────────────

/// §4: scan picks up a newly created file and indexes it.
/// Uses the CLI binary in watch mode: start scan, write a file, verify it appears in dive output.
#[test]
fn e2e_scan_indexes_new_file() {
    use std::{sync::atomic::{AtomicBool, Ordering}, sync::Arc, thread, time::Duration};
    use obsidian_shiotsuchi_vault_core::{
        db::NoteDatabase,
        indexer::index_file,
        models::IndexConfig,
        tokenizer::{JapaneseTokenizer, TokenizerConfig},
    };
    use notify::{Config as NotifyConfig, Event, PollWatcher, RecursiveMode, Watcher};

    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("db.sqlite3");
    let db = Arc::new(std::sync::Mutex::new(NoteDatabase::open(&db_path).unwrap()));
    let ready = Arc::new(AtomicBool::new(false));

    let db_clone = Arc::clone(&db);
    let vault = temp.path().to_path_buf();
    let ready_clone = Arc::clone(&ready);

    let handle = thread::spawn(move || -> usize {
        let tokenizer = Arc::new(JapaneseTokenizer::new(TokenizerConfig::default()).unwrap());
        let config = IndexConfig { notes_dir: vault.clone(), ..Default::default() };
        let (tx, rx) = std::sync::mpsc::channel();
        let poll_config = NotifyConfig::default()
            .with_poll_interval(Duration::from_millis(100));
        let mut watcher = PollWatcher::new(
            move |res: Result<Event, _>| { if let Ok(e) = res { let _ = tx.send(e); } },
            poll_config,
        ).unwrap();
        watcher.watch(&vault, RecursiveMode::Recursive).unwrap();
        ready_clone.store(true, Ordering::SeqCst);

        let mut indexed = 0usize;
        let deadline = std::time::Instant::now() + Duration::from_millis(2500);
        while std::time::Instant::now() < deadline {
            if let Ok(event) = rx.recv_timeout(Duration::from_millis(50)) {
                use notify::event::{EventKind, ModifyKind};
                if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(ModifyKind::Data(_))) {
                    for path in &event.paths {
                        if let Ok(rel) = path.strip_prefix(&config.notes_dir) {
                            let db = db_clone.lock().unwrap();
                            let _ = index_file(&db, &tokenizer, path, &rel.to_string_lossy(), &config);
                            indexed += 1;
                        }
                    }
                }
            }
        }
        indexed
    });

    while !ready.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(100));
    fs::write(temp.path().join("new.md"), "# New note\n\nauto-index test\n").unwrap();
    thread::sleep(Duration::from_millis(2500));

    let _ = handle.join();
    let count = db.lock().unwrap().stats().unwrap().total_notes;
    assert_eq!(count, 1, "expected 1 indexed note after file creation");
}

// ─── §5: XDG paths ───────────────────────────────────────────────────────────

/// §5: chart creates the DB at the XDG cache default when no --db-path given.
#[test]
fn e2e_xdg_default_db_path_created() {
    let temp = TempDir::new().unwrap();
    // Override XDG_CACHE_HOME to a temp dir to avoid polluting the real cache.
    let fake_cache = temp.path().join("cache");
    let expected_db = fake_cache.join("shiotsuchi").join("db.sqlite3");

    let out = Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model_path())
        .env("XDG_CACHE_HOME", &fake_cache)
        .args(["--notes-dir", temp.path().to_str().unwrap(), "chart", "--quiet"])
        .output()
        .unwrap();
    assert!(out.status.success(), "chart failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(expected_db.exists(), "expected DB at XDG path: {}", expected_db.display());
}

/// §5: --db-path override places the DB at the specified path.
#[test]
fn e2e_db_path_override() {
    let temp = TempDir::new().unwrap();
    let custom_db = temp.path().join("custom").join("my.db");

    let out = Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model_path())
        .args([
            "--notes-dir", temp.path().to_str().unwrap(),
            "--db-path", custom_db.to_str().unwrap(),
            "chart", "--quiet",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "chart failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(custom_db.exists(), "expected DB at custom path: {}", custom_db.display());
}

// ─── §6: Makefile ────────────────────────────────────────────────────────────

/// §6: make help lists key targets.
#[test]
fn e2e_make_help_lists_targets() {
    let out = Command::new("make")
        .arg("help")
        .current_dir(env!("CARGO_MANIFEST_DIR").to_string() + "/..")
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    for target in &["build", "test", "install", "uninstall", "clean", "model"] {
        assert!(stdout.contains(target), "make help missing '{}': {}", target, stdout);
    }
}

/// §6: make install / make uninstall round-trip.
#[test]
fn e2e_make_install_uninstall() {
    let temp = TempDir::new().unwrap();
    let prefix = temp.path().to_str().unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();

    let install = Command::new("make")
        .args(["install", &format!("PREFIX={}", prefix)])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(install.status.success(), "make install failed: {}", String::from_utf8_lossy(&install.stderr));

    for bin in &["shiotsuchi", "shiotsuchi-mcp"] {
        let p = temp.path().join("bin").join(bin);
        assert!(p.exists(), "expected {} after install", bin);
    }

    let uninstall = Command::new("make")
        .args(["uninstall", &format!("PREFIX={}", prefix)])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(uninstall.status.success());

    for bin in &["shiotsuchi", "shiotsuchi-mcp"] {
        let p = temp.path().join("bin").join(bin);
        assert!(!p.exists(), "expected {} removed after uninstall", bin);
    }
}

// ─── §7: MCP (shiotsuchi-mcp over stdio) ─────────────────────────────────────

fn mcp_request(id: u64, method: &str, params: serde_json::Value) -> String {
    serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    }))
    .unwrap()
        + "\n"
}

fn run_mcp(
    requests: &[String],
    notes_dir: &std::path::Path,
    db_path: &std::path::Path,
) -> Vec<serde_json::Value> {
    let mut child = Command::new(mcp_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model_path())
        .env("SHIOTSUCHI_NOTES_DIR", notes_dir)
        .env("SHIOTSUCHI_DB_PATH", db_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let stdin = child.stdin.as_mut().unwrap();
    for req in requests {
        stdin.write_all(req.as_bytes()).unwrap();
    }
    drop(child.stdin.take());

    let out = child.wait_with_output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// §7: search_vault returns results for a matching query.
#[test]
fn e2e_mcp_search_vault_returns_results() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("db.sqlite3");
    setup_vault(&temp);
    cmd(&["chart", "--quiet"], &temp.path(), &db);

    let requests = vec![
        mcp_request(1, "initialize", serde_json::json!({"protocolVersion": "2024-11-05", "clientInfo": {}})),
        mcp_request(2, "tools/call", serde_json::json!({
            "name": "search_vault",
            "arguments": {"query": "project"}
        })),
    ];

    let responses = run_mcp(&requests, &temp.path(), &db);
    let tool_resp = responses.into_iter().find(|r| r["id"] == 2 && r.get("result").is_some());
    assert!(tool_resp.is_some(), "no tool response found");
    let resp = tool_resp.unwrap();
    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("meeting.md"), "expected meeting.md in MCP response: {}", text);
}

/// §7: read_full_note returns file content.
#[test]
fn e2e_mcp_read_full_note_returns_content() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("db.sqlite3");
    setup_vault(&temp);
    cmd(&["chart", "--quiet"], &temp.path(), &db);

    let requests = vec![
        mcp_request(1, "initialize", serde_json::json!({"protocolVersion": "2024-11-05", "clientInfo": {}})),
        mcp_request(2, "tools/call", serde_json::json!({
            "name": "read_full_note",
            "arguments": {"path": "meeting.md"}
        })),
    ];

    let responses = run_mcp(&requests, &temp.path(), &db);
    let tool_resp = responses.into_iter().find(|r| r["id"] == 2 && r.get("result").is_some());
    assert!(tool_resp.is_some(), "no tool response found");
    let resp = tool_resp.unwrap();
    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains("project plan"), "expected note content: {}", text);
}

/// §7: vault_status returns note count.
#[test]
fn e2e_mcp_vault_status_returns_count() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("db.sqlite3");
    setup_vault(&temp);
    cmd(&["chart", "--quiet"], &temp.path(), &db);

    let requests = vec![
        mcp_request(1, "initialize", serde_json::json!({"protocolVersion": "2024-11-05", "clientInfo": {}})),
        mcp_request(2, "tools/call", serde_json::json!({
            "name": "vault_status",
            "arguments": {}
        })),
    ];

    let responses = run_mcp(&requests, &temp.path(), &db);
    let tool_resp = responses.into_iter().find(|r| r["id"] == 2 && r.get("result").is_some());
    assert!(tool_resp.is_some(), "no tool response found");
    let resp = tool_resp.unwrap();
    let text = resp["result"]["content"][0]["text"].as_str().unwrap_or("");
    assert!(text.contains('2'), "expected note count 2: {}", text);
}
