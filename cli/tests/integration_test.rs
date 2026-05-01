use std::fs;
use std::process::Command;
use tempfile::TempDir;

fn shiotsuchi_bin() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_shiotsuchi").into()
}

fn model_path() -> String {
    std::env::var("SHIOTSUCHI_MODEL_PATH")
        .unwrap_or_else(|_| "models/bccwj-suw+unidic_pos+kana.model.zst".to_string())
}

fn run(args: &[&str], temp: &TempDir, db: &std::path::Path) -> std::process::Output {
    Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model_path())
        .args(["--notes-dir", temp.path().to_str().unwrap(), "--db-path", db.to_str().unwrap()])
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn test_chart_then_dive() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("test.db");
    fs::write(
        temp.path().join("note.md"),
        "# Hello\n\nThis is a test note.",
    )
    .unwrap();

    let chart = Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model_path())
        .args([
            "--notes-dir",
            temp.path().to_str().unwrap(),
            "--db-path",
            db.to_str().unwrap(),
            "chart",
            "--quiet",
        ])
        .output()
        .unwrap();
    assert!(chart.status.success(), "chart failed: {:?}", chart);

    let dive = Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model_path())
        .args([
            "--notes-dir",
            temp.path().to_str().unwrap(),
            "--db-path",
            db.to_str().unwrap(),
            "dive",
            "test note",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(dive.status.success());
    let out = String::from_utf8_lossy(&dive.stdout);
    assert!(out.contains("note.md"), "expected note.md in output: {}", out);
}

// DBディレクトリが存在しなくても chart が自動作成すること
#[test]
fn test_chart_creates_db_dir_automatically() {
    let temp = TempDir::new().unwrap();
    let nested_db = temp.path().join("subdir").join("deep").join("db.sqlite3");
    fs::write(temp.path().join("note.md"), "# Note\n\nContent.").unwrap();

    let out = run(&["chart", "--quiet"], &temp, &nested_db);
    assert!(out.status.success(), "chart failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(nested_db.exists(), "DB file should have been created");
}

// log コマンドがインデックス済みファイル一覧と日時を表示すること
#[test]
fn test_log_shows_indexed_files() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("test.db");
    fs::write(temp.path().join("alpha.md"), "# Alpha\n\nAlpha content.").unwrap();
    fs::write(temp.path().join("beta.md"), "# Beta\n\nBeta content.").unwrap();

    let chart = run(&["chart", "--quiet"], &temp, &db);
    assert!(chart.status.success());

    let log = run(&["log"], &temp, &db);
    assert!(log.status.success(), "log failed: {}", String::from_utf8_lossy(&log.stderr));
    let out = String::from_utf8_lossy(&log.stdout);
    assert!(out.contains("alpha.md"), "expected alpha.md in log: {}", out);
    assert!(out.contains("beta.md"), "expected beta.md in log: {}", out);
    // 日時が YYYY-MM-DD 形式で含まれること
    assert!(out.contains("20"), "expected year in timestamp: {}", out);
    assert!(out.contains(':'), "expected time separator in timestamp: {}", out);
    assert!(out.contains('Z'), "expected UTC marker in timestamp: {}", out);
    assert!(out.contains("Total: 2 notes"), "expected total count: {}", out);
}

// log コマンドがインデックス未実施の場合に適切なメッセージを出すこと
#[test]
fn test_log_empty_db_shows_message() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("test.db");

    let chart = run(&["chart", "--quiet"], &temp, &db);
    assert!(chart.status.success());

    let log = run(&["log"], &temp, &db);
    assert!(log.status.success());
    let out = String::from_utf8_lossy(&log.stdout);
    assert!(out.contains("No notes") || out.contains("0"), "expected empty message: {}", out);
}

// tide の Last indexed が人間可読な日時形式で表示されること
#[test]
fn test_tide_shows_human_readable_timestamp() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("test.db");
    fs::write(temp.path().join("note.md"), "# Note\n\nContent.").unwrap();

    let chart = run(&["chart", "--quiet"], &temp, &db);
    assert!(chart.status.success());

    let tide = run(&["tide"], &temp, &db);
    assert!(tide.status.success());
    let out = String::from_utf8_lossy(&tide.stdout);
    // UNIX timestamp の生値（10桁の数字のみ）ではなく YYYY-MM-DD を含むこと
    assert!(out.contains("20"), "expected year in last_indexed: {}", out);
    assert!(out.contains(':'), "expected HH:MM:SS in last_indexed: {}", out);
    assert!(out.contains('Z'), "expected UTC marker in last_indexed: {}", out);
}

// 日本語クエリが正しく検索できること
#[test]
fn test_dive_japanese_query() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("test.db");
    fs::write(
        temp.path().join("meeting.md"),
        "# 会議メモ\n\nプロジェクト計画について議論した。",
    )
    .unwrap();
    fs::write(
        temp.path().join("other.md"),
        "# その他\n\nまったく関係ない内容。",
    )
    .unwrap();

    let chart = run(&["chart", "--quiet"], &temp, &db);
    assert!(chart.status.success());

    let dive = run(&["dive", "プロジェクト", "--json"], &temp, &db);
    assert!(dive.status.success());
    let out = String::from_utf8_lossy(&dive.stdout);
    assert!(out.contains("meeting.md"), "expected meeting.md in results: {}", out);
    assert!(!out.contains("other.md"), "other.md should not match: {}", out);
}

#[test]
fn test_tide_after_chart() {
    let temp = TempDir::new().unwrap();
    let db = temp.path().join("test.db");
    fs::write(temp.path().join("a.md"), "# A\n\nContent A.").unwrap();
    fs::write(temp.path().join("b.md"), "# B\n\nContent B.").unwrap();

    let chart = Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model_path())
        .args([
            "--notes-dir",
            temp.path().to_str().unwrap(),
            "--db-path",
            db.to_str().unwrap(),
            "chart",
            "--quiet",
        ])
        .output()
        .unwrap();
    assert!(chart.status.success());

    let tide = Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model_path())
        .args([
            "--db-path",
            db.to_str().unwrap(),
            "tide",
        ])
        .output()
        .unwrap();
    assert!(tide.status.success());
    let out = String::from_utf8_lossy(&tide.stdout);
    assert!(out.contains("Total notes"), "expected stats in output: {}", out);
    assert!(out.contains("2"), "expected 2 notes: {}", out);
}
