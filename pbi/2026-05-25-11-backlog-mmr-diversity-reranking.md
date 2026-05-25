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
