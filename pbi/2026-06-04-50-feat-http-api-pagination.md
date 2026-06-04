# PBI-50: HTTP API ページネーション対応

**発端:** API & Contract Negotiator - 検索APIにページネーションがない
**影響:** `POST /api/v1/search` と `GET /api/v1/list` に offset/limit パラメータなし。大量結果の取得が非効率
**対処:** offset/limit クエリパラメータを追加し、`list` は total/offset/limit をレスポンスに含める
**工数:** 1-2日
