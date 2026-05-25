# PBI: Backlink / PageRank によるノート重要度スコアリング

## ユーザーストーリー
Obsidian で双方向リンクを活用するユーザーとして、他のノートから多く参照されている「ハブノート」が検索上位に来てほしい、なぜなら重要なノートは多くのノートからリンクされているはずだから

## ビジネス価値
- 知識ベース内の「重要なハブ」を検索で発見しやすくする
- Google の PageRank 的アプローチで検索品質を向上

## BDD 受け入れシナリオ

```gherkin
Scenario: 多くのノートからリンクされているノートが上位に来る
  Given ノート A が 20 件のノートからリンクされている
  And ノート B が 1 件のノートからリンクされている
  When 両ノートが同じキーワードでヒットする
  Then ノート A がノート B より上位に表示される

Scenario: リンク解析なしでも従来通り動作する
  Given バックリンク解析を無効化している
  When 検索を実行する
  Then PageRank スコアは適用されず従来のスコア順になる
```

## 受け入れ基準
- [ ] インデックス時に `[[ノート名]]` 形式の Obsidian リンクを解析する
- [ ] `notes_meta` に `backlink_count INTEGER` を追加する
- [ ] 検索スコアに backlink_count を重み付けして加算する
- [ ] バックリンク解析のオン/オフを設定できる

## 見積もり
8 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/indexer.rs`、`core/src/db.rs`、`core/src/search.rs`
- `[[ノート名]]` のリンク解析はインデックス完了後に別パスで実行

---

## ⚠️ 実装者向け注記

### 実装手順

1. **リンク抽出関数を実装する**（`core/src/indexer.rs` または新ファイル）：
   ```rust
   fn extract_wikilinks(content: &str) -> Vec<String> {
       // [[ノート名]] と [[ノート名|表示テキスト]] を抽出
       let re = Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap();
       // ...
   }
   ```

2. **`core/src/db.rs` に `note_links` テーブルを追加する**：
   ```sql
   CREATE TABLE IF NOT EXISTS note_links (
       source_path TEXT NOT NULL,
       target_name TEXT NOT NULL,
       vault_name  TEXT NOT NULL
   );
   ```

3. **インデックス完了後にバックリンク集計を実行する**  
   `chart` コマンドの最後に `db.update_backlink_counts()` を呼ぶ。

4. **`file_cache` または `chunks` に `backlink_count INTEGER` を追加する**

5. **検索スコアへのバックリンクボーナス実装**  
   `build_results` 内で `score *= 1.0 / (1.0 + backlink_count as f64 * 0.1)` のような補正を加える（FTS スコアは低いほど良いため乗算で引く）。

### 落とし穴

- Obsidian の `[[ノート名]]` はファイル名（拡張子なし）でリンクする。  
  DB のパスは相対パスで格納されているため、ファイル名だけでの照合ロジックが必要：
  ```rust
  fn resolve_wikilink(target_name: &str, all_paths: &[&str]) -> Option<&str> {
      all_paths.iter().find(|p| p.ends_with(&format!("{}.md", target_name)))
  }
  ```
- サブディレクトリに同名ノートが複数ある場合の曖昧性解決が必要（Obsidian と同じルールを採用する）。
- バックリンク集計はインデックス完了後に全件スキャンするため、大規模 Vault では時間がかかる。進捗表示を入れること。

## Definition of Done
- [ ] バックリンク解析とスコアリングのテストがパスする
- [ ] コードレビュー完了
