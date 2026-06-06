# PBI-53: 構造化ログ・トレーシング導入（分割済み → 53a/53b/53c/53d）

> **このPBIは分割されました（2026-06-06）。**
> 以下の4枚のPBIに分割して再作成されています:
> - [PBI-53a](2026-06-06-53a-backlog-tracing-mcp.md) — MCP サーバー
> - [PBI-53b](2026-06-06-53b-backlog-tracing-http.md) — HTTP サーバー
> - [PBI-53c](2026-06-06-53c-backlog-tracing-core.md) — core ライブラリ
> - [PBI-53d](2026-06-06-53d-backlog-tracing-cli.md) — CLI
>
> このファイルは調査記録・OSS調査結果の参照用として残しています。

**発端:** SRE/Ops Specialist (スコア70)
**影響:** 現状 `log::warn/info/debug/error` のみでリクエストID・処理時間・コンテキスト情報が不足。本番運用時の障害特定が困難
**対処:** `tracing` crate 導入（方法A）
**工数:** 1日（段階的導入）
**状態:** 未着手

## 現状

- `log` crate の `log::warn!`, `log::info!`, `log::error!`, `log::debug!` を使用（`tracing` は未導入）
- HTTP サーバー: リクエストログなし（rate limit の log::warn のみ）
- MCP サーバー: ツール呼び出しログなし。`env_logger::init()` はデフォルトで stderr に出力するため現状は安全だが、`tracing` で明示的に `with_writer(stderr)` を指定する defense-in-depth が必要
- インデックス処理: 進捗表示は `indicatif` で行っているが、構造化ログなし

## OSS 調査結果

### Rust エコシステムの現状

- **ripgrep / fd**: 検索結果を stdout に出力するため診断ログ自体を持たない設計
- **meilisearch / tantivy 系アプリ**: `tracing` + `tracing-subscriber` が標準。`log` crate は軽量ライブラリ向けに残存するが、サーバー・非同期ランタイムでは `tracing` に収束している
- **axum 公式サンプル**: `tower-http` の `TraceLayer` + `SetRequestIdLayer` が canonical パターン

### MCP stdio サーバーの必須事項

MCP サーバーは stdout が JSON-RPC プロトコル専用のため、**ログは必ず stderr へ**:

```rust
tracing_subscriber::fmt()
    .with_writer(std::io::stderr)
    .with_ansi(false)  // ログファイルでエスケープコードが混入しないよう
    .with_env_filter(EnvFilter::from_default_env())
    .init();
```

### axum HTTP サーバーの canonical パターン

`tower-http`（すでに core の直接依存に含まれる、feature: `cors`）を使う:

```rust
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

Router::new()
    .route("/api/v1/search", get(search_handler))
    .layer(
        ServiceBuilder::new()
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(|req: &Request<_>| {
                        let id = req.extensions()
                            .get::<RequestId>()
                            .and_then(|id| id.header_value().to_str().ok())
                            .unwrap_or("-");
                        info_span!("request",
                            request_id = id,
                            method = %req.method(),
                            path = %req.uri().path()
                        )
                    })
                    .on_response(|res: &Response<_>, latency: Duration, _span: &Span| {
                        info!(status = res.status().as_u16(), latency_ms = latency.as_millis());
                    }),
            )
            .layer(PropagateRequestIdLayer::x_request_id()),
    )
```

### インデクサー・検索関数

`#[instrument]` マクロで自動的に span を生成し処理時間を計測:

```rust
#[tracing::instrument(skip(conn), fields(query_len = query.len()))]
pub async fn search(conn: &Connection, query: &str) -> Result<Vec<Hit>> {
    let results = fts_search(conn, query)?;
    tracing::info!(hits = results.len(), "search completed");
    Ok(results)
}
```

### tracing-subscriber 初期化（CLI / HTTP サーバー共通）

```rust
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .with_target(false)
    .compact()
    .init();
```

JSON 出力（ログ集約基盤向け）は `RUST_LOG_FORMAT=json` などで切り替え可能にする設計が一般的。

## 実装方針（段階的）

### Phase 1: MCP サーバー（優先度高）

`env_logger` はデフォルトで stderr に出力するため直ちに壊れるわけではないが、`tracing` で明示的な `with_writer(stderr)` を宣言することで防御的に対応する。あわせてツール呼び出しの構造化ログを追加する:

1. `tracing` + `tracing-subscriber` を `mcp/Cargo.toml` に追加
2. `mcp/src/main.rs` の初期化を `with_writer(stderr)` で実装
3. `mcp/src/handler.rs` の各ツールハンドラに `#[instrument]` 付与

**追加 crate**: `tracing`, `tracing-subscriber` (features: `env-filter`)

### Phase 2: HTTP サーバー

1. `core/Cargo.toml` に `tower-http` の `request-id`, `trace` features を有効化（すでに依存済みなら feature 追加のみ）
2. `core/src/server/handlers.rs` のルーター定義に `TraceLayer` + `SetRequestIdLayer` を追加
3. レスポンスヘッダーに `x-request-id` を伝播

**追加 crate**: tower-http features `request-id`, `trace`（`uuid` は tower-http の `request-id` feature が内部で持つため別途追加不要）

### Phase 3: コアライブラリ

1. `core/src/search.rs` の `search()` 関数に `#[instrument]`
2. `core/src/indexer.rs` の `index_file()` / `walk_vault()` に `#[instrument]`
3. `log` crate の呼び出しを `tracing` に置き換え（`tracing-subscriber` の `env-filter` feature に含まれる log 互換ブリッジで移行、必要に応じて `tracing-log` を直接追加）

### Phase 4: CLI

- `cli/src/main.rs` に `tracing_subscriber::fmt().compact().with_env_filter(...).init()` を追加
- `RUST_LOG=shiotsuchi=info` で動作確認できるようにする

## BDD 受け入れシナリオ

```gherkin
Scenario: HTTP リクエストにリクエストIDが付与される
  Given HTTP サーバーが起動している
  When クライアントがリクエストを送信する
  Then レスポンスヘッダー x-request-id が含まれる
  And stderr ログに request_id フィールドが記録される

Scenario: 処理時間が計測される
  Given HTTP サーバーが起動している
  When クライアントが /api/v1/search にリクエストを送信する
  Then stderr ログに latency_ms フィールドが記録される

Scenario: MCP ツール呼び出しが stderr に記録される
  Given MCP サーバーが起動している
  When ツールが呼び出される
  Then stderr にツール名とパラメータが記録される
  And stdout には JSON-RPC レスポンスのみが含まれる

Scenario: RUST_LOG で出力レベルが制御できる
  Given RUST_LOG=shiotsuchi=debug が設定されている
  When 任意のコマンドを実行する
  Then debug レベルのログが出力される
```

## 依存関係まとめ

| crate | 追加先 | 用途 |
|-------|--------|------|
| `tracing` | core, mcp, cli | マクロ (`info!`, `#[instrument]`) |
| `tracing-subscriber` | mcp, cli | subscriber 初期化 |
| `tracing-log` | core (任意) | 既存 `log::` 呼び出しの互換ブリッジ（`tracing-subscriber` の `env-filter` が内包するため原則不要、明示的な制御が必要な場合のみ追加） |
| tower-http features `request-id,trace` | core | HTTP リクエスト ID・TraceLayer（`MakeRequestUuid` が `uuid` を内包するため別途 `uuid` crate は不要） |

## 推奨

**方法A（段階的 tracing 導入）**: MCP → HTTP → Core → CLI の順で対応。  
MCP サーバーは env_logger のデフォルト動作（stderr）により現状ただちに壊れるわけではないが、明示的な `with_writer(stderr)` 宣言による defense-in-depth とツール呼び出しの構造化ログ追加が目的。  
`tower-http` はすでに core の直接依存に含まれ（feature: `cors`）、`uuid` も tower-http の `request-id` feature が内包するため、HTTP 側の依存コスト追加は実質ゼロ。
