# PBI: 英日混在（マルチリンガル）環境への Whitespace フォールバック

## ユーザーストーリー
英語と日本語が混在するノートを管理するユーザーとして、英語の技術用語やコードも漏れなく検索したい、なぜなら Vaporetto は日本語に最適化されており英語部分の分割精度が低いことがあるから

## ビジネス価値
- 英日混在ノートの英語部分の検索精度向上
- 技術ノート（英語コード + 日本語説明）での検索漏れを削減

## BDD 受け入れシナリオ

```gherkin
Scenario: 英語部分が Whitespace で正しく分割される
  Given "This is a React component for ユーザー管理" というテキストがある
  When インデックスする
  Then "React"・"component" が独立したトークンとして検索できる

Scenario: 日本語部分は従来通り Vaporetto でトークナイズされる
  When "ユーザー管理" というノートをインデックスする
  Then "ユーザー"・"管理" が適切なトークンに分割される
```

## 受け入れ基準
- [ ] ASCII 英数字トークンを Whitespace で分割するフォールバックを追加する
- [ ] 日本語部分は Vaporetto トークナイズを維持する
- [ ] 重複トークンを排除して FTS5 インデックスに格納する

## 見積もり
3 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/tokenizer.rs`
- Unicode 文字種判定で日本語・英語ブロックを分離

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
cat core/src/tokenizer.rs | head -80
grep -n "ascii\|english\|whitespace\|simple_and_query\|latin" core/src/tokenizer.rs | head -20
```

`core/src/tokenizer.rs` に `simple_and_query()` 関数が存在します。これが英語フォールバックの役割を担っている可能性があります。現状の動作を把握してから実装すること。

### 実装方針

Vaporetto は英語トークン（ASCII 英数字の連続）も分割するが、精度が低い場合がある。  
以下のアプローチで英語部分を補強する：

```rust
pub fn tokenize_mixed(text: &str, vaporetto: &JapaneseTokenizer) -> Vec<String> {
    // 1. Vaporetto でトークナイズ
    let jp_tokens = vaporetto.tokenize(text);
    
    // 2. 元テキストから ASCII 英数字の連続を空白区切りで追加抽出
    let ascii_tokens: Vec<String> = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| s.len() >= 2)  // 1文字は除外
        .map(|s| s.to_lowercase())
        .collect();
    
    // 3. 重複を除いて結合
    let mut all: HashSet<String> = jp_tokens.into_iter().collect();
    all.extend(ascii_tokens);
    all.into_iter().collect()
}
```

### 落とし穴

- 英語トークンの追加により `tokenized_content` が長くなり、FTS5 インデックスサイズが増加する。大きな影響は出ないはずだが、ベンチマークで確認する（`cargo bench -p shiotsuchi-core`）。
- 既存インデックスは再インデックスが必要になる。`file_cache` の `model_id` を更新してキャッシュを無効化する仕組みを使えるか確認する。
- 英語の大文字・小文字統一（lowercase）はインデックス時と検索時の両方に適用すること。

## Definition of Done
- [ ] 英日混在テキストのトークナイズテストがパスする
- [ ] コードレビュー完了
