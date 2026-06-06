# PBI-53c: core ライブラリの log:: → tracing:: 移行 + #[instrument]

## ユーザーストーリー

SRE として、インデックス処理や検索処理の実行時間と警告が構造化ログとして記録されてほしい、なぜなら現状の `log::warn!` は非構造化テキストのためアラートルールが書けないから

## ビジネス価値

- `log::warn!` のテキストログを `tracing::warn!` に移行することで、ログ集約基盤（Loki, CloudWatch 等）での構造化クエリが可能になる
- `#[tracing::instrument]` による自動 span 生成で、インデックス処理・検索処理の所要時間をトレースできる
- PBI-53d（CLI）の前提。core が `log` crate を削除することで CLI も `env_logger` を不要にできる

## BDD 受け入れシナリオ

```gherkin
Scenario: インデックス処理の span が記録される
  Given RUST_LOG=shiotsuchi_core=info が設定されている
  When shiotsuchi index を実行する
  Then stderr に index_directory span の開始・終了ログが記録される
  And vault_count フィールドが含まれる

Scenario: 検索処理の警告が構造化ログとして記録される
  Given ハイブリッド検索でベクター検索が失敗する状況
  When shiotsuchi search を実行する
  Then stderr に "Hybrid search vec component failed" の warn ログが記録される
  And log::warn ではなく tracing::warn として出力される

Scenario: log:: への依存が core から削除される
  Given core/Cargo.toml を確認する
  Then log = "0.4" の依存が存在しない
```

## 受け入れ基準

- [x] `core/Cargo.toml` から `log = "0.4"` が削除されている
- [x] `core/src/search.rs` の全 `log::warn!` が `tracing::warn!` に置き換えられている（3箇所）
- [x] `core/src/indexer.rs` の全 `log::warn!` / `log::debug!` が `tracing::warn!` / `tracing::debug!` に置き換えられている（12箇所）
- [x] `index_directory` 関数に `#[tracing::instrument]` が付与されている
- [x] `cargo test -p shiotsuchi-core` がグリーン（441 passed）
- [x] `cargo build` (ワークスペース全体) がエラーなし（cli / mcp ビルド確認済み）

## テスト戦略（t_wada スタイル）

`log::` → `tracing::` の置き換えは動作変更ではないため、既存テストがグリーンのままであることが主な検証手段。`#[instrument]` の追加もテスト対象の関数シグネチャを変えないため、既存テストへの影響はない。

追加テストは不要。ただし `cargo test` が全体でグリーンであることを必ず確認する（cli / mcp のコンパイルが通るか確認のため）。

## 実装アプローチ

### 1. `core/Cargo.toml` の変更

```toml
# 削除
log = "0.4"

# tracing は PBI-53b で既に追加済みのはず。未追加なら追加:
tracing = "0.1"
```

### 2. `core/src/search.rs` の置き換え（4箇所）

```bash
# 対象確認
grep -n "log::" core/src/search.rs
```

各 `log::warn!` を `tracing::warn!` に変更。ファイル冒頭の `use log;` または `log::` プレフィックス呼び出しを `tracing::` に変更。

### 3. `core/src/indexer.rs` の置き換え（9箇所）

```bash
# 対象確認
grep -n "log::" core/src/indexer.rs
```

各 `log::warn!` / `log::debug!` を `tracing::warn!` / `tracing::debug!` に変更。

### 4. `index_directory` への `#[instrument]` 追加

```rust
#[tracing::instrument(
    skip(db, tokenizer, config, embedder, progress),
    fields(vault_count = config.vaults.len())
)]
pub fn index_directory(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
    embedder: Option<&Embedder>,
    progress: Option<IndexProgress>,
) -> Result<...> {
```

`skip` リストは必須: `db` / `tokenizer` / `config` / `embedder` / `progress` は `Debug` を実装していないか、実装していても大きすぎるためログに含めない。`vault_count` のみ `fields` で明示する。

## 見積もり（ストーリーポイント）

3〜4時間（置き換え自体は機械的だが、`#[instrument]` の `skip` 設定で型制約を確認する必要がある）

## 技術的考慮事項

- `#[tracing::instrument]` を付けた関数の引数型はすべて `Debug` を実装している必要がある（`skip` で除外すれば不要）。`NoteDatabase`、`JapaneseTokenizer`、クロージャ型の `IndexProgress` は `Debug` 未実装のため必ず `skip` に含める
- `log` crate 削除後、`cli` と `mcp` は `core` に依存するため `cargo build` で影響が出る可能性がある。cli は `env_logger` / `log` に直接依存しているため PBI-53d まで cli は `log` を直接依存し続けて問題ない（PBI-53d で解消）
- `tracing` は `log` と API が酷似しているため置き換えは `s/log::/tracing::/g` で大部分が対応可能。ただし `log::Level` 等の型を使っている場合は別途対応が必要（本プロジェクトでは使っていないはず）
- `core` はライブラリクレートのため subscriber を初期化しない。`tracing::warn!` 等のマクロ呼び出しは subscriber が設定されていなければ no-op になる（安全）

## 実装者向け注記（ジュニア開発者必読）

### 現状コードの確認

```bash
# 置き換え対象の全 log:: 呼び出しを確認
grep -n "log::" core/src/search.rs core/src/indexer.rs

# log crate の use 宣言確認
grep -n "^use log\|extern crate log" core/src/

# index_directory のシグネチャ確認（skip リスト作成のため）
grep -n "pub fn index_directory" core/src/indexer.rs
```

### 実装手順

1. `core/Cargo.toml` から `log = "0.4"` を削除し `tracing = "0.1"` を追加（PBI-53b 未実施なら）
2. `core/src/search.rs` の `log::` → `tracing::` 一括置き換え
3. `core/src/indexer.rs` の `log::` → `tracing::` 一括置き換え
4. `core/src/indexer.rs` の `index_directory` に `#[tracing::instrument(...)]` 追加
5. `cargo build -p shiotsuchi-core` でコンパイル確認
6. `cargo test -p shiotsuchi-core` でグリーン確認
7. `cargo build` (ワークスペース全体) で cli / mcp のビルドが通ることを確認

### 落とし穴

- `log` を削除すると `core` に `log::` の呼び出しが残っている別ファイル（`db.rs` 等）がある可能性がある。`grep -rn "log::" core/src/` で全体を確認してから進める
- `#[instrument]` の `skip` リストに含め忘れた引数が `Debug` 未実装だとコンパイルエラーになる。エラーメッセージ中に「the trait `Debug` is not implemented for...」と出たら `skip` リストに追加する
- PBI-53d が未完了の状態では cli/mcp の `main.rs` がまだ `env_logger` を使っているが、これは問題ない。`tracing-subscriber` がなければ `tracing::warn!` は no-op になる

## Definition of Done

- [x] `cargo build -p shiotsuchi-core` がエラーなし
- [x] `cargo test -p shiotsuchi-core` が全テストグリーン（441 passed）
- [x] `cargo build` (ワークスペース全体) がエラーなし
- [x] `grep -rn "log::" core/src/` の結果が空（log への依存が完全に除去されている）
