# PBI-50: HTTP API ページネーション対ーション対応

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

## 残り作業

1. `SearchParams` に `offset` フィールドを追加
2. `handle_search` で offset の適用を実装
3. レスポンスに `total` フィールドを追加
4. テスト追加
