# PBI-54: Embedding API コスト上限とフォールバック機構

**発端:** FinOps Consultant (スコア70)
**影響:**
1. Embedding API にコスト上限がない
2. API障害時にFTS5にフォールバックする機構がない
3. vec_chunks の定期的な圧縮・削除オプションがない
**対処:** コスト上限設定、フォールバック、圧縮オプション
**工数:** 3-5日
**状態:** 部分実装済み

## 実装状況

### ✅ 完了済み

- **HTTP API レート制限**: スライディングウィンドウ（30 req/s）で API 呼び出しを制限
  - `core/src/server/handlers.rs` の `check_rate_limit()`
  - 429 Too Many Requests エラー返却

### ❌ 未実装

#### 1. Embedding API コスト上限

- **問題**: `embedder.embed_batch()` にコスト上限なし。大量テキスト埋め込みで予期せぬコスト発生
- **対処案**: 
  - 月間/日次の埋め込み回数上限を設定に追加
  - 上限到達時にインデックス処理を停止し、警告を出力

#### 2. API 障害時のフォールバック

- **問題**: ONNX/API 埋め込みが失敗した場合、Vec モードでは検索不可
- **対処案**:
  - API 障害時に FTS5 キーワード検索にフォールバック
  - 現状: `SearchMode::Vec` で埋め込み失敗時はエラー返却
  - 改善: `SearchMode::Hybrid` で Vec 部分をスキップし FTS のみで検索

#### 3. vec_chunks 圧縮

- **問題**: vec_chunks が無限に増大する可能性
- **対処案**:
  - 未参照の埋め込みベクトルの削除（`delete_orphaned_embeddings`）
  - 定期的な VACUUM

## 残り作業の優先順位

1. **API フォールバック**（高）: Hybrid モードでの Vec 部分スキップ機能
2. **コスト上限**（中）: 設定项目の追加とインデックス処理の制御
3. **vec_chunks 圧縮**（低）: 定期メンテナンスコマンド
