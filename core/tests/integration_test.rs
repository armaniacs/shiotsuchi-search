use std::collections::HashMap;
use shiotsuchi_core::{
    db::NoteDatabase,
    indexer::{cleanup_deleted, index_directory},
    models::{IndexConfig, SearchMode},
    search::{extract_snippet, search, SearchRequest},
    tokenizer::TokenizerConfig,
};
use std::fs;
use tempfile::TempDir;

// Re-export the macro for use in integration tests
use shiotsuchi_core::require_tokenizer;

#[test]
fn test_end_to_end_index_and_search() {
    let tokenizer = shiotsuchi_core::require_tokenizer!(TokenizerConfig::default());

    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();

    fs::write(
        vault.join("project.md"),
        "# Project Plan\n\nThis project is about building a search engine.",
    )
    .unwrap();

    fs::write(
        vault.join("meeting.md"),
        "---\ntitle: Team Meeting\n---\n\nWe discussed the search feature and timeline.",
    )
    .unwrap();

    fs::write(
        vault.join("japanese.md"),
        "# 日本語ノート\n\n形態素解析は非常に便利です。",
    )
    .unwrap();

    // Index
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        vaults: vec![("default".to_string(), vault.clone())],
        ..Default::default()
    };
    let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
    assert_eq!(results.len(), 3);

    // FTS search
    let _search_results = search(&db, &tokenizer, &SearchRequest {
        query: "search engine",
        limit: 10,
        mode: SearchMode::Fts,
        embedder: None,
        min_score: None,
        vault_filter: None,
        tag_filter: None,
        since_date: None,
        user_dictionary: &[],
        synonyms: &HashMap::new(),
        fuzzy: false,
        hybrid_alpha: None,
        mmr: false,
        lambda: 0.5,
        backlink_scoring: false,
        cursor: None,
    }).unwrap().results;

    let ja_results = search(&db, &tokenizer, &SearchRequest {
        query: "形態素",
        limit: 10,
        mode: SearchMode::Fts,
        embedder: None,
        min_score: None,
        vault_filter: None,
        tag_filter: None,
        since_date: None,
        user_dictionary: &[],
        synonyms: &HashMap::new(),
        fuzzy: false,
        hybrid_alpha: None,
        mmr: false,
        lambda: 0.5,
        backlink_scoring: false,
        cursor: None,
    }).unwrap().results;
    assert!(!ja_results.is_empty());

    // Stats
    let stats = db.stats().unwrap();
    assert_eq!(stats.total_files, 3);

    // Cleanup
    fs::remove_file(vault.join("meeting.md")).unwrap();
    let removed = cleanup_deleted(&db, &config).unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(db.stats().unwrap().total_files, 2);
}

#[test]
fn test_snippet_extraction() {
    let text = "First paragraph\n\nSecond paragraph with keyword\n\nThird paragraph";
    let snippet = extract_snippet(text, "keyword", 3, 1000);
    assert!(snippet.contains("keyword"));
}

/// vlm feature 有効ビルドでのみ実行されるテスト。
/// vlm が default に含まれない場合、このテスト関数はコンパイルされない。
#[cfg(feature = "vlm")]
#[test]
fn test_vlm_feature_is_compiled_and_not_compiled_stub_is_absent() {
    use shiotsuchi_core::config::VlmConfig;
    use shiotsuchi_core::vlm::extract_text_with_vlm;

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

#[test]
fn test_pdf_reindex_is_skipped_when_file_unchanged() {
    use shiotsuchi_core::indexer::IndexResult;

    let tokenizer = require_tokenizer!(TokenizerConfig::default());
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

/// vlm feature が OFF のとき、ビルドが通ることを確認するスタブテスト。
/// `#[cfg(not(feature = "vlm"))]` で囲むため、vlm が有効なビルドではこのテストは存在しない。
/// vlm が ON の場合、Task 1 の `test_vlm_feature_is_compiled` でカバー。
#[cfg(not(feature = "vlm"))]
#[test]
fn test_vlm_feature_not_compiled_builds_successfully() {
    use shiotsuchi_core::indexer::IndexResult;

    // vlm feature なしでも index_file は正常動作すること
    let tokenizer = require_tokenizer!(TokenizerConfig::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();

    fs::write(vault.join("hello.md"), "# Hello\n\nTest content.").unwrap();

    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        vaults: vec![("default".to_string(), vault.clone())],
        vlm_enabled: false,
        ..Default::default()
    };

    let (results, _, _) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
    assert_eq!(results.len(), 1);
    assert!(matches!(results[0].2, IndexResult::Inserted | IndexResult::Updated));
}
