# PBI: セマンティック検索スコア閾値（Threshold）フィルター

## ユーザーストーリー
セマンティック検索を使うユーザーとして、低スコアのノイズ結果を足切りしたい、なぜなら無関係なノートが「無理やり」下位に紛れ込んでくるから

## ビジネス価値
- 検索結果の精度向上（ノイズ削減）
- ユーザーが意味的に無関係な結果を見るストレスを軽減

## BDD 受け入れシナリオ

```gherkin
Scenario: 閾値以下のノートが除外される
  Given config.toml に semantic_threshold = 0.75 を設定している
  When セマンティック検索を実行する
  Then コサイン類似度 0.75 未満のノートは結果に含まれない

Scenario: 閾値設定なしでは全結果を返す
  Given semantic_threshold を設定していない
  When セマンティック検索を実行する
  Then スコア順で全結果が返される
```

## 受け入れ基準
- [ ] `config.toml` に `semantic_threshold` (0.0〜1.0) を設定できる
- [ ] `--threshold` フラグで実行時上書き可能
- [ ] 閾値未設定時は従来通り全件返す

## 見積もり
2 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/search.rs`
- 依存: Fix-2（semantic feature flag）

---

## ⚠️ 実装者向け注記

### 現状確認

```bash
grep -n "min_score\|threshold\|min_score" core/src/search.rs cli/src/commands/dive.rs
```

`search()` 関数のシグネチャ：
```rust
pub fn search(..., min_score: Option<f64>, ...) -> Result<Vec<ChunkSearchResult>, DbError>
```

**`min_score` は既に実装されています。**

`build_results` 関数内で `results.retain(|r| r.score <= ms)` が使われています。  
ただし FTS スコアは低いほど良く（BM25 の負値）、vec スコアは低いほど良い（距離）という統一がされているか確認が必要。

### このPBIで実際にやること

1. **`--threshold` CLI フラグが未実装なら追加する**  
   `cli/src/commands/dive.rs` の `DiveArgs` に `#[arg(long)] min_score: Option<f64>` を追加。

2. **`config.toml` に `semantic_threshold` の設定項目を追加する**

3. **スコアの向き（高いほど良いか低いほど良いか）をモードごとに文書化する**  
   現状コードで FTS, Vec, Hybrid の各スコアの向きを確認してドキュメントに明記する。

### 落とし穴

- FTS の `min_score` と Vec の `min_score` は同じ数値でも意味が全く異なる。  
  モードごとに別々の閾値設定が必要かもしれない。
- Hybrid（RRF）スコアは [0, ~0.03] 程度の範囲。FTS の BM25 スコアとは全く異なる。  
  ユーザーが設定するときに混乱しないよう、ドキュメントに範囲の目安を記載する。

## Definition of Done
- [ ] 閾値フィルターのテストがパスする
- [ ] コードレビュー完了
