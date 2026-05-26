# PBI: コードブロック・数式の特別パース

## ユーザーストーリー
技術系ノートを管理するユーザーとして、コードブロック内の関数名や数式の変数名で検索したい、なぜなら現状はコード部分が日本語テキストと混在してトークナイズされ精度が落ちるから

## ビジネス価値
- コードブロック内の識別子（関数名・変数名）での検索精度向上
- 数式の変数名での検索を実用的にする

## BDD 受け入れシナリオ

```gherkin
Scenario: コードブロック内の関数名で検索できる
  Given ノートに ```rust\nfn calculate_total()``` というコードブロックがある
  When ユーザーが `shiotsuchi dive "calculate_total"` を実行する
  Then そのノートが検索結果に含まれる

Scenario: コードブロックの内容が日本語トークナイザを通らない
  Given コードブロック内に日本語テキストと混在するコードがある
  When インデックスする
  Then コードブロック部分は空白区切りでトークナイズされる
```

## 受け入れ基準
- [x] ` ``` ` で囲まれたコードブロックを検出して別扱いでインデックスする
- [x] `$ ... $` の LaTeX 数式ブロックを検出して別扱いでインデックスする
- [x] コード部分は Whitespace Tokenizer で分割する

## 見積もり
5 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/indexer.rs`、`core/src/tokenizer.rs`
- pulldown-cmark 等の Markdown パーサーでコードブロックを抽出

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
# 現状のトークナイズフローを確認する
grep -n "tokenize\|tokenized_content\|code.*block\|```" core/src/indexer.rs | head -20
grep -n "pulldown\|comrak\|markdown" core/Cargo.toml
```

現状はコードブロックを含むMarkdown全体をVaporettoでトークナイズしている可能性が高い。

### 実装手順

1. **`core/Cargo.toml` に `pulldown-cmark` を追加する**（既に入っている場合は不要）

2. **Markdown を「通常テキスト」と「コードブロック」に分離する関数を実装する**：
   ```rust
   struct ParsedMarkdown {
       prose_text: String,    // 通常テキスト（Vaporetto でトークナイズ）
       code_text: String,     // コードブロック内（空白区切りトークナイズ）
   }
   
   fn split_markdown_content(content: &str) -> ParsedMarkdown {
       use pulldown_cmark::{Parser, Event, Tag};
       // ...
   }
   ```

3. **`core/src/tokenizer.rs` に空白区切りトークナイザーを追加する**：
   ```rust
   pub fn whitespace_tokenize(text: &str) -> Vec<String> {
       text.split_whitespace()
           .flat_map(|w| w.split(|c: char| !c.is_alphanumeric()))
           .filter(|s| !s.is_empty())
           .map(String::from)
           .collect()
   }
   ```

4. **`tokenized_content` に prose トークン + code トークンを結合して格納する**

### 落とし穴

- インラインコード（`` `code` ``）とフェンスドコードブロック（` ``` `）を両方処理する。
- コードブロック内の言語識別子（` ```rust `）はトークンに含めない。
- 既に格納済みのインデックスは再インデックスが必要になる。`chart` 実行時に自動検出・再インデックスするか、`--reindex` フラグを提供する。

## Definition of Done
- [x] コードブロック内識別子の検索テストがパスする
- [x] コードレビュー完了
