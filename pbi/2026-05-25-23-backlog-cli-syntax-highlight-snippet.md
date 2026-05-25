# PBI: CLI シンタックスハイライト＋スニペット（前後文脈）表示

## ユーザーストーリー
CLI で検索するユーザーとして、ヒット箇所の前後文脈をカラー強調で見たい、なぜなら現在の出力ではどの部分がマッチしたかわかりにくく、結果を読むのに時間がかかるから

## ビジネス価値
- 検索結果の視認性向上
- ヒット箇所を一目で把握でき操作効率アップ

## BDD 受け入れシナリオ

```gherkin
Scenario: ヒット箇所が色付きで表示される
  When ユーザーが `shiotsuchi dive "プロジェクト"` を実行する
  Then 各結果にヒット箇所の前後 50 文字のスニペットが表示される
  And "プロジェクト" 部分が ANSI カラーコードで強調される

Scenario: --no-color で色なし出力
  When ユーザーが `shiotsuchi dive --no-color "プロジェクト"` を実行する
  Then ANSI コードなしのプレーンテキストで出力される
```

## 受け入れ基準
- [ ] 各検索結果にスニペット（前後 N 文字）が表示される
- [ ] スニペット内のヒット語が ANSI カラーで強調される
- [ ] `--no-color` フラグまたは `NO_COLOR` 環境変数でカラーを無効化できる
- [ ] スニペット文字数を設定で変更できる

## 見積もり
3 ポイント

## 技術的考慮事項
- 影響ファイル: `cli/src/main.rs`、`core/src/search.rs`（スニペット抽出）
- ANSI カラー: `colored` または `owo-colors` クレート
- FTS5 の `snippet()` 関数の活用を検討

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
# スニペット・出力の現状実装を確認する
grep -n "print_results\|snippet\|color\|extract_snippet" cli/src/commands/dive.rs cli/src/main.rs | head -20
grep -n "extract_snippet\|SearchConfig\|max_snippet" core/src/search.rs core/src/models.rs | head -20
```

`core/src/search.rs` に `extract_snippet()` 関数が既に実装されています（`SearchConfig.max_snippet_chars` も存在）。  
`cli/src/commands/dive.rs` の `print_results` 関数でどう出力しているか確認する。

### 実装手順

1. **`cli/Cargo.toml` に `colored` クレートを追加する**（または既存の依存を確認）

2. **`print_results` でクエリとスニペットを照合してハイライトを付ける**：
   ```rust
   fn highlight_query(text: &str, query_tokens: &[&str]) -> String {
       let mut result = text.to_string();
       for token in query_tokens {
           result = result.replace(token, &token.red().bold().to_string());
       }
       result
   }
   ```

3. **`--no-color` フラグを `DiveArgs` に追加する**  
   `NO_COLOR` 環境変数も確認する（`std::env::var("NO_COLOR").is_ok()`）。

### 落とし穴

- `colored` クレートは `NO_COLOR` 環境変数を自動で尊重する設定がある（`.no_color()` メソッドを使う）。
- 日本語テキストのハイライトはバイト境界に注意。`str::replace` は UTF-8 セーフだが、バイト単位の操作は避けること。
- パイプ出力時（`| less` 等）に ANSI コードが混入すると読みにくい。`atty` クレートで端末かどうかを判定し、非端末ならカラーを無効化する。

## Definition of Done
- [ ] スニペット・ハイライト表示のテストがパスする
- [ ] コードレビュー完了
