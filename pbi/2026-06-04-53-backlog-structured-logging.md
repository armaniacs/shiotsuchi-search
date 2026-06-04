# PBI-53: 構造化ログ・トレーシング導入

**発端:** SRE/Ops Specialist (スコア70)
**影響:** 現状 `log::warn/info` のみでリクエストID・処理時間・コンテキスト情報が不足。本番運用時の障害特定が困難
**対処:** `tracing` crate 導入を検討
**工数:** 2-4日 (tracing全面導入), 0.5日 (最小改善)
**状態:** 未着手

## 現状

- `log` crate の `log::warn!`, `log::info!`, `log::error!` のみ使用
- HTTP サーバー: リクエストログなし（rate limit の log::warn のみ）
- MCP サーバー: ツール呼び出しログなし
- インデックス処理: 進捗表示は `indicatif` で行っているが、ログ出力なし

## BDD 受け入れシナリオ

```gherkin
Scenario: HTTP リクエストにリクエストIDが付与される
  Given HTTP サーバーが起動している
  When クライアントがリクエストを送信する
  Then レスポンスヘッダーにリクエストIDが含まれる
  And ログにリクエストIDが記録される

Scenario: 処理時間が計測される
  Given HTTP サーバーが起動している
  When クライアントがリクエストを送信する
  Then ログに処理時間が記録される

Scenario: MCP ツール呼び出しにコンテキストが付与される
  Given MCP サーバーが起動している
  When ツールが呼び出される
  Then ログにツール名とパラメータが記録される
```

## TDD アプローチ

### 方法1: tracing 導入（大規模）

#### Phase 1: テスト追加（レッド）

```rust
#[tokio::test]
async fn test_http_request_has_request_id() {
    let app = create_test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // レスポンスヘッダーにリクエストIDが含まれることを確認
    assert!(response.headers().contains_key("x-request-id"));
}

#[tokio::test]
async fn test_http_request_logs_processing_time() {
    // ログに出力されることを確認（実際のテストでは難しい）
    // 代わりに、処理時間が計測されることを確認
}
```

#### Phase 2: 実装（グリーン）

```rust
use tracing::{info, warn, error};
use tracing_subscriber;

// 初期化
fn init_tracing() {
    tracing_subscriber::fmt::init();
}

// リクエストID生成
fn generate_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
```

### 方法2: log + 手動コンテキスト（最小改善）

#### Phase 1: テスト追加（レッド）

```rust
#[test]
fn test_log_includes_context() {
    // ログにコンテキスト情報が含まれることを確認
}
```

#### Phase 2: 実装（グリーン）

```rust
// リクエストIDと処理時間を手動で付加
log::info!(
    request_id = %request_id,
    processing_time_ms = elapsed.as_millis(),
    "Request processed"
);
```

### 方法3: 一旦保留（推奨）

- 現在の規模では `log` で十分
- 本番運用が必要になった時点で方法1を導入
- **メリット**: 開発速度維持
- **デメリット**: 障害時に後から大変

## 選択肢

### 方法1: tracing 導入（大規模）

- `tracing` crate で `log` を置き換え
- `tracing-subscriber` で構造化ログ出力
- リクエストID、処理時間、スパンを自動計測
- **メリット**: 本番運用に最適、エコシステム充実
- **デメリット**: 大規模な変更、学習コスト

### 方法2: log + 手動コンテキスト（最小改善）

- 現状の `log` crate を維持
- リクエストIDと処理時間を手動で付加
- **メリット**: 変更最小、即効性
- **デメリット**: 構造化不十分

### 方法3: 一旦保留

- 現在の規模では `log` で十分
- 本番運用が必要になった時点で導入
- **メリット**: 開発速度維持
- **デメリット**: 障害時に後から大変

## 推奨

**方法3（一旦保留）**: 現在は開発段階で、本番運用予定がないため。将来的に本番運用が必要になった時点で方法1を導入。
