use shiotsuchi_core::{
    db::NoteDatabase,
    indexer::{cleanup_deleted, index_directory},
    models::{Chunk, IndexConfig, ReindexParams},
    tokenizer::TokenizerConfig,
};
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

fn make_chunk(file_path: &str, content: &str, tags: &str) -> Chunk {
    Chunk {
        id: None,
        file_path: file_path.into(),
        chunk_index: 0,
        parent_header: None,
        content: content.into(),
        tokenized_content: content.into(),
        vault_name: "default".to_string(),
        tags: tags.into(),
        frontmatter_date: String::new(),
        title: String::new(),
        emphasized_text: String::new(),
    }
}

fn tag_map(stats: &[(String, usize)]) -> HashMap<&str, usize> {
    stats.iter().map(|(tag, count)| (tag.as_str(), *count)).collect()
}

#[test]
fn test_tag_count_decrement_with_count_zero_guard() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks = vec![make_chunk("file.md", "content", "test-tag")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "file.md",
        hash: "hash1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.tag_stats(100).unwrap();
    let map = tag_map(&stats);
    assert_eq!(map.get("test-tag"), Some(&1));

    let chunks_no_tag = vec![make_chunk("file.md", "content", "")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "file.md",
        hash: "hash2",
        mtime: 1001,
        model_id: "none",
        chunks: &chunks_no_tag,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.tag_stats(100).unwrap();
    assert!(stats.is_empty(), "tag with count=0 should be filtered out: {:?}", stats);

    db.decrement_tag_count(vault, "never-existed").unwrap();

    let stats = db.stats().unwrap();
    assert_eq!(stats.total_files, 1);
}

#[test]
fn test_tag_count_increment_decrement_balance() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks = vec![make_chunk("a.md", "content", "tag1,tag2")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "a.md",
        hash: "h1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let chunks2 = vec![make_chunk("b.md", "content", "tag1,tag3")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "b.md",
        hash: "h2",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks2,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.tag_stats(100).unwrap();
    let map = tag_map(&stats);
    assert_eq!(map.get("tag1"), Some(&2));
    assert_eq!(map.get("tag2"), Some(&1));
    assert_eq!(map.get("tag3"), Some(&1));

    let chunks_no_tag = vec![make_chunk("a.md", "content", "")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "a.md",
        hash: "h3",
        mtime: 1001,
        model_id: "none",
        chunks: &chunks_no_tag,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.tag_stats(100).unwrap();
    let map = tag_map(&stats);
    assert_eq!(map.get("tag1"), Some(&1), "tag1 should be 1 after reindex");
    assert_eq!(map.get("tag3"), Some(&1), "tag3 should remain 1");
    assert_eq!(map.get("tag2"), None, "tag2 should be gone (count=0)");
}

#[test]
fn test_char_count_is_unicode_chars_not_bytes() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let japanese = "形態素解析は便利";
    assert_eq!(japanese.len(), 24, "sanity: UTF-8 byte length is 24");
    assert_eq!(japanese.chars().count(), 8, "sanity: Unicode char count is 8");

    let chunks = vec![make_chunk("jp.md", japanese, "japanese")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "jp.md",
        hash: "h1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks,
        embeddings: &[],
        file_size: japanese.len() as i64,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.stats().unwrap();
    assert_eq!(
        stats.total_chars, 8,
        "total_chars should be 8 (Unicode chars) not 24 (UTF-8 bytes)"
    );

    let conn = db.write_conn.borrow();
    let stored: i64 = conn.query_row(
        "SELECT char_count FROM file_cache WHERE path = 'jp.md'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(stored, 8);
}

#[test]
fn test_cleanup_deleted_tag_count_consistency() {
    let tokenizer = shiotsuchi_core::require_tokenizer!(TokenizerConfig::default());
    let temp = TempDir::new().unwrap();

    let vault = temp.path().join("vault1");
    fs::create_dir(&vault).unwrap();

    fs::write(
        vault.join("keep.md"),
        "---\ntags: [keep-tag]\n---\n\nKeep this file.",
    ).unwrap();
    fs::write(
        vault.join("remove.md"),
        "---\ntags: [remove-tag]\n---\n\nWill be removed.",
    ).unwrap();

    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        vaults: vec![("default".to_string(), vault.clone())],
        ..Default::default()
    };
    let (results, _invalid, _excluded) = index_directory(&db, &tokenizer, &config, None, None).unwrap();
    assert_eq!(results.len(), 2);

    let stats = db.tag_stats(100).unwrap();
    let map = tag_map(&stats);
    assert_eq!(map.get("keep-tag"), Some(&1), "keep-tag should be 1");
    assert_eq!(map.get("remove-tag"), Some(&1), "remove-tag should be 1");

    fs::remove_file(vault.join("remove.md")).unwrap();
    let removed = cleanup_deleted(&db, &config).unwrap();
    assert_eq!(removed.len(), 1, "one file should be removed");

    let stats = db.tag_stats(100).unwrap();
    let map = tag_map(&stats);
    assert_eq!(map.get("keep-tag"), Some(&1), "keep-tag should remain 1");
    assert_eq!(map.get("remove-tag"), None, "remove-tag should be gone (count=0)");

    let stats = db.stats().unwrap();
    assert_eq!(stats.total_files, 1);
}

#[test]
fn test_multiple_chunks_tag_aggregation() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks = vec![
        make_chunk("multi.md", "# Header 1", "tag-a,tag-b"),
        make_chunk("multi.md", "Body text", "tag-a"),
        make_chunk("multi.md", "## Subheader", "tag-b,tag-c"),
    ];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "multi.md",
        hash: "h1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks,
        embeddings: &[],
        file_size: 30,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.tag_stats(100).unwrap();
    let map = tag_map(&stats);
    assert_eq!(map.get("tag-a"), Some(&2), "tag-a appears in 2 chunks");
    assert_eq!(map.get("tag-b"), Some(&2), "tag-b appears in 2 chunks");
    assert_eq!(map.get("tag-c"), Some(&1), "tag-c appears in 1 chunk");

    let chunks2 = vec![
        make_chunk("multi.md", "# Header 1", "tag-a"),
        make_chunk("multi.md", "Body text", "tag-a"),
    ];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "multi.md",
        hash: "h2",
        mtime: 1001,
        model_id: "none",
        chunks: &chunks2,
        embeddings: &[],
        file_size: 20,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.tag_stats(100).unwrap();
    let map = tag_map(&stats);
    assert_eq!(map.get("tag-a"), Some(&2), "tag-a should remain 2");
    assert_eq!(map.get("tag-b"), None, "tag-b should be gone");
    assert_eq!(map.get("tag-c"), None, "tag-c should be gone");
}

#[test]
fn test_char_count_ascii() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks = vec![make_chunk("ascii.md", "Hello World", "ascii")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "ascii.md",
        hash: "h1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks,
        embeddings: &[],
        file_size: 11,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.stats().unwrap();
    assert_eq!(stats.total_chars, 11);
}

#[test]
fn test_tag_count_idempotent_reindex() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks = vec![make_chunk("stable.md", "content", "tag-x")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "stable.md",
        hash: "h1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "stable.md",
        hash: "h1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.tag_stats(100).unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].1, 1, "idempotent reindex should not inflate tag count");
}

#[test]
fn test_tag_count_empty_tags_handling() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks = vec![make_chunk("empty.md", "content", "")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "empty.md",
        hash: "h1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.tag_stats(100).unwrap();
    assert!(stats.is_empty(), "no tags should produce empty stats");

    let stats = db.stats().unwrap();
    assert_eq!(stats.total_files, 1);
}

#[test]
fn test_tag_count_no_double_decrement_on_reindex_without_tags() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks_a = vec![make_chunk("a.md", "content", "shared-tag")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "a.md",
        hash: "h1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks_a,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let chunks_b = vec![make_chunk("b.md", "content", "shared-tag")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "b.md",
        hash: "h2",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks_b,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let chunks_a_no_tag = vec![make_chunk("a.md", "content", "")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "a.md",
        hash: "h3",
        mtime: 1001,
        model_id: "none",
        chunks: &chunks_a_no_tag,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let stats = db.tag_stats(100).unwrap();
    assert_eq!(stats.len(), 1);
    assert_eq!(stats[0].1, 1);
}
