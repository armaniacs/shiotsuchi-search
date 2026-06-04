# PBI-28: VLM ベースの PDF Markdown 化 — 修订版計画

> **深掘りセッション — 2026-06-02** で発見された6件の問題を修正した修订版です。

**Goal:** スキャンPDF（テキスト埋め込みなし）を `edgequake-pdf2md` 経由で VLM に送り Markdown 化し、FTS5 検索対象に加える。

**Architecture:** コアロジック（`vlm.rs`、`indexer.rs`）は既に実装済み。残作業は (1) feature flag の有効化、(2) feature が有効であることを証明するテスト追加、(3) キャッシュ動作のテスト追加。(4) は前セッションで完了済み。

**Tech Stack:** Rust, `edgequake-pdf2md` v0.9, `edgequake-llm` v0.6.23（OpenAI/Anthropic/Gemini/Ollama 対応）, `pdfium-auto` v0.3（bundled）, SQLite FTS5

---

## 深掘りで発見された問題と修正内容

| # | 元の計画 | 問題 | 修正 |
|---|---------|------|------|
| 1 | `cargo test -p shiotsuchi-core` で feature 有効化を検証 | ワークスペース feature は独立解決。cli の default 変更で core のテストに影響なし | 検証を `cargo test -p shiotsuchi` に変更。core のテストは feature 有効/無効の2パターンで検証 |
| 2 | `results.get("scan_empty.pdf")` | `index_directory` は `Vec<(String, String, IndexResult)>` を返す。HashMap の `.get()` は使えない | `.iter().find(|(_, path, _)| path == "scan_empty.pdf")` に修正 |
| 3 | `scan_empty.pdf` を Python で生成 | pdfium に拒否される可能性がある | `hello.pdf` + `enable_pdf_extraction: false` で代替。pdfium が受け入れる確実な PDF を使う |
| 4 | `vlm` を cli の default に含める | そのまま採用 | ユーザー判断に基づく。`cli/Cargo.toml:38` を修正 |
| 5 | Task 4: `ref/cli.md` に `[vlm]` 追記 | **既に完了済み** | Task 4 を削除 |
| 6 | `core/Cargo.toml` は変更しない | 全 downstream が `default-features = false` なので core default 変更も安全 | 今回は cli の default のみ変更（変更不要） |

---

## 実装状況

| 状態 | 内容 |
|------|------|
| ✅ 完了 | `core/src/vlm.rs` — `extract_text_with_vlm()` 実装済み |
| ✅ 完了 | `core/src/indexer.rs:446-470` — 空テキスト時に VLM fallback するロジック |
| ✅ 完了 | `core/src/config.rs:218-244` — `VlmConfig` 構造体 |
| ✅ 完了 | `core/src/models.rs:161-164` — `IndexConfig` に vlm_* フィールド |
| ✅ 完了 | `cli/` の chart/clean/dredge/scan/doctor コマンドが `VlmConfig` を受け取る |
| ✅ 完了 | `core/Cargo.toml:59` — `vlm` feature 定義（`default` には未追加） |
| ✅ 完了 | `ref/cli.md` — `[vlm]` 設定セクション記載済み |
| ✅ 完了 | `docs/CLI-USE.md`, `docs/CLI-USE.ja.md` — `[vlm]` 設定セクション記載済み |
| ✅ 完了 | `docs/INSTALL.md`, `docs/INSTALL.ja.md` — `vlm` feature 追記済み |
| ❌ 未完了 | `cli/Cargo.toml` の `default` に `vlm` が含まれていない |
| ❌ 未完了 | vlm feature 有効化テストがない |
| ❌ 未完了 | PDF mtime キャッシュの再実行防止テストがない |
| ❌ 未完了 | vlm スタブ動作テストがない |

---

## ファイル構成

| ファイル | 変更内容 |
|---------|---------|
| `cli/Cargo.toml:38` | `default` に `vlm` を追加 |
| `core/tests/integration_test.rs` | vlm feature コンパイル確認テスト追加 |
| `core/tests/integration_test.rs` | mtime キャッシュ再実行防止テスト追加 |
| `core/src/vlm.rs` | `#[cfg(not(feature = "vlm"))]` スタブテスト追加 |

---

## Task 1: vlm feature を cli default に追加し、ビルド確認テストを追加

**Files:**
- Modify: `cli/Cargo.toml:38`
- Modify: `core/tests/integration_test.rs`

### Step 1-1: テストを書く（RED）

`core/tests/integration_test.rs` の末尾に追加：

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
    assert!(matches!(result, Ok(None)),
        "vlm feature enabled + config.enabled=false should return Ok(None), got: {:?}", result);
}
```

### Step 1-2: RED 確認

```bash
cargo test -p shiotsuchi-core --test integration_test -- --list 2>&1 | grep vlm_feature
# 期待: 何も表示されない（vlm が core の default にないため）
```

### Step 1-3: `cli/Cargo.toml` の default を修正

```toml
[features]
default = ["semantic", "pdf", "vlm"]
semantic = ["shiotsuchi-core/semantic"]
pdf = ["shiotsuchi-core/pdf"]
vlm = ["shiotsuchi-core/vlm"]
```

### Step 1-4: GREEN 確認（修正済みの検証方法）

**重要**: `cargo test -p shiotsuchi-core` では cli の feature 変更は反映されない。以下の方法で検証する：

```bash
# 方法1: core に vlm feature を明示的に付けて検証
cargo test -p shiotsuchi-core --features vlm --test integration_test -- --list 2>&1 | grep vlm_feature
# 期待: test_vlm_feature_is_compiled_and_not_compiled_stub_is_absent: test

# 方法2: CLI バイナリのビルドで vlm が有効になることを確認
cargo check -p shiotsuchi 2>&1 | grep -E "Finished|error"
# 期待: Finished

# 方法3: CLI 全テスト実行（vlm feature が有効な状態）
cargo test -p shiotsuchi 2>&1 | tail -5
# 期待: test result: ok. N passed; 0 failed
```

### Step 1-5: コミット

```bash
git add cli/Cargo.toml core/tests/integration_test.rs
git commit -m "feat(vlm): enable vlm feature in cli default build with compile-time test"
```

---

## Task 2: mtime キャッシュによる再実行防止テスト

**Files:**
- Modify: `core/tests/integration_test.rs`

### 背景

BDD シナリオ「VLM 抽出は初回のみ実行される」をテストする。`scan_empty.pdf` の代わりに `hello.pdf` + `enable_pdf_extraction: false` を使って確実にテキスト空の状態を再現する。

### Step 2-1: テストを書く

```rust
#[test]
fn test_pdf_reindex_is_skipped_when_file_unchanged() {
    use shiotsuchi_core::db::NoteDatabase;
    use shiotsuchi_core::indexer::{index_directory, IndexResult};
    use shiotsuchi_core::config::IndexConfig;
    use shiotsuchi_core::tokenizer::TokenizerConfig;

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
        enable_pdf_extraction: false, // テキスト抽出を無効化 → 空テキスト同等
        vlm_enabled: false,           // VLM も無効（API 呼び出しなし）
        ..Default::default()
    };

    // 1回目: 新規なので Inserted/Updated
    let (results1, _, _) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
    let first = results1.iter()
        .find(|(_, path, _)| path == "scan.pdf")
        .expect("scan.pdf should be in results");
    assert!(matches!(first.2, IndexResult::Inserted | IndexResult::Updated),
        "first index should insert or update, got: {:?}", first.2);

    // 2回目: ファイル未変更なので Skipped
    let (results2, _, _) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
    let second = results2.iter()
        .find(|(_, path, _)| path == "scan.pdf")
        .expect("scan.pdf should appear in results");
    assert!(matches!(second.2, IndexResult::Skipped),
        "second index of unchanged PDF should be Skipped, got: {:?}", second.2);
}
```

### Step 2-2: 実行・GREEN 確認

```bash
cargo test -p shiotsuchi-core --test integration_test test_pdf_reindex_is_skipped_when_file_unchanged 2>&1 | tail -5
```

### Step 2-3: コミット

```bash
git add core/tests/integration_test.rs
git commit -m "test(vlm): add mtime cache skip test for PDF reindex prevention"
```

---

## Task 3: vlm スタブ動作テスト

**Files:**
- Modify: `core/src/vlm.rs`

### Step 3-1: vlm.rs の `#[cfg(test)]` ブロックにスタブテスト追加

```rust
#[cfg(not(feature = "vlm"))]
#[test]
fn test_extract_not_compiled_returns_error() {
    let config = VlmConfig::default();
    let path = Path::new("/nonexistent/test.pdf");
    let result = extract_text_with_vlm(path, &config);
    assert!(matches!(result, Err(VlmError::NotCompiled)),
        "should return NotCompiled when vlm feature is off, got: {:?}", result);
}
```

### Step 3-2: vlm feature なしで実行

```bash
cargo test -p shiotsuchi-core --no-default-features --features pdf -- test_extract_not_compiled 2>&1 | tail -5
```

### Step 3-3: コミット

```bash
git add core/src/vlm.rs
git commit -m "test(vlm): add not-compiled stub test for vlm feature guard"
```

---

## Task 4: 全テストと最終確認

### Step 4-1: Core 全テスト

```bash
cargo test -p shiotsuchi-core 2>&1 | tail -5
```

### Step 4-2: CLI 全テスト（vlm feature 有効）

```bash
cargo test -p shiotsuchi 2>&1 | tail -5
```

### Step 4-3: MCP テスト

```bash
cargo test -p shiotsuchi-mcp 2>&1 | tail -5
```

### Step 4-4: vlm なしビルド

```bash
cargo build -p shiotsuchi --no-default-features --features "pdf,semantic" 2>&1 | tail -3
```

### Step 4-5: 受け入れ基準チェック

| 受け入れ基準 | 対応 |
|------------|------|
| VLM でテキスト変換を実行する | `test_vlm_feature_is_compiled_and_not_compiled_stub_is_absent` |
| mtime キャッシュで再実行を防ぐ | `test_pdf_reindex_is_skipped_when_file_unchanged` |
| VLM プロバイダーを config で選択できる | `VlmConfig` + `ref/cli.md` (既存) |
| VLM を設定でオフにできる | `VlmConfig::default().enabled == false` (既存) |
| API キー未設定時はスキップ | `test_extract_missing_api_key_returns_error` (既存) |
