use shiotsuchi_core::{
    db::NoteDatabase,
    indexer::{cleanup_deleted, index_directory},
    models::IndexConfig,
    search::extract_snippet,
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

    // Index: tokenizer を index_directory に渡す
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let (results, _invalid) = index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(results.len(), 3);

    // Search: tokenizer.and_query() で FTS5 AND クエリを構築してから db.search() に渡す
    let fts5_query = tokenizer.and_query("search engine");
    let search_results = db.search(&fts5_query, 10).unwrap();
    assert!(!search_results.is_empty());
    assert!(search_results[0].path.contains("project"));

    // Search 日本語: 同様に and_query() を経由する
    let ja_query = tokenizer.and_query("形態素");
    let ja_results = db.search(&ja_query, 10).unwrap();
    assert!(!ja_results.is_empty());

    // Stats
    let stats = db.stats().unwrap();
    assert_eq!(stats.total_notes, 3);

    // Cleanup
    fs::remove_file(vault.join("meeting.md")).unwrap();
    let removed = cleanup_deleted(&db, &config).unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(db.stats().unwrap().total_notes, 2);
}

#[test]
fn test_snippet_extraction() {
    let text = "First paragraph\n\nSecond paragraph with keyword\n\nThird paragraph";
    let snippet = extract_snippet(text, "keyword", 1, 1000);
    assert!(snippet.contains("keyword"));
}
