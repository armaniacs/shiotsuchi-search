use shiotsuchi_core::{
    db::NoteDatabase,
    models::Chunk,
};
use tempfile::TempDir;

#[test]
fn test_insert_chunks_and_lookup() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    let db = NoteDatabase::open(&db_path).unwrap();

    let chunks = vec![
        Chunk {
            id: None,
            file_path: "note1.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "Title 1 body content".into(),
            tokenized_content: "Title 1 body content".into(),
            vault_name: String::new(),
        },
    ];
    let ids = db.insert_chunks(&chunks).unwrap();
    assert_eq!(ids.len(), 1);

    db.upsert_file_cache("note1.md", "hash1", 1_000, "none").unwrap();

    let cached = db.cached_hash("note1.md").unwrap();
    assert_eq!(cached, Some("hash1".to_string()));

    // Second insert with same hash — caller would skip before calling insert
    let cached2 = db.cached_hash("note1.md").unwrap();
    assert_eq!(cached2, Some("hash1".to_string()));

    let paths = db.list_cached_paths().unwrap();
    assert_eq!(paths.len(), 1);
    assert!(paths.contains(&"note1.md".to_string()));

    // FTS search should find the chunk
    let results = db.fts_search("Title", 10).unwrap();
    assert!(!results.is_empty());
}

#[test]
fn test_delete_chunks_atomic() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    let db = NoteDatabase::open(&db_path).unwrap();

    let chunks_a = vec![Chunk {
        id: None,
        file_path: "a.md".into(),
        chunk_index: 0,
        parent_header: None,
        content: "body a".into(),
        tokenized_content: "body a".into(),
        vault_name: String::new(),
    }];
    let chunks_b = vec![Chunk {
        id: None,
        file_path: "b.md".into(),
        chunk_index: 0,
        parent_header: None,
        content: "body b".into(),
        tokenized_content: "body b".into(),
        vault_name: String::new(),
    }];

    db.insert_chunks(&chunks_a).unwrap();
    db.upsert_file_cache("a.md", "hash_a", 1, "none").unwrap();
    db.insert_chunks(&chunks_b).unwrap();
    db.upsert_file_cache("b.md", "hash_b", 2, "none").unwrap();

    assert_eq!(db.stats().unwrap().total_files, 2);

    // Delete a.md
    db.delete_chunks_for_file("a.md").unwrap();
    db.delete_file_cache("a.md").unwrap();

    // a.md should be gone, b.md should remain
    assert_eq!(db.cached_hash("a.md").unwrap(), None);
    assert_eq!(db.cached_hash("b.md").unwrap(), Some("hash_b".to_string()));
    assert_eq!(db.stats().unwrap().total_files, 1);
}
