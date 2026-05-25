use shiotsuchi_core::{
    db::NoteDatabase,
    indexer::{cleanup_deleted, index_directory},
    models::{IndexConfig, SearchMode},
    search::{extract_snippet, search},
    tokenizer::TokenizerConfig,
};
use std::fs;
use tempfile::TempDir;

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
    let (results, _invalid) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
    assert_eq!(results.len(), 3);

    // FTS search
    let search_results = search(&db, &tokenizer, "search engine", 10, SearchMode::Fts, None, None, None, None, None).unwrap();
    assert!(!search_results.is_empty());
    assert!(search_results[0].file_path.contains("project"));

    // Japanese FTS search
    let ja_results = search(&db, &tokenizer, "形態素", 10, SearchMode::Fts, None, None, None, None, None).unwrap();
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
