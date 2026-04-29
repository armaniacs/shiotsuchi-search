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
