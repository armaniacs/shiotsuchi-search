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
fn test_dive_on_unreadable_db_shows_helpful_error() {
    let model = std::env::var("SHIOTSUCHI_MODEL_PATH")
        .unwrap_or_else(|_| "models/bccwj-suw+unidic_pos+kana.model.zst".to_string());
    // ディレクトリをDBパスとして渡すと open に失敗する
    let out = Command::new(shiotsuchi_bin())
        .env("SHIOTSUCHI_MODEL_PATH", model)
        .args([
            "--notes-dir", "/tmp",
            "--db-path", "/tmp",   // ディレクトリはSQLiteで開けない
            "dive", "test",
        ])
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("chart") || stderr.contains("index") || stderr.contains("Error"),
        "expected error output, got: {}",
        stderr
    );
    assert!(!out.status.success(), "expected non-zero exit");
}
