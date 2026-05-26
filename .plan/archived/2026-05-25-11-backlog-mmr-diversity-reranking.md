# PBI: MMR（Maximal Marginal Relevance）による検索結果多様化

## ユーザーストーリー
セマンティック検索を使うユーザーとして、似たようなノートが上位を独占しないようにしたい、なぜなら毎日の日報や同プロジェクトの議事録が大量にヒットして有益な情報が埋もれるから

## ビジネス価値
- 検索結果の多様性を向上し、異なる観点のノートを発見しやすくする
- セマンティック検索の実用性を大幅改善

## BDD 受け入れシナリオ

```gherkin
Scenario: 類似ノートが上位を独占しない
  Given 同じプロジェクトの議事録が 50 件存在する
  When ユーザーが `shiotsuchi dive --mmr "プロジェクト"` を実行する
  Then 議事録以外の関連ノートも上位に混在して表示される

Scenario: --mmr なしでは従来のスコア順
  When ユーザーが `shiotsuchi dive "プロジェクト"` を実行する（--mmr なし）
  Then スコア降順で結果が返される
```

## 受け入れ基準
- [x] `--mmr` フラグで MMR リランキングを有効化できる
- [x] MMR の lambda 値を設定で調整できる
- [x] デフォルトは従来のスコア順

## 見積もり
5 ポイント

## 技術的考慮事項
- セマンティックベクトルが必要なため Fix-2（semantic feature）が前提
- 影響ファイル: `core/src/search.rs`

---

## ⚠️ 実装者向け注記

### MMR アルゴリズムの概要

MMR（Maximal Marginal Relevance）は以下のスコアで各ドキュメントを選択する：

```
MMR(d) = lambda * Sim(d, query) - (1 - lambda) * max(Sim(d, already_selected))
```

- `lambda` が 1 に近いほどクエリとの関連性重視
- `lambda` が 0 に近いほど多様性重視
- `already_selected` は既に選ばれたドキュメントのセット

### 実装手順

1. **`core/src/db.rs` にチャンクのベクトルを取得するメソッドを追加する**  
   `get_chunk_vectors(ids: &[i64]) -> Vec<(i64, Vec<f32>)>`

2. **`core/src/search.rs` に `mmr_rerank` 関数を実装する**：
   ```rust
   fn mmr_rerank(
       results: Vec<ChunkSearchResult>,
       vectors: &HashMap<i64, Vec<f32>>,
       lambda: f64,
       limit: usize,
   ) -> Vec<ChunkSearchResult>
   ```

3. **コサイン類似度関数を実装する**（既存実装があれば再利用）

4. **`--mmr` フラグと `--lambda` フラグを `DiveArgs` に追加する**

### 落とし穴

- MMR は O(n²) の計算量。候補数が多い場合（1000件以上）はパフォーマンスに注意。  
  `search_vec` で取得する候補数を制限（limit * 3 程度）してから MMR を適用する。
- ベクトルが DB に保存されている場合のみ実装可能。`vec` テーブルにベクトルが格納されているか確認：
  ```bash
  grep -n "vec\|vector\|embedding" core/src/db.rs | head -20
  ```

## Definition of Done
- [x] MMR リランキングのテストがパスする
- [ ] コードレビュー完了

---

## 第2フェーズ: 性能改善（A + B + C）

初回実装後、以下の3つの最適化を追加適用する。

### A: 類似度行列の事前計算

**現状の問題**: `mmr_rerank()` の選択ループ内で、残り候補 × 選択済み要素の `cosine_similarity()` を毎イテレーション計算している。同じ候補ペアの類似度が選択が進むたびに再計算される。

正確な計算量: 選択ループ i 回目に残り `(n - i)` 候補 × `i` 選択済みを計算 → 合計は **O(n² · limit / 2)**（n=60, limit=20 で約 34,800 回）。

**改善**: 選択ループに入る前に全候補間の n×n 対称行列を1回だけ計算する。選択ループ内は行列のルックアップのみ。
計算量は **O(n²/2 + limit · n)**（約 1,770 回 + 1,200 回のルックアップ）。

```rust
// Before: 選択ループ内で毎回再計算（同じペアを繰り返し計算）
for (i, candidate) in candidates.iter().enumerate() {
    for selected in &selected {
        candidate_vectors.get(&selected.chunk_id)
            .and_then(|sv| candidate_vectors.get(&candidate.chunk_id)
                .map(|cv| cosine_similarity(sv, cv)))  // 毎回 HashMap ルックアップ + 計算
    }
}

// After: 事前計算した行列をインデックスでルックアップ
let n = candidates.len();
let sim_matrix: Vec<f32> = precompute_pairwise_flat(&candidates, &candidate_vectors);
// sim_matrix[i * n + j] = cosine_similarity(candidates[i], candidates[j])
for (i, candidate) in candidates.iter().enumerate() {
    for sel_idx in &selected_indices {
        sim_matrix[i * n + sel_idx]  // O(1) ルックアップ、HashMap なし
    }
}
```

> **実装注意**: 行列は `Vec<f32>` のフラット配列（`n×n`）で持つ。`Vec<Vec<f32>>` にすると inner Vec の heap allocation が n 回発生してキャッシュに不利。対称行列なので `j > i` の部分だけ計算して両方向に書き込む（実質 n²/2 回の `cosine_similarity` 呼び出し）。

### B: 候補プールの拡大

**現状の問題**: `search_vec(limit)` の結果だけを MMR に渡している。MMR は多様な結果を promote するために limit より多い候補が必要。さらに `search_hybrid()` は内部で `search_vec(limit * 2)` を呼ぶが、MMR 適用後に `search()` で `limit` に再度絞り込むため、Hybrid モードでは候補プールが事実上 `limit * 2` 止まりになっている。

**改善**: MMR 有効時、`search_vec` / `search_hybrid` 内部の vec 候補取得を `limit * MMR_POOL_MULTIPLIER`（定数: 3）に拡大する。`search()` からフラグを受け取り、呼び出し側で候補数を制御する。

```
Vec モード（変更前）:   vec_search(limit=20) → mmr_rerank(20 candidates, limit=20)
Vec モード（変更後）:   vec_search(limit=60) → mmr_rerank(60 candidates, limit=20)

Hybrid モード（変更前）: vec_search(limit=40) → RRF → mmr_rerank(~40 candidates, limit=20)
Hybrid モード（変更後）: vec_search(limit=60) → RRF → mmr_rerank(60 candidates, limit=20)
```

> **実装注意**: `search_hybrid()` では RRF 後の候補数は FTS と Vec の union になるため、単純に `limit * 3` すると FTS 側の候補も膨らんで FTS クエリが遅くなる。Vec 側のみ拡大（`limit * 3`）し、FTS 側は `limit * 2` のままにする。

### C: クエリベクトルの二重計算排除

**現状の問題**: `emb.embed(query)` が 2 箇所で呼ばれ、ONNX 推論が2回実行される：

1. `search_vec()` 内部 — KNN 検索のため（`embedder.embed(query)` → `db.vec_search(&embedding, ...)`）
2. `search()` MMR ブロック — クエリ類似度計算のため（`emb.embed(query)` → `mmr_rerank(..., &query_vec, ...)`）

Hybrid モードでは `search_hybrid()` → `search_vec()` でも1回呼ばれるため、計2回。

**改善**: `search()` で1回だけ `emb.embed(query)` を実行し、`Vec<f32>` を下位関数に渡す。

```rust
// search() の先頭で1回 embed
let precomputed_vec: Option<Vec<f32>> = if mmr || matches!(effective_mode, SearchMode::Vec | SearchMode::Hybrid) {
    embedder.and_then(|e| e.embed(query).ok())
} else {
    None
};

// search_vec / search_hybrid に渡す
fn search_vec(
    db: &NoteDatabase,
    query: &str,
    precomputed_embedding: &[f32],   // embedder を受け取らない
    limit: usize,
    ...
) -> Result<Vec<ChunkSearchResult>, DbError> {
    let hits = db.vec_search(precomputed_embedding, limit, vault_filter)?;
    ...
}
```

> **実装注意**: `search_vec` のシグネチャ変更は `search_hybrid` も連鎖して変わる。`mcp/src/handler.rs` と `cli/src/commands/dive.rs` は `search()` 経由のため直接影響なし。ただし `search_vec` を直接呼んでいる箇所（`search_hybrid` 内）を漏れなく更新すること。

### 追加最適化: vec_search がベクトルも返す（DB ラウンドトリップ削減）✅ 実装済み

**実装内容**: sqlite-vec 0.1 の vec0 virtual table は KNN クエリ時に `xColumn` ハンドラ（`vec0Column_knn`）が vector カラムへの `SELECT` をサポートすることをソースコード（sqlite-vec.c:7710）で確認。`db::vec_search()` の戻り値を `Vec<(i64, f64, Vec<f32>)>` に変更し、MMR の `get_chunk_vectors()` 呼び出しを廃止。

### 影響ファイル

| ファイル | 変更内容 |
|---------|---------|
| `core/src/db.rs` | `vec_search()` 戻り値に embedding 追加（sqlite-vec が対応している場合のみ） |
| `core/src/search.rs` | `search_vec()` / `search_hybrid()` のシグネチャ変更（precomputed_embedding）、`mmr_rerank()` に類似度フラット行列、候補プール拡大フラグ |
| `core/tests/integration_test.rs` | 統合テスト更新 |

### 実装順序の推奨

B → C → A の順で実施する。B は独立した変更で効果が大きく、C は B の後に行うとシグネチャ変更が1回で済む。A は最後に追加しても既存テストで回帰を検出できる。

### 性能見積もり（実装後）

| 指標 | 改善前 | 改善後 | 削減率 |
|------|--------|--------|--------|
| ONNX 推論呼び出し | 2 回 | 1 回 | **50%** |
| SQL ラウンドトリップ | 2 回（vec_search + get_chunk_vectors） | 1 回（vec_search のみ） | **50%** |
| 類似度計算量（limit=20, pool=60） | ~34,800 ops（O(n²·limit/2)） | ~2,970 ops（O(n²/2 + limit·n)） | **91%** |
| MMR 候補数 | limit（20） | limit × 3（60） | 多様性向上 |
| メモリ使用量（ベクトル） | 60 × 1024 × 4B ≈ 246KB | 60 × 1024 × 4B ≈ 246KB（変わらず） | 0% |
