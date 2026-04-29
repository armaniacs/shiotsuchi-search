use std::process::Command;

fn shiotsuchi_bin() -> std::path::PathBuf {
    env!("CARGO_BIN_EXE_shiotsuchi").into()
}

#[test]
fn test_version_contains_tagline() {
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

#[test]
fn test_dive_missing_db_shows_helpful_error() {
    let out = Command::new(shiotsuchi_bin())
        .args([
            "--notes-dir",
            "/tmp",
            "--db-path",
            "/tmp/nonexistent_shiotsuchi_db_test.sqlite3",
            "dive",
            "test",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("chart") || stderr.contains("index"),
        "expected helpful error mentioning 'chart', got: {}",
        stderr
    );
}
