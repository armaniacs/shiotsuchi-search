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

## BDD 受け入れシナリオ

```gherkin
Scenario: Embedding API コスト上限が設定されている
  Given embedding_cost_limit が1000に設定されている
  When 1001個目の埋め込みリクエストが来たら
  Then エラーが返される
  And ログに警告が記録される

Scenario: API 障害時に FTS5 にフォールバックする
  Given Embedding API が障害を起こしている
  When Hybrid モードで検索する
  Then FTS5 のみで検索が実行される
  And 結果が返される

Scenario: vec_chunks のオーファンが削除される
  Given chunks テーブルに存在しない vec_chunks エントリが存在する
  When delete_orphaned_embeddings を呼び出す
  Then オーファンエントリが削除される
```

## TDD アプローチ

### Phase 1: API フォールバック（高優先度）

#### テスト追加（レッド）

```rust
#[tokio::test]
async fn test_hybrid_search_falls_back_to_fts_on_embedder_failure() {
    // 埋め込みが失敗した場合に FTS5 のみで検索されることを確認
    let app = create_test_app_with_failing_embedder().await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/search?q=test&mode=hybrid")
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

    // FTS5 の結果が返されることを確認
    assert!(json["results"].as_array().unwrap().len() > 0);
}
```

#### 実装（グリーン）

```rust
// search.rs
fn search_hybrid_with_fallback(
    query: &str,
    embedder: &Option<Embedder>,
    ...
) -> Vec<ChunkSearchResult> {
    let vec_results = if let Some(emb) = embedder {
        match search_vec(query, emb, ...) {
            Ok(results) => results,
            Err(_) => {
                log::warn!("Vec search failed, falling back to FTS only");
                vec![]
            }
        }
    } else {
        vec![]
    };

    let fts_results = search_fts(query, ...);
    compute_rrf(&fts_results, &vec_results, limit, k)
}
```

### Phase 2: コスト上限（中優先度）

#### テスト追加（レッド）

```rust
#[test]
fn test_embedding_cost_limit_enforced() {
    let config = EmbeddingConfig {
        cost_limit: Some(1000),
        ..Default::default()
    };
    let embedder = Embedder::new(&config).unwrap();

    // 1000回目の埋め込みは成功
    for _ in 0..1000 {
        assert!(embedder.embed("test").is_ok());
    }

    // 1001回目はエラー
    assert!(embedder.embed("test").is_err());
}
```

#### 実装（グリーン）

```rust
// embedder.rs
impl Embedder {
    pub fn embed(&self, text: &str) -> Result<Vec<f32>, EmbedderError> {
        if let Some(limit) = self.config.cost_limit {
            if self.embed_count.load(Ordering::Relaxed) >= limit {
                return Err(EmbedderError::CostLimitExceeded);
            }
        }
        self.embed_count.fetch_add(1, Ordering::Relaxed);
        // 実際の埋め込み処理
    }
}
```

### Phase 3: vec_chunks 圧縮（低優先度）

#### テスト追加（レッド）

```rust
#[test]
fn test_delete_orphaned_embeddings() {
    let db = NoteDatabase::open_in_memory().unwrap();

    // chunks に存在しない vec_chunks エントリを作成
    db.conn.execute_batch("
        INSERT INTO vec_chunks (chunk_id, embedding) VALUES (999, X'00000000');
    ").unwrap();

    // オーファン削除
    let deleted = db.delete_orphaned_embeddings().unwrap();
    assert_eq!(deleted, 1);

    // 削除されたことを確認
    let count: i64 = db.conn.query_row(
        "SELECT COUNT(*) FROM vec_chunks WHERE chunk_id = 999",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(count, 0);
}
```

#### 実装（グリーン）

```rust
// db.rs
pub fn delete_orphaned_embeddings(&self) -> Result<usize, DbError> {
    let deleted = self.write_conn.borrow().execute(
        "DELETE FROM vec_chunks WHERE chunk_id NOT IN (SELECT id FROM chunks)",
        [],
    )?;
    Ok(deleted)
}
```

## 残り作業の優先順位

1. **API フォールバック**（高）: Hybrid モードでの Vec 部分スキップ機能
2. **コスト上限**（中）: 設定項目の追加とインデックス処理の制御
3. **vec_chunks 圧縮**（低）: 定期メンテナンスコマンド
