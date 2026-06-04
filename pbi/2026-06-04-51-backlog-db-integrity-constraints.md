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
