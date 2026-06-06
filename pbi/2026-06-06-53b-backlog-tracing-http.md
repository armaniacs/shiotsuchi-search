# PBI-53b: HTTP サーバーへの TraceLayer + リクエストID 導入

## ユーザーストーリー

SRE として、HTTP API の各リクエストに一意のリクエストIDと処理時間が記録されてほしい、なぜなら本番障害時に「どのリクエストが遅かったか」「どのリクエストでエラーが出たか」を特定できないから

## ビジネス価値

- `x-request-id` ヘッダーにより、クライアント側ログとサーバー側ログを突き合わせられる
- `latency_ms` フィールドにより、レスポンスタイム劣化の検出が可能になる
- `tower-http` はすでに core の直接依存（feature: `cors`）に含まれるため、依存追加コストは feature フラグ2つのみ
- PBI-53a/53c/53d と独立して完結する（core/server/ 内で閉じた変更）

## BDD 受け入れシナリオ

```gherkin
Scenario: HTTP リクエストにリクエストIDが付与される
  Given HTTP サーバーが起動している
  When クライアントが /api/v1/health にリクエストを送信する
  Then レスポンスヘッダーに x-request-id が含まれる
  And stderr ログに request_id フィールドが記録される

Scenario: クライアント指定の x-request-id が伝播される
  Given HTTP サーバーが起動している
  When クライアントが x-request-id: my-trace-123 ヘッダー付きでリクエストを送信する
  Then レスポンスヘッダーの x-request-id が my-trace-123 である

Scenario: 処理時間が計測される
  Given HTTP サーバーが起動している
  When クライアントが /api/v1/search にリクエストを送信する
  Then stderr ログに latency_ms フィールドが記録される
  And status フィールドに HTTP ステータスコードが記録される
```

## 受け入れ基準

- [ ] `core/Cargo.toml` の `tower-http` features に `request-id` と `trace` が追加されている
- [ ] `core/Cargo.toml` に `tracing = "0.1"` が追加されている
- [ ] `create_router` に `SetRequestIdLayer` / `TraceLayer` / `PropagateRequestIdLayer` が組み込まれている
- [ ] `cargo test -p shiotsuchi-core` がグリーン（既存の HTTP ハンドラーテストを含む）
- [ ] `curl -i http://localhost:7171/api/v1/health` のレスポンスヘッダーに `x-request-id` が含まれる
- [ ] `RUST_LOG=tower_http=trace shiotsuchi serve` 起動時に stderr にリクエストログが出力される

## テスト戦略（t_wada スタイル）

`create_router` のテストは `tower::ServiceExt` を使った既存パターンで拡張する。

```rust
// 追加テスト例
#[tokio::test]
async fn test_response_has_request_id_header() {
    let (router, _tmp) = setup_test_router();
    let response = router
        .oneshot(Request::builder().uri("/api/v1/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(response.headers().contains_key("x-request-id"));
}
```

既存の `test_health_returns_ok` 等は影響を受けないため変更不要。

## 実装アプローチ

### 1. `core/Cargo.toml` の変更

```toml
# 変更前
tower-http = { version = "0.6", features = ["cors"] }

# 変更後
tower-http = { version = "0.6", features = ["cors", "request-id", "trace"] }
tracing = "0.1"
```

`uuid` crate の別途追加は不要（`tower-http` の `request-id` feature が内包する）。

### 2. `core/src/server/handlers.rs` の `create_router` 変更

```rust
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};

pub fn create_router(state: Arc<AppState>, config: &ShiotsuchiConfig) -> Router {
    use crate::server::cors::create_cors_layer;
    let cors = create_cors_layer(&config.server);

    let protected = Router::new()
        // ... 既存のルート定義（変更なし）
        .layer(axum::middleware::from_fn(auth_middleware));

    let public = Router::new()
        // ... 既存のルート定義（変更なし）

    Router::new()
        .merge(protected)
        .merge(public)
        .layer(cors)
        .layer(axum::extract::Extension(state.clone()))
        .layer(axum::extract::Extension(config.clone()))
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(
                    TraceLayer::new_for_http()
                        .make_span_with(|req: &axum::http::Request<_>| {
                            let id = req
                                .extensions()
                                .get::<tower_http::request_id::RequestId>()
                                .and_then(|id| id.header_value().to_str().ok())
                                .unwrap_or("-");
                            tracing::info_span!(
                                "request",
                                request_id = id,
                                method = %req.method(),
                                path = %req.uri().path()
                            )
                        })
                        .on_response(
                            |res: &axum::http::Response<_>,
                             latency: Duration,
                             _span: &tracing::Span| {
                                tracing::info!(
                                    status = res.status().as_u16(),
                                    latency_ms = latency.as_millis()
                                );
                            },
                        ),
                )
                .layer(PropagateRequestIdLayer::x_request_id()),
        )
        .with_state(state)
}
```

## 見積もり（ストーリーポイント）

2〜3時間（実装はシンプル。テスト追加に時間がかかる）

## 技術的考慮事項

- `SetRequestIdLayer` はリクエストに UUID を付与し、`PropagateRequestIdLayer` はそれをレスポンスヘッダーに伝播する。両方必要
- レイヤーの適用順序が重要: `SetRequestIdLayer` → `TraceLayer` → `PropagateRequestIdLayer` の順にネストする必要がある（`ServiceBuilder` で記述すると適用順が直感的）
- `TraceLayer` のデフォルト span は `tower_http` crate の tracing subscriber を使う。`RUST_LOG=tower_http=trace` で詳細ログが出る
- 既存の `auth_middleware` は `axum::middleware::from_fn` で実装されており、`TraceLayer` との競合はない
- CLI の `shiotsuchi serve` コマンドは `env_logger` を初期化するが、PBI-53d 完了前は `tracing_subscriber` が初期化されないため TraceLayer のログが出力されない。これは意図した動作（53d で解消される）

## 実装者向け注記（ジュニア開発者必読）

### 現状コードの確認

```bash
# tower-http の現在の feature 確認
grep -n "tower-http" core/Cargo.toml

# create_router の現在の実装確認
grep -n "create_router\|ServiceBuilder\|TraceLayer" core/src/server/handlers.rs

# tracing の既存使用確認（未使用のはず）
grep -rn "tracing::" core/src/server/
```

### 実装手順

1. `core/Cargo.toml` の feature 追加
2. `core/src/server/handlers.rs` の `use` 宣言追加
3. `create_router` 末尾の `.with_state(state)` 前に layer 追加
4. `cargo build -p shiotsuchi-core` でコンパイル確認
5. `x-request-id` ヘッダー確認テストを追加
6. `cargo test -p shiotsuchi-core` でグリーン確認

### 落とし穴

- `ServiceBuilder::new()` で複数レイヤーを重ねる場合、記述順と適用順が **逆** になる（`tower` の仕様）。`SetRequestIdLayer` を `TraceLayer` より先に書くことで、span 生成時に request_id が取得できる
- `tower_http::request_id::RequestId` は `req.extensions()` から取得する。`unwrap_or("-")` のフォールバックは必須（SetRequestId より前に span が生成される場合があるため）
- 既存テストの `setup_test_router()` が使う `create_router` にもレイヤーが追加されるため、テストが遅くなる場合がある（UUID 生成のため）。許容範囲内

## Definition of Done

- [ ] `cargo build -p shiotsuchi-core` がエラーなし
- [ ] `cargo test -p shiotsuchi-core` が全テストグリーン
- [ ] `x-request-id` ヘッダーの存在を確認するテストが追加されている
- [ ] 手動確認: `RUST_LOG=tower_http=trace shiotsuchi serve` 起動後に `curl http://localhost:7171/api/v1/health` を実行すると stderr にリクエストログが出力される
