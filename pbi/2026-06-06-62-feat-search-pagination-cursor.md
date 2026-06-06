# PBI-62: 検索結果のカーソルベースページネーション対応 (DEV-67)

## ユーザーストーリー

上位ユーザー（大量ノートを検索するパワーユーザー）として、オフセットではなくカーソルでページネーションしてほしい、なぜなら現在の offset/limit 方式では大量ページをめくると性能が劣化し（FTS5 の `OFFSET` は全候補をスキャンする）、またインデックス更新中にページ間で結果の重複・欠落が発生するから

## ビジネス価値

- 大量データセット（10万件超）でも安定したページネーション性能を提供
- インデックス更新中のページ間一貫性を保証（カーソル方式では追加・削除があってもカーソル位置がずれない）
- API ユーザー（HTTP、MCP）にとって信頼性の高いページネーション手段を提供

## 現状

現在の `search` 関数および HTTP API は offset/limit 方式のみをサポートしている。

```rust
// 現行の API パラメータ
pub struct SearchParams {
    pub q: String,
    pub limit: Option<usize>,     // 最大200、デフォルト50
    pub offset: Option<usize>,    // デフォルト0、最大50000
    pub mode: Option<String>,     // "fts" | "vec" | "hybrid"
    pub vault: Option<String>,
    pub tag: Option<String>,
    pub since: Option<String>,
}

// レスポンス
{
    "results": [...],
    "count": 10,
    "total": 1500,
    "offset": 0,
    "limit": 10
}
```

## BDD 受け入れシナリオ

```gherkin
Scenario: カーソルで次のページを取得する
  Given 検索クエリ "project" の検索結果が20件以上存在する
  When  1ページ目（limit=10）をカーソルなしでリクエストする
  Then  レスポンスに10件の結果と next_cursor が含まれる
  When  next_cursor を使って2ページ目をリクエストする
  Then  次の10件が返る
  And   1ページ目と2ページ目に重複がない

Scenario: カーソルが空の場合は全件取得完了を示す
  Given 検索結果が5件のみ存在する
  When  limit=10 でリクエストする
  Then  next_cursor が null または空文字列である

Scenario: カーソルなしリクエストは offset/limit と後方互換性がある
  Given 既存の offset/limit API
  When  cursor パラメータなしでリクエストする
  Then  現在と同じ offset/limit 方式で動作する
  But  レスポンスに next_cursor が追加で含まれる
```

## 受け入れ基準

- [ ] HTTP API `/api/v1/search` が `cursor` クエリパラメータを受け付ける
- [ ] レスポンスに `next_cursor` フィールドが含まれる
- [ ] `offset` パラメータとの併用時にエラーにならない（offset が優先）
- [ ] カーソル方式でページ送りした結果に重複・欠落がない
- [ ] 内部の `search()` 関数のシグネチャは変わらない（後方互換性）
- [ ] 既存の offset/limit テストがすべてグリーン
- [ ] `cargo test -p shiotsuchi-core` がグリーン

## テスト戦略（t_wada スタイル）

### E2Eテスト
- `core/tests/integration_test.rs` にカーソルページネーションのシナリオテストを追加

### 統合テスト
- HTTP API の `test_search_cursor_pagination` テストを追加（`tower::ServiceExt` 使用）

### 単体テスト
- カーソルのエンコード/デコードのユニットテスト
- ページ境界値テスト（cursor が不正な場合のエラーハンドリング）

## 実装アプローチ

### カーソルの設計

カーソルは base64 エンコードされた opaque な文字列とする。内部構造は以下の通り：

```rust
struct Cursor {
    // 最終結果の FTS rowid（排他的）。これ以降の結果を返す。
    last_rowid: i64,
    // 検索パラメータのハッシュ（カーソルが異なる検索に使われるのを防ぐ）
    query_hash: u64,
}
```

### 実装手順

1. `core/src/search.rs` に `Cursor` エンコード/デコード関数を追加
2. `search()` 関数の内部で cursor を受け取り FTS `WHERE rowid < ? ORDER BY rowid DESC` に変換
3. レスポンス生成時に次のカーソルを計算
4. HTTP API の `SearchParams` に `cursor` フィールドを追加
5. API レスポンスに `next_cursor` フィールドを追加

### レイヤー変更

```
core::search::search()
  └─ 新規引数: cursor: Option<&str>
  └─ 戻り値に next_cursor: Option<String> を追加
      └─ core::models::SearchResult に next_cursor を追加

core::server::handlers::handle_search()
  └─ SearchParams に cursor: Option<String> 追加
  └─ レスポンス JSON に next_cursor 追加

cli::commands::dive::run_dive()
  └─ CLI レベルでは cursor は実装しない（MCP/HTTP 向け機能）
```

## 見積もり

6〜8時間（カーソル設計 + エンコード/デコード + テスト + API 変更）

## 技術的考慮事項

- **FTS5 の制約**: FTS5 の `ORDER BY rank DESC` と cursor の組み合わせは、`rank` 値が同一の場合に順序が不定になる可能性がある。`rowid` をタイブレーカーとして使用する
- **セキュリティ**: カーソルは base64 エンコードされていても opaque（内部構造を公開しない）。改ざんを検出するための HMAC も検討する（ただし簡易版では query_hash でのチェックだけでも十分）
- **Hybrid 検索**: Hybrid モードでは RRF スコア順のため、カーソル方式は FTS 時のみシンプルに実装。Hybrid のカーソルは将来の拡張とする
- **offset との併用**: cursor と offset の両方が指定された場合は cursor を優先する
- **性能測定**: 10万件のデータセットで offset 10000 と cursor ベース10000件目以降の取得時間を比較する

## 実装者向け注記

### 現状コードの確認

```bash
# 検索のデータフロー
grep -n "pub fn search\|fn search_fts\|fn search_hybrid" core/src/search.rs

# HTTP API のパラメータ
grep -n "SearchParams\|struct SearchParams" core/src/server/types.rs

# レスポンス形式
grep -n "next_cursor\|offset\|limit" core/src/server/handlers.rs | head -10
```

### 実装手順

1. `core/src/search.rs` に `Cursor` 型とエンコード/デコードを追加
2. `search_fts` で cursor に対応した SQL クエリを生成
3. `pub fn search` のシグネチャを拡張（後方互換を維持するためデフォルト引数は使えない。`Option<&str>` で追加）
4. HTTP ハンドラーを更新
5. テスト追加
6. `make test` で全テスト確認

### 落とし穴

- **FTS5 の `ORDER BY rank DESC, rowid DESC`**: 同一 rank の結果順序を安定させるために必ず `rowid` をタイブレーカーに含める
- **カーソル不正時のエラー**: デコード失敗時は `400 Bad Request`（`ApiError::BadRequest`）を返す。HMAC 検証失敗時も同様
- **`total` フィールド**: cursor 方式でも `total`（合計件数）は変わらず提供する。ただし cursor ベースページネーションでは `total` は概算でもよい

## Definition of Done

- [ ] 全BDDシナリオが自動テストとして実装されパスする
- [ ] 既存の offset/limit テストがすべてグリーン
- [ ] `cargo test`（ワークスペース全体）がグリーン
- [ ] HTTP API で `curl "http://localhost:7171/api/v1/search?q=test&cursor=..."` が動作する
