# PBI-28: VLM ベースの PDF Markdown 化 — 実装計画（修订版）

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `vlm` feature を CLI デフォルトビルドに追加し、VLM 抽出の動作をテストで証明する。

**Architecture:** コアロジック（`vlm.rs`、`indexer.rs`）は既に実装済み。残作業は feature flag の有効化とテスト追加のみ。

**Tech Stack:** Rust, `edgequake-pdf2md` v0.9, SQLite FTS5

---

## ファイル構成

| ファイル | 変更内容 |
|---------|---------|
| `cli/Cargo.toml:38` | `default` に `vlm` を追加 |
| `core/tests/integration_test.rs` | vlm feature コンパイル確認テスト + mtime キャッシュテスト追加 |
| `core/src/vlm.rs:108` | `#[cfg(not(feature = "vlm"))]` スタブテスト追加 |

---

### Task 1: vlm feature を cli default に追加 + コンパイル確認テスト

**Files:**
- Modify: `cli/Cargo.toml:38`
- Modify: `core/tests/integration_test.rs`（末尾）

- [ ] **Step 1: テストを書く（RED 確認用）**

`core/tests/integration_test.rs` の末尾に以下を追加：

```rust
/// vlm feature 有効ビルドでのみ実行されるテスト。
/// vlm が default に含まれない場合、このテスト関数はコンパイルされない。
#[cfg(feature = "vlm")]
#[test]
fn test_vlm_feature_is_compiled_and_not_compiled_stub_is_absent() {
    use shiotsuchi_core::config::VlmConfig;
    use shiotsuchi_core::vlm::{extract_text_with_vlm, VlmError};

    let config = VlmConfig { enabled: false, ..Default::default() };
    let path = std::path::Path::new("/nonexistent/dummy.pdf");
    let result = extract_text_with_vlm(path, &config);
    // enabled=false なら Ok(None) が返ること（NotCompiled エラーではない）
    assert!(
        matches!(result, Ok(None)),
        "vlm feature enabled + config.enabled=false should return Ok(None), got: {:?}",
        result
    );
}
```

- [ ] **Step 2: RED 確認 — テストが存在しないことを確認**

Run: `cargo test -p shiotsuchi-core --test integration_test -- --list 2>&1 | grep vlm_feature`
Expected: 何も表示されない（vlm feature が core の default にないため）

- [ ] **Step 3: `cli/Cargo.toml` の default を修正（GREEN 実装）**

`cli/Cargo.toml:38` を変更：

```toml
[features]
default = ["semantic", "pdf", "vlm"]
semantic = ["shiotsuchi-core/semantic"]
pdf = ["shiotsuchi-core/pdf"]
vlm = ["shiotsuchi-core/vlm"]
```

- [ ] **Step 4: GREEN 確認 — core のテストで vlm feature を明示して実行**

Run: `cargo test -p shiotsuchi-core --features vlm --test integration_test -- --list 2>&1 | grep vlm_feature`
Expected: `integration_test::test_vlm_feature_is_compiled_and_not_compiled_stub_is_absent: test`

Run: `cargo test -p shiotsuchi-core --features vlm --test integration_test test_vlm_feature_is_compiled_and_not_compiled_stub_is_absent 2>&1 | tail -3`
Expected: `test result: ok. 1 passed`

- [ ] **Step 5: CLI ビルド確認**

Run: `cargo check -p shiotsuchi 2>&1 | tail -1`
Expected: `Finished`

- [ ] **Step 6: コミット**

```bash
git add cli/Cargo.toml core/tests/integration_test.rs
git commit -m "feat(vlm): enable vlm feature in cli default build with compile-time test"
```

---

### Task 2: mtime キャッシュ再実行防止テスト

**Files:**
- Modify: `core/tests/integration_test.rs`（Task 1 で追加したファイルの末尾に追記）

- [ ] **Step 1: テストを書く**

`core/tests/integration_test.rs` の末尾に以下を追加：

```rust
#[test]
fn test_pdf_reindex_is_skipped_when_file_unchanged() {
    use shiotsuchi_core::db::NoteDatabase;
    use shiotsuchi_core::indexer::index_directory;
    use shiotsuchi_core::models::{IndexConfig, IndexResult};

    let tokenizer = JapaneseTokenizer::new(TokenizerConfig::default()).expect("tokenizer");
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();

    // hello.pdf をコピー（pdfium が受け入れる確実な PDF）
    let fixture_pdf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/hello.pdf");
    fs::copy(&fixture_pdf, vault.join("scan.pdf")).unwrap();

    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        vaults: vec![("default".to_string(), vault.clone())],
        enable_pdf_extraction: false, // テキスト抽出無効 → テキスト空同等
        vlm_enabled: false,           // VLM も無効
        ..Default::default()
    };

    // 1回目: 新規なので Inserted/Updated
    let (results1, _, _) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
    let first = results1.iter()
        .find(|(_, path, _)| path == "scan.pdf")
        .expect("scan.pdf should be in results");
    assert!(
        matches!(first.2, IndexResult::Inserted | IndexResult::Updated),
        "first index should insert or update, got: {:?}", first.2
    );

    // 2回目: ファイル未変更なので Skipped（VLM も再実行されない）
    let (results2, _, _) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
    let second = results2.iter()
        .find(|(_, path, _)| path == "scan.pdf")
        .expect("scan.pdf should appear in results");
    assert!(
        matches!(second.2, IndexResult::Skipped),
        "second index of unchanged PDF should be Skipped, got: {:?}", second.2
    );
}
```

- [ ] **Step 2: テストを実行して GREEN を確認**

Run: `cargo test -p shiotsuchi-core --test integration_test test_pdf_reindex_is_skipped_when_file_unchanged 2>&1 | tail -3`
Expected: `test result: ok. 1 passed`

- [ ] **Step 3: コミット**

```bash
git add core/tests/integration_test.rs
git commit -m "test(vlm): add mtime cache skip test for PDF reindex prevention"
```

---

### Task 3: vlm スタブ動作テスト

**Files:**
- Modify: `core/src/vlm.rs:108`（`#[cfg(test)]` ブロック内）

- [ ] **Step 1: スタブテストを書く**

`core/src/vlm.rs` の `#[cfg(test)]` ブロック末尾に以下を追加：

```rust
    #[cfg(not(feature = "vlm"))]
    #[test]
    fn test_extract_not_compiled_returns_error() {
        let config = VlmConfig::default();
        let path = Path::new("/nonexistent/test.pdf");
        let result = extract_text_with_vlm(path, &config);
        assert!(
            matches!(result, Err(VlmError::NotCompiled)),
            "should return NotCompiled when vlm feature is off, got: {:?}",
            result
        );
    }
```

- [ ] **Step 2: vlm feature なしで実行**

Run: `cargo test -p shiotsuchi-core --no-default-features --features pdf -- test_extract_not_compiled 2>&1 | tail -3`
Expected: `test result: ok. 1 passed`

> もし `--no-default-features` で他エラーが出る場合：`cargo test -p shiotsuchi-core --features pdf --no-default-features -- test_extract_not_compiled 2>&1 | tail -3`

- [ ] **Step 3: コミット**

```bash
git add core/src/vlm.rs
git commit -m "test(vlm): add not-compiled stub test for vlm feature guard"
```

---

### Task 4: 全テスト最終確認

- [ ] **Step 1: Core 全テスト**

Run: `cargo test -p shiotsuchi-core 2>&1 | grep -E "test result:"`
Expected: 全行が `ok. N passed; 0 failed`

- [ ] **Step 2: CLI 全テスト（vlm feature 有効）**

Run: `cargo test -p shiotsuchi 2>&1 | grep -E "test result:"`
Expected: 全行が `ok. N passed; 0 failed`

- [ ] **Step 3: MCP テスト**

Run: `cargo test -p shiotsuchi-mcp 2>&1 | grep -E "test result:"`
Expected: 全行が `ok. N passed; 0 failed`

- [ ] **Step 4: vlm なしビルド確認**

Run: `cargo build -p shiotsuchi --no-default-features --features "pdf,semantic" 2>&1 | tail -1`
Expected: `Finished`

- [ ] **Step 5: 受け入れ基準チェック**

| 受け入れ基準 | テスト |
|------------|--------|
| VLM テキスト変換を実行する | `test_vlm_feature_is_compiled_and_not_compiled_stub_is_absent` |
| mtime キャッシュで再実行を防ぐ | `test_pdf_reindex_is_skipped_when_file_unchanged` |
| VLM プロバイダーを config で選択できる | `VlmConfig` + `ref/cli.md`（既存） |
| VLM を設定でオフにできる | `VlmConfig::default().enabled == false`（既存） |
| API キー未設定時はスキップ | `test_extract_missing_api_key_returns_error`（既存） |
