use shiotsuchi_core::db::NoteDatabase;
use tempfile::TempDir;

#[test]
fn test_upsert_note_commits_on_success() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    let db = NoteDatabase::open(&db_path).unwrap();

    let result = db
        .upsert_note(
            "note1.md",
            "Title 1",
            "トークン化 本文",
            "hash1",
            1_000,
        )
        .unwrap();
    assert!(result, "first upsert should report changed");

    let meta = db.get_metadata("note1.md").unwrap();
    assert_eq!(meta.title, "Title 1");
    assert_eq!(meta.hash, "hash1");

    // Second upsert with same hash should skip
    let result2 = db
        .upsert_note(
            "note1.md",
            "Title 1",
            "トークン化 本文",
            "hash1",
            1_000,
        )
        .unwrap();
    assert!(!result2, "unchanged note should be skipped");

    // Ensure exactly one note in metadata
    let paths = db.list_paths().unwrap();
    assert_eq!(paths.len(), 1);
    assert!(paths.contains(&"note1.md".to_string()));

    // Ensure exactly one row in FTS via search
    let results = db.search("Title 1", 10).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].path, "note1.md");
}

#[test]
fn test_delete_note_atomic() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    let db = NoteDatabase::open(&db_path).unwrap();

    // Insert two notes
    db.upsert_note("a.md", "A", "body a", "hash_a", 1).unwrap();
    db.upsert_note("b.md", "B", "body b", "hash_b", 2).unwrap();

    // Verify both exist
    let meta_a_before = db.get_metadata("a.md").unwrap();
    let meta_b_before = db.get_metadata("b.md").unwrap();
    assert_eq!(meta_a_before.title, "A");
    assert_eq!(meta_b_before.title, "B");

    // Delete a.md
    db.delete_note("a.md").unwrap();

    // a.md should be gone, b.md should remain
    assert!(db.get_metadata("a.md").is_err());
    let meta_b_after = db.get_metadata("b.md").unwrap();
    assert_eq!(meta_b_after.title, "B");
}
