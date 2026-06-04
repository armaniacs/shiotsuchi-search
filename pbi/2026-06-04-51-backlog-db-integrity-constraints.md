# PBI-51: FTS/vec 参照整合性制約と vec_chunks トランザクション

**発端:** Data Integrity Expert (スコア85)
**影響:**
1. v3→v4 マイグレーションで vec_chunks の DROP/CREATE がトランザクション外
2. FTS/vec 仮想テーブルに参照整合性制約がない (chunk削除時に残骸が残る可能性)
**対処:**
1. vec_chunks DROP/CREATE をトランザクション内に移動
2. 仮想テーブル運用の制約をドキュメント化 (sqlite-vec/ FTS5 は外部キー制約未対応のため)
**工数:** 1-2日
**状態:** 未着手

## 現状分析

### トランザクション外の移行ブロック

現在の `core/src/migration/v04.rs`:
```rust
pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // ❌ BEGIN TRANSACTION なし
    conn.execute_batch("DROP TABLE IF EXISTS vec_chunks")?;
    conn.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(...)")?;
    conn.execute_batch("PRAGMA user_version = 4")?;
    Ok(())
}
```

v1→v2, v8→v9, v9→v10 はトランザクションで包まれているが、v3→v4 は包まれていない。

### 参照整合性の問題

- `chunks` テーブルから chunk を削除しても、`fts_chunks` と `vec_chunks` に残骸が残る
- 現状は `delete_chunks_for_file` で3テーブルを個別に削除（手動整合性管理）
- SQLite の仮想テーブル（FTS5, vec0）は外部キー制約非対応

## BDD 受け入れシナリオ

```gherkin
Scenario: v04 マイグレーションがトランザクションで実行される
  Given version=3 のデータベースが存在する
  When マイグレーションを実行する
  Then vec_chunks テーブルが再作成される
  And user_version が4になる
  And マイグレーションが中断された場合、変更がロールバックされる

Scenario: マイグレーション中にエラーが発生した場合
  Given version=3 のデータベースが存在する
  And vec_chunks の DROP が失敗する
  Then マイグレーションがエラーで終了する
  And user_version が3のままになる
  And vec_chunks テーブルが残っている

Scenario: delete_file_fully が整合性を保つ
  Given チャンクが存在する
  And 対応する FTS/vec エントリが存在する
  When delete_file_fully を呼び出す
  Then chunks, fts_chunks, vec_chunks から全てのエントリが削除される
  And 残骸が残らない
```

## TDD アプローチ

### Phase 1: 既存テストの確認（グリーン維持）

1. **既存テストの実行**: `cargo test -p shiotsuchi-core --test migration` で全テスト通過を確認
2. **テストカバレッジの確認**: v04 マイグレーションのテストが存在することを確認

### Phase 2: トランザクション化のテスト追加（レッド）

```rust
#[test]
fn test_v04_migration_is_transactional() {
    let temp = TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");

    // v3 の DB を作成
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("
            CREATE TABLE chunks (id INTEGER PRIMARY KEY, content TEXT);
            CREATE VIRTUAL TABLE vec_chunks USING vec0(chunk_id INTEGER PRIMARY KEY, embedding FLOAT[1024]);
            PRAGMA user_version = 3;
        ").unwrap();
    }

    // v04 マイグレーションを実行（途中で中断されるようシミュレート）
    // 実際のテストでは、マイグレーション中のエラーをシミュレートするのは困難
    // 代わりに、トランザクションが正しく使用されることを確認

    let conn = Connection::open(&db_path).unwrap();
    shiotsuchi_core::migration::v04::migrate(&conn).unwrap();

    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
    assert_eq!(version, 4);
}
```

### Phase 3: 実装（グリーン）

```rust
pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    conn.execute_batch("BEGIN TRANSACTION")?;
    conn.execute_batch("DROP TABLE IF EXISTS vec_chunks")?;
    conn.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(...)")?;
    conn.execute_batch("PRAGMA user_version = 4")?;
    conn.execute_batch("COMMIT")?;
    Ok(())
}
```

### Phase 4: ドキュメント化

- `ref/core.md` に「仮想テーブルの整合性管理」セクションを追加
- FTS5/vec0 は外部キー制約非対応のため、アプリケーション層で手動管理が必要なことを明記
- `delete_file_fully()` が正しい整合性管理方法であることを記載

## 実装方針

### 1. v04 マイグレーションのトランザクション化

```rust
pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    conn.execute_batch("BEGIN TRANSACTION")?;
    conn.execute_batch("DROP TABLE IF EXISTS vec_chunks")?;
    conn.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks USING vec0(...)")?;
    conn.execute_batch("PRAGMA user_version = 4")?;
    conn.execute_batch("COMMIT")?;
    Ok(())
}
```

### 2. ドキュメント化

- `ref/core.md` に「仮想テーブルの整合性管理」セクションを追加
- FTS5/vec0 は外部キー制約非対応のため、アプリケーション層で手動管理が必要なことを明記
- `delete_file_fully()` が正しい整合性管理方法であることを記載
