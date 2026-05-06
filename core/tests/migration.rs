use shiotsuchi_core::db::NoteDatabase;

#[test]
fn test_schema_version_is_set() {
    let db = NoteDatabase::open_in_memory().unwrap();
    let conn = db.conn.borrow();
    let version: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
}
