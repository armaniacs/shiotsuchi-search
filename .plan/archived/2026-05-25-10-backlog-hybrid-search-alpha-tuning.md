# PBI: ハイブリッド検索のブレンド比率（Alpha 値）カスタマイズ

## ユーザーストーリー
ハイブリッド検索を使うユーザーとして、FTS5 とセマンティック検索の重み比率を調整したい、なぜなら用途によってキーワード完全一致優先か意味的類似性優先かが変わるから

## ビジネス価値
- ユーザーが検索の質を自分の用途に合わせてチューニングできる
- 「キーワード派」と「セマンティック派」両方に対応

## BDD 受け入れシナリオ

```gherkin
Scenario: Alpha=1.0 でキーワード検索のみ
  Given config.toml に hybrid_alpha = 1.0 を設定している
  When ユーザーが `shiotsuchi dive "検索語"` を実行する
  Then FTS5 スコアのみで順位付けされる

Scenario: Alpha=0.0 でセマンティック検索のみ
  Given config.toml に hybrid_alpha = 0.0 を設定している
  When ユーザーが `shiotsuchi dive "検索語"` を実行する
  Then セマンティックスコアのみで順位付けされる
```

## 受け入れ基準
- [ ] `config.toml` に `hybrid_alpha` (0.0〜1.0) を設定できる
- [ ] `--alpha` フラグで実行時にも上書きできる
- [ ] デフォルト値は 0.5（均等ブレンド）

## 見積もり
2 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/search.rs`、`cli/src/config.rs`
- 依存: Fix-2（semantic feature flag）が前提

---

## ⚠️ 実装者向け注記

### 現状確認

```bash
grep -n "rrf\|RRF\|alpha\|hybrid\|K: f64" core/src/search.rs
```

現状のハイブリッド検索は **RRF（Reciprocal Rank Fusion）** を使っています（`search.rs` の `compute_rrf`）。  
RRF は alpha 値ではなく `k` 定数（デフォルト 60.0）でブレンドを調整します。

「alpha 値」を実装するには、RRF から **線形結合スコアリング** への切り替えが必要：
```
score = alpha * fts_score + (1 - alpha) * vec_score
```
ただし FTS スコアとベクトルスコアはスケールが異なるため正規化が必要。

### 実装手順

1. **`SearchConfig`（`core/src/models.rs`）に `hybrid_alpha: f64` を追加する**
2. **`compute_rrf` に加えて `compute_linear_blend` を実装する**  
   alpha=0.5 で両スコアを正規化して線形結合する。
3. **`--alpha` フラグを `cli/src/commands/dive.rs` の `DiveArgs` に追加する**
4. **alpha=1.0 → FTS のみ、alpha=0.0 → vec のみになることをテストで確認する**

### 落とし穴

- FTS5 の `rank` スコアはデフォルトで負値（`-bm25`）。vec スコアはコサイン距離（0〜2 の範囲）。  
  線形結合前に両スコアを [0, 1] に正規化する必要がある。正規化なしで足しても意味のある結果にならない。
- RRF は既にうまく動いている。alpha 実装が複雑で時間がかかるなら、まず `k` パラメータの設定化（`rrf_k = 60.0`）だけ実装して PBI をスコープダウンしても良い。

## Definition of Done
- [ ] alpha 調整のテストがパスする
- [ ] コードレビュー完了
