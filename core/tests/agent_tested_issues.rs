use shiotsuchi_core::{
    db::NoteDatabase,
    models::{Chunk, ReindexParams},
};
use std::collections::HashMap;

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

// ============================================================================
// Test 1: reindex_file stores Unicode char_count, not UTF-8 byte length.
//
// Regression guard for the v10 backfill bug where LENGTH(content) was used
// (returns bytes). reindex_file uses .chars().count() which is correct.
// ============================================================================

#[test]
fn test_migration_v10_backfill_char_count_is_unicode_not_bytes() {
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

    let conn = db.write_conn.borrow();
    let stored: i64 = conn.query_row(
        "SELECT char_count FROM file_cache WHERE path = 'jp.md' AND vault_name = 'default'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(
        stored, 8,
        "char_count in file_cache should be 8 (Unicode chars), not 24 (UTF-8 bytes)"
    );
    drop(conn);

    let stats = db.stats().unwrap();
    assert_eq!(
        stats.total_chars, 8,
        "stats.total_chars should reflect Unicode char count"
    );
}

// ============================================================================
// Test 2: reindex_file should physically delete zero-count tag rows.
//
// delete_file_fully deletes tag_counts rows where count=0.
// reindex_file only does UPDATE without DELETE, leaving ghost rows.
// This test verifies that after reindex with tag removal, the row is gone.
// ============================================================================

#[test]
fn test_reindex_file_cleans_up_zero_count_tag_rows() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks_a = vec![make_chunk("a.md", "content A", "removable-tag")];
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

    {
        let conn = db.write_conn.borrow();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tag_counts WHERE tag = 'removable-tag' AND vault_name = 'default'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 1, "removable-tag row should exist after first index");
    }

    let chunks_b = vec![make_chunk("a.md", "content A updated", "")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "a.md",
        hash: "h2",
        mtime: 1001,
        model_id: "none",
        chunks: &chunks_b,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    {
        let conn = db.write_conn.borrow();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tag_counts WHERE tag = 'removable-tag' AND vault_name = 'default'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(
            count, 0,
            "zero-count tag row should be deleted by reindex_file, not just filtered"
        );
    }

    let stats = db.tag_stats(100).unwrap();
    assert!(
        stats.iter().all(|(tag, _)| tag != "removable-tag"),
        "tag_stats should not include zero-count tags: {:?}",
        stats
    );
}

// ============================================================================
// Test 3: delete_file_fully should clean up incoming note_links
//         (where the deleted file is the target, not the source).
//
// Issue: delete_file_fully only DELETEs WHERE source_path = file.
// Incoming links (WHERE target_path = file) are left as orphans.
// ============================================================================

#[test]
fn test_delete_file_fully_removes_incoming_note_links() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    // Index file "a.md" with a link TO "target.md"
    let chunks_a = vec![make_chunk("a.md", "content A", "")];
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
        note_link_targets: &["target.md".to_string()],
        vlm_hash: None,
    }).unwrap();

    // Index file "b.md" with a link TO "target.md"
    let chunks_b = vec![make_chunk("b.md", "content B", "")];
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
        note_link_targets: &["target.md".to_string()],
        vlm_hash: None,
    }).unwrap();

    // Index "target.md" itself
    let chunks_t = vec![make_chunk("target.md", "target content", "")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "target.md",
        hash: "h3",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks_t,
        embeddings: &[],
        file_size: 10,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    // Verify both incoming links exist
    {
        let conn = db.write_conn.borrow();
        let incoming: i64 = conn.query_row(
            "SELECT COUNT(*) FROM note_links WHERE target_path = 'target.md' AND vault_name = 'default'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(incoming, 2, "both incoming links should exist before delete");
    }

    // Delete target.md
    db.delete_file_fully(vault, "target.md").unwrap();

    // Verify: source_path links of target.md should be deleted
    {
        let conn = db.write_conn.borrow();
        let source_links: i64 = conn.query_row(
            "SELECT COUNT(*) FROM note_links WHERE source_path = 'target.md' AND vault_name = 'default'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(source_links, 0, "outgoing links from deleted file should be gone");
    }

    // Verify: incoming links TO target.md should also be deleted
    {
        let conn = db.write_conn.borrow();
        let incoming: i64 = conn.query_row(
            "SELECT COUNT(*) FROM note_links WHERE target_path = 'target.md' AND vault_name = 'default'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(
            incoming, 0,
            "incoming links to deleted file should be removed (no orphans)"
        );
    }

    // Links from a.md and b.md to other targets should remain intact
    // (They link to target.md which was deleted, so they should be gone too)
    {
        let conn = db.write_conn.borrow();
        let remaining: i64 = conn.query_row(
            "SELECT COUNT(*) FROM note_links WHERE vault_name = 'default'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(remaining, 0, "all links referencing deleted file should be cleaned up");
    }
}

// ============================================================================
// Test 4 (bonus): Multi-chunk Japanese char_count accuracy
// ============================================================================

#[test]
fn test_char_count_multi_chunk_japanese() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks = vec![
        make_chunk("multi.md", "日本語テスト", ""),
        make_chunk("multi.md", "形態素解析", ""),
    ];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "multi.md",
        hash: "h1",
        mtime: 1000,
        model_id: "none",
        chunks: &chunks,
        embeddings: &[],
        file_size: 20,
        tasks: &[],
        note_link_targets: &[],
        vlm_hash: None,
    }).unwrap();

    let conn = db.write_conn.borrow();
    let stored: i64 = conn.query_row(
        "SELECT char_count FROM file_cache WHERE path = 'multi.md' AND vault_name = 'default'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(stored, 11, "日本語テスト=6 chars + 形態素解析=5 chars = 11");
}

// ============================================================================
// Test 5: delete_file_fully + reindex_file tag consistency
// ============================================================================

#[test]
fn test_delete_then_reindex_tag_consistency() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let vault = "default";

    let chunks = vec![make_chunk("del.md", "content", "del-tag")];
    db.reindex_file(&ReindexParams {
        vault_name: vault,
        relative_path: "del.md",
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

    let binding = db.tag_stats(100).unwrap();
    let stats_before = tag_map(&binding);
    assert_eq!(stats_before.get("del-tag"), Some(&1));

    db.delete_file_fully(vault, "del.md").unwrap();

    let stats_after = db.tag_stats(100).unwrap();
    assert!(stats_after.is_empty(), "all tags should be gone after delete_file_fully");

    {
        let conn = db.write_conn.borrow();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM tag_counts WHERE tag = 'del-tag' AND vault_name = 'default'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(count, 0, "tag_counts row should be physically deleted");
    }
}
