# PBI: 同義語・類義語（シソーラス）マッピング

## ユーザーストーリー
技術系ノートを管理するユーザーとして、「AWS」で検索したら「Amazon Web Services」を含むノートもヒットしてほしい、なぜなら略語・正式名称の表記揺れで検索漏れが発生するから

## ビジネス価値
- 略語・正式名称・表記揺れによる検索漏れを解消
- ユーザーが辞書を育てることで検索精度が継続的に向上

## BDD 受け入れシナリオ

```gherkin
Scenario: 同義語辞書に登録した語で検索漏れがなくなる
  Given config.toml に "AWS" = ["Amazon Web Services"] の同義語設定がある
  And "Amazon Web Services" を本文に含むノートが存在する
  When ユーザーが `shiotsuchi dive "AWS"` を実行する
  Then "Amazon Web Services" を含むノートが検索結果に含まれる

Scenario: 同義語辞書がない場合は従来通りの検索
  Given 同義語辞書を設定していない
  When ユーザーが `shiotsuchi dive "AWS"` を実行する
  Then "AWS" を含むノートのみがヒットする
```

## 受け入れ基準
- [ ] config.toml または専用ファイルで同義語マッピングを定義できる
- [ ] FTS5 クエリ生成時に同義語を OR で展開する
- [ ] 辞書なしでも従来通り動作する

## 見積もり
3 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/search.rs`、`cli/src/config.rs`
- クエリ展開: `("AWS" OR "Amazon Web Services")` の形式で FTS5 に渡す

---

## ⚠️ 実装者向け注記

### 実装手順

1. **設定スキーマに同義語マッピングを追加する**  
   `core/src/config.rs`（または `ShiotsuchiConfig`）に：
   ```toml
   [synonyms]
   "AWS" = ["Amazon Web Services", "アマゾン"]
   "k8s" = ["Kubernetes"]
   ```

2. **クエリ展開ロジックを `core/src/search.rs` の `search_fts` 入口に追加する**  
   ```rust
   fn expand_synonyms(query: &str, synonyms: &HashMap<String, Vec<String>>) -> String {
       // 各トークンについて同義語があれば OR で展開
       // "AWS" → "AWS OR \"Amazon Web Services\""
   }
   ```

3. **FTS5 の OR 構文に注意する**  
   FTS5 では `OR` は大文字で、フレーズ検索は `"..."` で囲む。
   ```
   AWS OR "Amazon Web Services"
   ```

### 落とし穴

- FTS5 のクエリ構文はシンプルな SQL の OR とは異なる。  
  `simple_and_query()` 関数（`core/src/tokenizer.rs`）の実装を参考にすること。
- 同義語展開でクエリが非常に長くなる場合（多数の同義語）、FTS5 がエラーになる可能性がある。展開数に上限を設けること。
- 日本語の同義語はトークン化後の形と一致する必要がある。「Amazon」が「アマゾン」にトークン化される場合と直接「Amazon」として扱われる場合がある。

## Definition of Done
- [ ] 同義語展開のテストがパスする
- [ ] コードレビュー完了
