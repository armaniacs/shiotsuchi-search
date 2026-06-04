# migrate() モジュール分割 設計書

## PBI
PBI-48 (Linear DEV-42) — migrate() 単一責任原則違反の解消

## 設計目標
`core/src/db.rs` 内の `migrate()` (245行) をバージョン別ファイルに分割する。
ロジックの変更は一切行わず、コードの物理的な再配置のみ。

## 非目標（この設計の範囲外）
- トランザクション化されていない移行ブロックの修正（PBI-51）
- パーバージョンの移行テスト追加
- `create_schema()` のロジック変更
- スキーマバージョン番号体系の変更

## モジュール構造

```
core/src/migration/
├── mod.rs       # ディスパッチャ run() + create_schema()
├── v02.rs       # v1→v2: DROP old, create_schema
├── v03.rs       # v2→v3: vault_name + file_cache再構成
├── v04.rs       # v3→v4: vec_chunks再作成 (FLOAT[1024])
├── v05.rs       # v4→v5: file_size
├── v06.rs       # v5→v6: tags/ frontmatter_date/ title
├── v07.rs       # v6→v7: tasks + self-heal
├── v08.rs       # v7→v8: emphasized_text
├── v09.rs       # v8→v9: note_links + backlink_count (transaction)
├── v10.rs       # v9→v10: char_count + tag_counts (transaction)
└── v11.rs       # v10→v11: vlm_hash
```

## ディスパッチャ (`mod.rs`)

```rust
/// Run all pending schema migrations.
pub fn run(conn: &Connection) -> Result<(), crate::db::DbError> {
    conn.execute_batch("DROP TABLE IF EXISTS file_cache_v3")?;

    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if version < 2  { v02::migrate(conn)?; }
    if version < 3  { v03::migrate(conn)?; }
    if version < 4  { v04::migrate(conn)?; }
    if version < 5  { v05::migrate(conn)?; }
    if version < 6  { v06::migrate(conn)?; }
    if version < 7  { v07::migrate(conn)?; }
    if version < 8  { v08::migrate(conn)?; }
    if version < 9  { v09::migrate(conn)?; }
    if version < 10 { v10::migrate(conn)?; }
    if version < 11 { v11::migrate(conn)?; }

    Ok(())
}
```

### 設計判断
- **バージョンチェックを dispatcher に一元化**: 1回の `PRAGMA user_version` 読み取りで済む。各バージョン関数は「必ず実行される」前提で書けるためシンプル
- **孤立クリーンアップは dispatcher 先頭**: 既存の挙動を維持（バージョンに関係なく常に実行）
- **`create_schema()` は `mod.rs` に同居**: v02 からのみ呼ばれる。`pub(crate) fn create_schema(conn: &Connection) -> SqliteResult<()>` として定義

## 各バージョン関数のシグネチャ（共通）

```rust
pub fn migrate(conn: &Connection) -> Result<(), crate::db::DbError> {
    // DDL/DML の実行（既存コードをそのまま移植）
    conn.execute_batch("PRAGMA user_version = N")?;
    Ok(())
}
```

- `DbError` のフルパス指定（`crate::db::DbError`）— 循環参照にならずコンパイル可能
- トランザクションの有無、self-heal、column check の有無も含めて既存コードを完全コピー
- `v02.rs` 内でのみ `super::create_schema(&conn)` を呼ぶ（他は不要）

## `db.rs` の変更

```rust
// 変更前:
fn migrate(&self) -> Result<(), DbError> {
    let conn = self.write_conn.borrow();
    // ...245行...
}

// 変更後:
fn migrate(&self) -> Result<(), DbError> {
    let conn = self.write_conn.borrow();
    crate::migration::run(&conn)
}
```

`create_schema()` メソッドも `db.rs` から削除（`migration/mod.rs` に移動）。

`open()` と `open_in_memory()` は変更不要（内部で `self.migrate()` を呼んでいるため）。

## `lib.rs` の変更

```rust
pub mod migration;
```

## テスト

既存のテストは変更不要。`migration::run(&conn)` 経由で透過的に動作する。
パーバージョンの新規テストは作成しない（本PBIの範囲外）。

## 実装手順

1. `core/src/migration/` ディレクトリを作成
2. `mod.rs` に `run()` + `create_schema()` を実装
3. `v02.rs`〜`v11.rs` を順次作成（各バージョンブロックを切り出し）
4. `db.rs` から `migrate()` + `create_schema()` を削除し、`crate::migration::run()` 呼び出しに置換
5. `lib.rs` に `pub mod migration;` を追加
6. `cargo check` でコンパイル確認
7. `cargo test -p shiotsuchi-core` で全テスト通過確認

## リスク
- なし。コードの物理移動のみでロジック変更は一切行わない
