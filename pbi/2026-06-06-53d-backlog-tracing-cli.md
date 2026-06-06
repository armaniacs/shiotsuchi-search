# PBI-53d: CLI の env_logger → tracing-subscriber 移行

## ユーザーストーリー

開発者として、`RUST_LOG=shiotsuchi=debug shiotsuchi search "query"` で debug ログが確認できるようにしてほしい、なぜなら現状の `env_logger` では core ライブラリの `tracing::` ログが出力されない（PBI-53c 完了後）から

## ビジネス価値

- PBI-53a/53b/53c で `tracing` に移行した MCP・HTTP・core のログが CLI からも確認できるようになる
- `RUST_LOG` による統一的なログレベル制御が全コンポーネントで一貫して機能するようになる
- `env_logger` 依存を cli/Cargo.toml から削除し、依存グラフを整理する

## 依存関係

**PBI-53c の完了が前提。** core が `log` crate を削除した後、cli の `env_logger` を `tracing-subscriber` に置き換えることで全体の移行が完結する。

PBI-53a/53b は独立して完了可能。

## BDD 受け入れシナリオ

```gherkin
Scenario: RUST_LOG で出力レベルが制御できる
  Given RUST_LOG=shiotsuchi=debug が設定されている
  When shiotsuchi search "test" を実行する
  Then debug レベルのログが stderr に出力される

Scenario: RUST_LOG 未設定時はデフォルトで warn 以上が出力される
  Given RUST_LOG が設定されていない
  When shiotsuchi index を実行する
  Then warn レベル以上のログのみが stderr に出力される
  And debug / info ログは出力されない

Scenario: shiotsuchi serve でリクエストログが出力される
  Given RUST_LOG=tower_http=trace が設定されている
  When shiotsuchi serve を起動して curl でリクエストを送る
  Then stderr に TraceLayer のリクエストログが出力される（PBI-53b との連携）
```

## 受け入れ基準

- [x] `cli/Cargo.toml` から `log = "0.4"` と `env_logger = "0.11"` が削除されている
- [x] `cli/Cargo.toml` に `tracing = "0.1"` と `tracing-subscriber` (features: `env-filter`) が追加されている
- [x] `cli/src/main.rs` の初期化が `tracing_subscriber::fmt().compact()...` になっている
- [x] `cargo test -p shiotsuchi` がグリーン（144 passed）
- [x] `cargo build` (ワークスペース全体) がエラーなし
- [x] `grep -rn "log::\|env_logger" cli/src/ cli/Cargo.toml` の結果が空（log への依存が完全に除去されている）

## テスト戦略（t_wada スタイル）

`env_logger` → `tracing_subscriber` の置き換えは動作変更ではないため、既存テストがグリーンのままであることが主な検証手段。subscriber の初期化は `main()` 内でのみ行うため、ユニットテストには影響しない。

手動での RUST_LOG 動作確認が主な受け入れ検証となる。

## 実装アプローチ

### 1. `cli/Cargo.toml` の変更

```toml
# 削除
log = "0.4"
env_logger = "0.11"

# 追加
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

### 2. `cli/src/main.rs` の初期化置き換え

現在の実装（`cli/src/main.rs` 約130行目付近）:

```rust
// 削除（2行）
let env = env_logger::Env::default().default_filter_or("warn");
env_logger::Builder::from_env(env).init();
```

置き換え後:

```rust
tracing_subscriber::fmt()
    .with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
    )
    .compact()
    .with_target(false)
    .init();
```

`with_target(false)` によりモジュールパスが省略されコンパクトな出力になる。`compact()` は1行フォーマットを選択する。

### 3. `use` 宣言の整理

`cli/src/main.rs` から `use log;` や `use env_logger;` の宣言を削除する（あれば）。

## 見積もり（ストーリーポイント）

1〜2時間（最もシンプルな変更）

## 技術的考慮事項

- `tracing_subscriber::EnvFilter::from_default_env()` は `RUST_LOG` が未設定の場合にパニックする。`try_from_default_env().unwrap_or_else(...)` でフォールバックを設定すること
- `env_logger` の `default_filter_or("warn")` に相当するのが `unwrap_or_else(|_| EnvFilter::new("warn"))` のパターン
- `shiotsuchi serve` は HTTP サーバーを起動するが、PBI-53b で追加した `TraceLayer` のログは `tracing_subscriber` が初期化されることで初めて出力される。この PBI の完了で PBI-53b の効果が CLI からも確認できるようになる
- cli に直接 `log::` 呼び出しがあれば `tracing::` に置き換える（`grep -rn "log::" cli/src/` で確認）

## 実装者向け注記（ジュニア開発者必読）

### 現状コードの確認

```bash
# env_logger / log の使用箇所確認
grep -rn "env_logger\|log::" cli/src/

# tracing の既存使用確認
grep -rn "tracing" cli/src/

# 初期化箇所の特定
grep -n "env_logger::Builder\|env_logger::init\|env_logger::Env" cli/src/main.rs
```

### 実装手順

1. PBI-53c が完了していることを確認（`cargo test -p shiotsuchi-core` がグリーン）
2. `cli/Cargo.toml` の依存変更
3. `cli/src/main.rs` の初期化コード置き換え
4. `cli/src/` 内に他の `log::` 呼び出しがあれば `tracing::` に置き換え
5. `cargo build -p shiotsuchi` でコンパイル確認
6. `cargo test -p shiotsuchi` でグリーン確認
7. `cargo test` (ワークスペース全体) でグリーン確認
8. 手動確認: `RUST_LOG=shiotsuchi_core=debug shiotsuchi search "test" 2>&1 | head -20` でログ出力確認

### 落とし穴

- `tracing_subscriber::EnvFilter::from_default_env()` を使うと `RUST_LOG` 未設定時にパニックする。必ず `try_from_default_env()` + `unwrap_or_else` のパターンを使うこと
- `cli/src/commands/` 配下のファイルに `log::` 呼び出しが残っていると `cargo build` でエラーになる。`grep -rn "log::" cli/src/` で全ファイルを確認する
- PBI-53c が未完了の状態でこの PBI を進めると、core がまだ `log` を使っているため整合性が取れない。必ず 53c 完了後に着手すること

## Definition of Done

- [x] `cargo build` (ワークスペース全体) がエラーなし
- [x] `cargo test -p shiotsuchi` が全テストグリーン（144 passed）
- [x] `grep -rn "env_logger\|^log = " cli/Cargo.toml cli/src/` の結果が空
- [x] 手動確認: `RUST_LOG=info shiotsuchi search "test"` で stderr にログが出力される（CLI でテスト済み）
- [x] 手動確認: `RUST_LOG` 未設定でも `shiotsuchi search "test"` が正常に動作する（`unwrap_or_else` でフォールバック済み）
