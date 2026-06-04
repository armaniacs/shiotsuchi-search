# PBI-46: コード健全性の小粒修正

## ユーザーストーリー
開発者として、公開APIの可視性が適切でありCORSの設定がセキュアであるコードベースを維持したい、なぜなら誤った公開APIは内部結合を増やし、過剰に緩いCORSはセキュリティリスクとなるから

## ビジネス価値
- コードベースの保守性向上（不要な公開APIの削減）
- セキュリティ posture の改善（CORS 最小権限）
- テクニカルデットの解消

## 発端
Checking Team レビュー（2回の指摘の積み残し）:

1. **`upsert_file_cache` visibility**（`plans/2026-06-01-2146-review-checking-team-2.md` Medium）: `core/src/db.rs` の `upsert_file_cache` が `pub` のまま。doc comment で注意喚起済みだが visibility 制限は未実施。
2. **CORS AllowHeaders::any()**（`plans/2026-06-03-1354-review-serve-pdf.md` Low）: `core/src/server/cors.rs` で `AllowHeaders::any()` を使用。GET-only API なので実害は軽微だが、必要最小限のヘッダーに制限すべき。

## 前提条件
- `core/src/db.rs` に `upsert_file_cache` が存在
- `core/src/server/cors.rs` に CORS レイヤーが存在

## BDD 受け入れシナリオ

```gherkin
Scenario: upsert_file_cache が pub(crate) に制限される
  Given core/src/db.rs の upsert_file_cache
  When visibility を pub(crate) に変更する
  Then 外部クレート（cli, mcp, e2e）からの直接アクセスがコンパイルエラーになる
  And コアクレート内の全呼び出し元は変更不要

Scenario: CORS AllowHeaders が最小限に制限される
  Given cors.rs の AllowHeaders::any()
  When 必要最小限のヘッダー（Content-Type, Authorization）に制限する
  Then 既存の全 API 呼び出しが正常に動作する
```

## 受け入れ基準
- [ ] `upsert_file_cache` が `pub(crate)` に変更されている
- [ ] 外部クレートで `upsert_file_cache` を直接呼び出している箇所があれば、適切な代替メソッドに置き換えられている
- [ ] CORS AllowHeaders が `any()` から `list(["content-type", "authorization"])` に変更されている
- [ ] 全テストがパスする

## テスト戦略（TDD）

### コンパイル時検証
- 別クレートからの `upsert_file_cache` 呼び出しがコンパイルエラーになることを確認（手動）

### 既存テストの確認
- CORS テストが引き続きパスすること
- upsert_file_cache を間接的に呼ぶ全テストがパスすること

## 実装アプローチ

### 1. upsert_file_cache visibility
```rust
// core/src/db.rs
// Before: pub fn upsert_file_cache(...)
// After:  pub(crate) fn upsert_file_cache(...)
```

呼び出し元の確認:
```bash
grep -rn "upsert_file_cache" cli/src/ mcp/src/ e2e/
```
- 外部クレートから呼ばれている場合 → その処理をラップする public メソッドを `NoteDatabase` に追加するか、呼び出し側をリファクタリング
- コアクレート内のみ → 変更のみで完了

### 2. CORS AllowHeaders 制限
```rust
// core/src/server/cors.rs
use tower_http::cors::AllowHeaders;

// Before: .allow_headers(AllowHeaders::any())
// After:  .allow_headers(AllowHeaders::list(["content-type", "authorization"]))
```

## 見積もり
1 ポイント（半日）

## 技術的考慮事項
- `upsert_file_cache` が外部から呼ばれている場合、コンパイルエラーになる。事前に grep で確認すること
- CORS の `AllowHeaders::list()` は case-insensitive なので lowercase で指定して良い
- Content-Type と Authorization 以外に fetch リクエストで送信されるヘッダー（Accept, X-Requested-With 等）は simple header / CORS-safelisted なので明示不要

## 実装者向け注記

### 現状コードの確認
```bash
# upsert_file_cache の定義
grep -n "pub fn upsert_file_cache" core/src/db.rs

# 外部からの呼び出し
grep -rn "upsert_file_cache" cli/ mcp/ e2e/

# CORS レイヤー
grep -n "AllowHeaders" core/src/server/cors.rs
```

### 実装手順
1. `core/src/db.rs` で `pub fn upsert_file_cache` → `pub(crate) fn upsert_file_cache` に変更
2. コンパイルして外部クレートからの呼び出しがないことを確認
3. `core/src/server/cors.rs` で `AllowHeaders::any()` → `AllowHeaders::list(...)` に変更
4. `make test` で全テストパス確認

### 落とし穴
- `pub(crate)` に変更後、外部テスト（`e2e/`）がコンパイルできない場合は、テスト用の public ラッパーを `#[cfg(test)]` で追加するか、テスト対象を indirect な検証に変更する
- この PBI は「最小の変更で最大の効果」を狙う。過剰なリファクタリングはしない
