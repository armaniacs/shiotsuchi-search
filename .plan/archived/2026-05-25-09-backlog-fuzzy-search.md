# PBI: あいまい検索（Fuzzy Search）統合

## ユーザーストーリー
ノートを検索するユーザーとして、タイポや送り仮名の揺れを許容して検索したい、なぜなら「引越し」と「引っ越し」のように表記が揺れる語でヒットしないことが多いから

## ビジネス価値
- タイポ・送り仮名・全角半角の差異を許容し検索漏れを削減
- 検索のストレスを軽減

## BDD 受け入れシナリオ

```gherkin
Scenario: 送り仮名が異なる語でもヒットする
  Given "引越し" を本文に含むノートが存在する
  When ユーザーが `shiotsuchi dive --fuzzy "引っ越し"` を実行する
  Then "引越し" を含むノートが検索結果に含まれる

Scenario: 明示的に --fuzzy を指定しない場合は厳密検索
  When ユーザーが `shiotsuchi dive "引っ越し"` を実行する（--fuzzy なし）
  Then 表記揺れは許容されない
```

## 受け入れ基準
- [ ] `--fuzzy` フラグで Fuzzy Search を有効化できる
- [ ] 全角・半角、大文字・小文字の正規化を行う
- [ ] デフォルトは厳密検索を維持

## 見積もり
5 ポイント

## 技術的考慮事項
- FTS5 の `LIKE` 演算や後処理での実装を検討
- 日本語の送り仮名正規化は unicodeNormalization + 独自ルールで対応

---

## ⚠️ 実装者向け注記

### 事前調査

```bash
grep -n "fuzzy\|LIKE\|unicode\|normalize" core/src/search.rs core/src/tokenizer.rs
```

### 実装方針

FTS5 はネイティブにあいまい検索をサポートしない。現実的なアプローチは2つ：

**アプローチ A（推奨、範囲を絞る）**: Unicode 正規化のみ実装  
- NFC/NFD 正規化で全角・半角、濁点合成を統一する  
- インデックス時と検索時の両方に適用する  
- `unicode-normalization` クレートを `core/Cargo.toml` に追加する  
```rust
use unicode_normalization::UnicodeNormalization;
let normalized = query.nfc().collect::<String>();
```

**アプローチ B（将来）**: 編集距離ベースの後処理  
- FTS5 で粗いヒットを取得後、`strsim` クレートで類似度スコアリング  
- 実装コストが高い。このスプリントでは A のみ実装する。

### 落とし穴

- 「引越し」と「引っ越し」の違いは Unicode 正規化では解消できない（送り仮名ルールの問題）。  
  まずは全角/半角統一と NFC 正規化から始め、それ以上は別 PBI にする。
- インデックス時に正規化する場合、既存 DB は再インデックスが必要になる。`chart --reindex` フラグの追加を検討する。

## Definition of Done
- [ ] あいまい検索テストがパスする
- [ ] コードレビュー完了
