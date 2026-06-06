# PBI-59: search_* 関数の引数を SearchContext 構造体に統合 (DEV-65)

## ユーザーストーリー

開発者として、`search_fts`・`search_vec`・`search_hybrid` の引数が少なくて読みやすいコードベースがほしい、なぜなら現状の15引数は可読性が低く新規機能追加時の修正コストが高いから

## ビジネス価値

- Clippy 警告 6件（`too_many_arguments` + `type_complexity`）を一掃する
- 引数追加時に全関数シグネチャを変えずに済むようになる（メンテナンス性向上）
- 既存の `SearchRequest` 構造体を活用することで、バイナリ互換性を保ったまま改善可能

## 現状の問題

`search_fts`、`search_vec`、`search_hybrid` の3関数は `SearchRequest` のフィールドを手動で展開して引数にしている。その結果、15引数まで膨れ上がり Clippy の `too_many_arguments` 警告が発生している。

```rust
// 現状: 15 arguments — too_many_arguments
fn search_hybrid(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    embedding: &[f32],
    query: &str,
    limit: usize,
    vec_fetch_limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&str>,
    tag_filter: Option<&str>,
    since_date: Option<&str>,
    user_dictionary: &[String],
    synonyms: &HashMap<String, Vec<String>>,
    fuzzy: bool,
    alpha: Option<f64>,
    include_embeddings: bool,
) -> Result<...> { ... }
```

同様に `search_fts`（11引数）、`search_vec`（9引数）も同じ問題を持つ。

## BDD 受け入れシナリオ

```gherkin
Scenario: 引数展開を SearchRequest 参照で置き換える
  Given search_fts が SearchRequest のフィールドを個別展開して11引数受け取っている
  When  それを &SearchRequest の参照に置き換える
  Then  too_many_arguments 警告が消える
  And   全ての呼び出し元が変更なしでコンパイルできる

Scenario: 内部関数特有の引数は構造体に含めずそのまま残す
  Given search_hybrid に embedding（Vec 検索用）と vec_fetch_limit が SearchRequest にない内部引数として存在する
  When  これらの引数は SearchContext 構造体に移動する
  Then  新しい SearchContext 構造体が SearchRequest のフィールドと内部引数をまとめる
```

## 受け入れ基準

- [ ] `search_fts` の引数が 7以下になっている
- [ ] `search_vec` の引数が 7以下になっている
- [ ] `search_hybrid` の引数が 7以下になっている
- [ ] 型の複雑さ (`type_complexity`) 警告が解消されている
- [ ] `cargo clippy -p shiotsuchi-core` の警告が0件
- [ ] `cargo test -p shiotsuchi-core` がグリーン

## テスト戦略（t_wada スタイル）

### 単体テスト（既存のまま）
- すべての検索関数は既存テストでカバーされている。リファクタリングは動作変更を伴わないため、既存テストのグリーンが十分な検証となる。

### 統合テスト（変更なし）
- `core/tests/integration_test.rs` — 変更不要

## 実装アプローチ

- **動作変更なしの純粋なリファクタリング**。Outside-In ではなく、コンパイラ + Clippy 主導で進める。

### Step 1: SearchExecutionParams 構造体の導入

```rust
/// Internal: parameters passed through search_fts / search_vec / search_hybrid.
/// Bundles fields from SearchRequest together with execution-specific params
/// to keep function signatures under the clippy threshold.
struct SearchExecutionParams<'a> {
    db: &'a NoteDatabase,
    tokenizer: &'a JapaneseTokenizer,
    query: &'a str,
    limit: usize,
    min_score: Option<f64>,
    vault_filter: Option<&'a str>,
    tag_filter: Option<&'a str>,
    since_date: Option<&'a str>,
    user_dictionary: &'a [String],
    synonyms: &'a HashMap<String, Vec<String>>,
    fuzzy: bool,
}
```

### Step 2: 内部関数の置き換え

```rust
fn search_fts(params: &SearchExecutionParams) -> Result<Vec<ChunkSearchResult>, DbError> {
    // ...
}
```

### Step 3: search_hybrid は SearchExecutionParams + 追加引数

```rust
fn search_hybrid(
    params: &SearchExecutionParams,
    embedding: &[f32],
    vec_fetch_limit: usize,
    alpha: Option<f64>,
    include_embeddings: bool,
) -> Result<...> {
    // ...
}
```

## 見積もり

2〜3時間（コンパイルを通すだけなら30分、Clippy 警告ゼロ確認に追加時間）

## 技術的考慮事項

- `SearchExecutionParams` は `core::search` モジュール内でのみ使用する。外部に公開しない（`pub(crate)` または非公開）
- ライフタイム注釈が正しく伝搬することを確認する。`db` と `tokenizer` のライフタイムは `SearchRequest` と同じスコープで十分
- `pub fn search` の公開シグネチャは変更しない — 外部互換性を維持

## 実装者向け注記

### 現状コードの確認

```bash
# 対象関数の引数確認
grep -n "fn search_fts\|fn search_vec\|fn search_hybrid" core/src/search.rs

# Clippy 警告の確認
cargo clippy -p shiotsuchi-core 2>&1 | grep "too_many_arguments\|type_complexity"
```

### 実装手順

1. `SearchExecutionParams` 構造体を `core/src/search.rs` に追加
2. `search_fts` の引数を置き換え、呼び出し元を調整
3. `search_vec` の引数を置き換え
4. `search_hybrid` の引数を置き換え
5. `cargo build -p shiotsuchi-core` でコンパイル確認
6. `cargo clippy -p shiotsuchi-core` で警告0確認
7. `cargo test -p shiotsuchi-core` で全テストグリーン確認

### 落とし穴

- `SearchRequest` のライフタイムより `SearchExecutionParams` が長生きしないようにする
- `embedding` と `vec_fetch_limit` は `SearchExecutionParams` には含めない — FTS 検索では不要な引数になる

## Definition of Done

- [ ] `cargo clippy -p shiotsuchi-core` が警告0
- [ ] `cargo test -p shiotsuchi-core` が全テストグリーン
- [ ] 公開API (`pub fn search`) のシグネチャが変わっていない
