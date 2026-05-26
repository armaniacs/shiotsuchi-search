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
- [ ] `--mmr` フラグで MMR リランキングを有効化できる
- [ ] MMR の lambda 値を設定で調整できる
- [ ] デフォルトは従来のスコア順

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
- [ ] MMR リランキングのテストがパスする
- [ ] コードレビュー完了

---

## 第2フェーズ: 性能改善（A + B + C）

初回実装後、以下の3つの最適化を追加適用する。

### A: 類似度行列の事前計算

**現状の問題**: `mmr_rerank()` の選択ループ内で、各イテレーションごとに残り全候補 × 選択済み全要素の `cosine_similarity()` を計算している。計算量は **O(limit · n²)**（limit=20, n=60 で 72,000 回の類似度計算）。

**改善**: 選択ループに入る前に全候補間の n×n 類似度行列を1回だけ計算する。選択ループ内は行列のルックアップのみ。
計算量は **O(n² + limit · n)**（約 2,100 回 + 1,200 回のルックアップ、約 22 倍高速）。

```rust
// Before: 選択ループ内で毎回計算
for candidate in remaining {
    for selected in &selected {
        cosine_similarity(cv, sv)  // O(limit · n²)
    }
}

// After: 事前計算した行列をルックアップ
let sim_matrix = precompute_pairwise(vectors);  // O(n²)
for candidate in remaining {
    sim_matrix[candidate_idx][selected_idx]  // O(1) lookup
}
```

### B: 候補プールの拡大

**現状の問題**: `search_vec(limit)` の結果だけを MMR に渡している。MMR は多様な結果を promote するために limit より多い候補が必要。

**改善**: MMR 有効時、内部で `search_vec(limit * MMR_POOL_MULTIPLIER)`（デフォルト 3 倍）で候補を取得し、MMR で `limit` に絞り込む。

```
変更前: vec_search(limit=20) → mmr_rerank(20 candidates, limit=20)
変更後: vec_search(limit=60) → mmr_rerank(60 candidates, limit=20)
```

### C: クエリベクトルの二重計算排除

**現状の問題**: `emb.embed(query)` が以下の 2 箇所で呼ばれ、ONNX 推論が2回実行される：

1. `search_vec()` 内部（行 344）— KNN 検索のため
2. `search()` MMR ブロック（行 139）— クエリ類似度計算のため

**改善**: `search()` で 1 回だけ `emb.embed(query)` を実行し、`Vec<f32>` を `search_vec()` と `mmr_rerank()` の両方に共有する。`search_vec()` に `precomputed_embedding: Option<Vec<f32>>` 引数を追加し、`Some` の場合は内部の `emb.embed()` をスキップする。

### 追加最適化: vec_search がベクトルも返す（DB ラウンドトリップ削減）

**現状の問題**: MMR のために `get_chunk_vectors()` で別途 SQL クエリを発行している。`vec_search()` はすでに sqlite-vec 経由でベクトルを計算しているが、距離だけ返してベクトルを捨てている。

**改善**: `db::vec_search()` の戻り値を `Vec<(i64, f64, Vec<f32>)>` に変更し、chunk_id, distance, embedding の3つ組を返す。`search_vec()` は結果とベクトルマップの両方を上位に返す。MMR はそれを使うため、`get_chunk_vectors()` の呼び出しが完全に不要になる。

### 影響ファイル

| ファイル | 変更内容 |
|---------|---------|
| `core/src/db.rs` | `vec_search()` 戻り値に embedding 追加 |
| `core/src/search.rs` | `search_vec()` 戻り値変更、`search()` で query を1回 embedding、`mmr_rerank()` に類似度行列＋候補プール拡大 |
| `core/src/tests/` | 統合テスト更新 |

### 性能見積もり

| 指標 | 改善前 | 改善後 | 削減率 |
|------|--------|--------|--------|
| ONNX 推論呼び出し | 2 回 | 1 回 | **50%** |
| SQL ラウンドトリップ | 2 回（vec_search + get_chunk_vectors） | 1 回（vec_search のみ） | **50%** |
| 類似度計算量（limit=20, pool=60） | O(20·60²) ≈ 72,000 ops | O(60² + 20·60) ≈ 4,800 ops | **93%** |
| メモリ使用量（ベクトル） | 60 × 1024 × 4B = 246KB | 60 × 1024 × 4B = 246KB（変わらず） | 0% |
