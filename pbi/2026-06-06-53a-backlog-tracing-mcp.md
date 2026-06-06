# PBI-53a: MCP サーバーの構造化ログ導入（tracing-subscriber + stderr 固定）

## ユーザーストーリー

SRE として、MCP サーバーのツール呼び出しが stderr に構造化ログとして記録されてほしい、なぜなら Claude Desktop 統合時に stdout が JSON-RPC 専用である制約下で障害特定が困難だから

## ビジネス価値

- MCP サーバーの `env_logger` は現状デフォルトで stderr に出力するため即座に壊れるわけではないが、`tracing` で `with_writer(stderr)` を明示宣言することで stdout 汚染リスクをコードで保証できる
- ツール呼び出し（`search_local_notes` / `get_surrounding_context` / `index_status`）の呼び出しログが残るようになり、Claude Desktop から送られたリクエストのデバッグが可能になる
- PBI-53b/53c/53d の前提ではなく独立して完結する（mcp crate 内で閉じた変更）

## BDD 受け入れシナリオ

```gherkin
Scenario: MCP ツール呼び出しが stderr に記録される
  Given MCP サーバーが起動している
  When ツール "search_local_notes" が呼び出される
  Then stderr にツール名 "search_local_notes" が記録される
  And stdout には JSON-RPC レスポンスのみが含まれる

Scenario: RUST_LOG で出力レベルが制御できる
  Given RUST_LOG=shiotsuchi_mcp=debug が設定されている
  When MCP サーバーを起動して任意のツールを呼び出す
  Then debug レベルのログが stderr に出力される

Scenario: RUST_LOG 未設定でもサーバーが正常起動する
  Given RUST_LOG が設定されていない
  When MCP サーバーを起動する
  Then エラーなく起動し、JSON-RPC 通信が正常に機能する
```

## 受け入れ基準

- [x] `mcp/Cargo.toml` から `log` と `env_logger` が削除され `tracing` と `tracing-subscriber` が追加されている
- [x] `mcp/src/main.rs` の初期化が `tracing_subscriber::fmt().with_writer(std::io::stderr).with_ansi(false)` になっている
- [x] `spawn_rebuild` 内の `log::error!` / `log::info!` が `tracing::error!` / `tracing::info!` に置き換えられている
- [x] `mcp/src/handler/mod.rs` の各ツール呼び出しに `tracing::info!(tool = name)` が追加されている
- [x] `cargo test -p shiotsuchi-mcp` がグリーン（44 passed）
- [x] `echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | RUST_LOG=info shiotsuchi-mcp 2>/tmp/mcp.log` を実行したとき `/tmp/mcp.log` にログが記録され stdout に JSON のみが出力される

## テスト戦略（t_wada スタイル）

既存のユニットテスト（`dispatch_*`、`resolve_path_env_*`）はそのまま維持する。
ログ出力のテストは `tracing-test` crate を使わず、統合テストとして「stdout に JSON 以外が混入しないこと」を確認する形で十分（subscriber の初期化は main でのみ行うためユニットテストでは検証不要）。

## 実装アプローチ

### 1. `mcp/Cargo.toml` の依存変更

```toml
# 削除
log = "0.4"
env_logger = "0.11"

# 追加
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

### 2. `mcp/src/main.rs` の初期化置き換え

```rust
// 削除
env_logger::init();

// 追加（main() の先頭）
tracing_subscriber::fmt()
    .with_writer(std::io::stderr)
    .with_ansi(false)
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

### 3. `spawn_rebuild` 内の log:: → tracing:: 置き換え

`mcp/src/main.rs` 内の `log::error!` / `log::info!` を `tracing::error!` / `tracing::info!` に置き換える（4箇所）。

### 4. `mcp/src/handler/mod.rs` のツール呼び出しログ追加

`call_tool` の `match name` ブロック各アームに追加:

```rust
"search_local_notes" => {
    tracing::info!(tool = "search_local_notes", "MCP tool called");
    handle_search_local_notes(&ctx, args)
}
"get_surrounding_context" => {
    tracing::info!(tool = "get_surrounding_context", "MCP tool called");
    handle_get_surrounding_context(&ctx, args)
}
"index_status" => {
    tracing::info!(tool = "index_status", "MCP tool called");
    handle_index_status(&ctx, args)
}
```

## 見積もり（ストーリーポイント）

2〜3時間（機械的な置き換えが主体）

## 技術的考慮事項

- MCP サーバーは stdout が JSON-RPC プロトコル専用。`tracing_subscriber` の `with_writer(std::io::stderr)` を明示しないと `fmt()` のデフォルトが stdout になる可能性がある（現状 `env_logger` が stderr に出力しているが、`tracing_subscriber` のデフォルトは stdout）
- `with_ansi(false)` を指定しないとターミナルでない環境（Claude Desktop のログファイル等）でエスケープコードが混入する
- `shiotsuchi-core` はまだ `log` crate を使っているが、`tracing-subscriber` の `env-filter` feature には `log` 互換ブリッジが含まれるため、core 側の `log::warn!` も `tracing` のフィルタリング対象になる（PBI-53c 完了前でも機能する）

## 実装者向け注記（ジュニア開発者必読）

### 現状コードの確認

```bash
# env_logger の使用箇所確認
grep -rn "env_logger\|log::" mcp/src/

# tracing の既存使用確認（未使用のはず）
grep -rn "tracing" mcp/src/
```

### 実装手順

1. `mcp/Cargo.toml` の依存変更
2. `mcp/src/main.rs` の `use` 宣言変更（`use log::` を削除、`use tracing::` を追加）
3. `env_logger::init()` の置き換え
4. `spawn_rebuild` 内の `log::` 呼び出しを `tracing::` に変更
5. `handler/mod.rs` にツールログ追加
6. `cargo build -p shiotsuchi-mcp` でコンパイル確認
7. `cargo test -p shiotsuchi-mcp` でテスト確認

### 落とし穴

- `tracing_subscriber::fmt()` のデフォルト出力先は **stdout**。必ず `.with_writer(std::io::stderr)` を指定すること。これを忘れると Claude Desktop が JSON-RPC パースエラーを起こす
- `tracing-subscriber` の `fmt` feature はデフォルトで有効だが、`env-filter` は明示的に features に含める必要がある
- `handler/mod.rs` の `#[cfg(test)]` ブロックも `log::` を使っていないか確認する（現状は使っていないはずだが確認必須）

## Definition of Done

- [x] `cargo build -p shiotsuchi-mcp` がエラーなし
- [x] `cargo test -p shiotsuchi-mcp` が全テストグリーン（44 passed）
- [x] `log` / `env_logger` への依存が mcp/Cargo.toml から削除されている
- [x] 手動確認: `RUST_LOG=info shiotsuchi-mcp` 起動時に stderr にログが出力され、stdout には JSON のみが出力される
