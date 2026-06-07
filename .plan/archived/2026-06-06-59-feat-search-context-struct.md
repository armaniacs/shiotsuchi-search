# PBI-59: search_* 関数の引数を SearchExecutionParams 構造体に統合 (DEV-65)

## ユーザーストーリー

開発者として、`search_fts`・`search_vec`・`search_hybrid` の引数が少なくて読みやすいコードベースがほしい、なぜなら現状の15引数は可読性が低く新規機能追加時の修正コストが高いから

## ビジネス価値

- Clippy 警告（`too_many_arguments` + `type_complexity`）を一掃する
- 引数追加時に全関数シグネチャを変えずに済むようになる（メンテナンス性向上）
- 既存の `SearchRequest` 構造体を活用することで、バイナリ互換性を保ったまま改善可能

## 実装内容

`SearchExecutionParams` 構造体を導入し、`search_fts`、`search_vec`、`search_hybrid` の共通引数を束ねた。

```rust
struct SearchExecutionParams<'a> {
    db: &'a NoteDatabase,
    tokenizer: &'a JapaneseTokenizer,
    query: &'a str,
    min_score: Option<f64>,
    vault_filter: Option<&'a str>,
    tag_filter: Option<&'a str>,
    since_date: Option<&'a str>,
    user_dictionary: &'a [String],
    synonyms: &'a HashMap<String, Vec<String>>,
    fuzzy: bool,
}
```

### 現在のシグネチャ

```rust
fn search_fts(params: &SearchExecutionParams, limit: usize, cursor: Option<&Cursor>) -> Result<Vec<ChunkSearchResult>, DbError>
fn search_vec(params: &SearchExecutionParams, embedding: &[f32], limit: usize, include_embeddings: bool) -> VecSearchResult
fn search_hybrid(params: &SearchExecutionParams, embedding: &[f32], limit: usize, vec_fetch_limit: usize, alpha: Option<f64>, include_embeddings: bool) -> VecSearchResult
```

### その他の Clippy 修正

本 PBI の過程で以下の追加修正も行った：

- `search_vec` / `search_hybrid` の共通戻り値型 `VecSearchResult` を型エイリアスとして定義（`type_complexity` 解消）
- `tokenizer.rs` の `needless_range_loop` をイテレータベースに修正

## 受け入れ基準

- [x] `search_fts` の引数が 7以下になっている（現在 3引数）
- [x] `search_vec` の引数が 7以下になっている（現在 4引数）
- [x] `search_hybrid` の引数が 7以下になっている（現在 6引数）
- [x] 型の複雑さ (`type_complexity`) 警告が解消されている
- [x] `cargo clippy -p shiotsuchi-core` の警告が0件
- [x] `cargo test -p shiotsuchi-core` がグリーン（477 passed）

## テスト戦略（t_wada スタイル）

### 単体テスト（既存のまま）
- すべての検索関数は既存テストでカバーされている。リファクタリングは動作変更を伴わないため、既存テストのグリーンが十分な検証となる。

### 統合テスト（変更なし）
- `core/tests/integration_test.rs` — 変更不要

## Definition of Done

- [x] `cargo clippy -p shiotsuchi-core` が警告0
- [x] `cargo test -p shiotsuchi-core` が全テストグリーン
- [x] 公開API (`pub fn search`) のシグネチャが変わっていない
