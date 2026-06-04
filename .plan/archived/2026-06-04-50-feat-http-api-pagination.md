# PBI-50: HTTP API ページネーション対応

**発端:** API & Contract Negotiator - 検索APIにページネーションがない
**影響:** `POST /api/v1/search` と `GET /api/v1/list` のページネーション
**対処:** offset/limit クエリパラメータの追加
**工数:** 0.5日（残り実装部分のみ）
**状態:** 部分実装済み

## 実装状況

### ✅ 完了済み

- **`GET /api/v1/list`**: offset/limit パラメータ対応済み
  - `offset=0&limit=5` → ページネーション結果返却
  - レスポンスに `total`, `offset`, `limit` を含む
  - テスト `test_list_pagination_offset_limit`, `test_list_pagination_second_page` が存在

- **`POST /api/v1/search`**: limit パラメータ対応済み
  - `limit` パラメータで結果数を制限（デフォルト20、最大200）
  - `deserialize_clamped_limit` で超過時に200にクランプ
  - テスト `test_search_with_limit`, `test_search_limit_clamped_silently` が存在

### ❌ 未実装

- **`POST /api/v1/search` の offset パラメータ**: 現在は limit のみ。offset によるページングができない
- **`POST /api/v1/search` の total カウント**: レスポンスに全件数が含まれていない

## BDD 受け入れシナリオ

```gherkin
Scenario: search API で offset パラメータが動作する
  Given インデックスに100件のノートが存在する
  When POST /api/v1/search?q=test&offset=10&limit=5 をリクエストする
  Then レスポンスの results が5件以下である
  And レスポンスの offset が10である
  And レスポンスの total が100である

Scenario: search API で offset がデフォルト0になる
  Given インデックスに100件のノートが存在する
  When POST /api/v1/search?q=test&limit=5 をリクエストする
  Then レスポンスの offset が0である

Scenario: search API で offset が総数を超える場合
  Given インデックスに100件のノートが存在する
  When POST /api/v1/search?q=test&offset=200&limit=5 をリクエストする
  Then レスポンスの results が空である
  And レスポンスの total が100である

Scenario: search API の limit が200でクランプされる
  Given インデックスに1000件のノートが存在する
  When POST /api/v1/search?q=test&limit=9999 をリクエストする
  Then レスポンスの results が200件以下である
  And レスポンスの limit が200である
```

## TDD アプローチ

### Phase 1: テスト追加（レッド）

1. **offset パラメータのテスト追加**:
```rust
#[tokio::test]
async fn test_search_with_offset() {
    let app = create_test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=test&offset=10&limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert_eq!(json["offset"], 10);
    assert!(json["results"].as_array().unwrap().len() <= 5);
}
```

2. **total カウントのテスト追加**:
```rust
#[tokio::test]
async fn test_search_response_includes_total() {
    let app = create_test_app().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=test&limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let json: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(json["total"].is_number(), "response must include total count");
}
```

### Phase 2: 実装（グリーン）

1. `SearchParams` に `offset` フィールドを追加
2. `handle_search` で offset の適用を実装
3. レスポンスに `total` フィールドを追加

### Phase 3: リファクタリング

- offset のクランプ処理（負の値、総数超過）
- total の計算最適化

## 残り作業

1. `SearchParams` に `offset` フィールドを追加
2. `handle_search` で offset の適用を実装
3. レスポンスに `total` フィールドを追加
4. テスト追加
