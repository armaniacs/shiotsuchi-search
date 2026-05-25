# PBI: 埋め込み画像・PDF の OCR テキスト抽出検索

## ユーザーストーリー
スキャンした PDF や画像をノートに添付しているユーザーとして、その内容でも検索したい、なぜなら現状は Markdown テキストしかインデックスされず添付ファイルが検索対象外だから

## ビジネス価値
- `![[image.png]]` や `![[document.pdf]]` の内容を検索対象に追加
- 画像・PDF 内の情報を発見しやすくする

## BDD 受け入れシナリオ

```gherkin
Scenario: 添付 PDF のテキストで検索できる
  Given ノートに `![[report.pdf]]` が埋め込まれており、PDF 内に "四半期報告" という語がある
  When ユーザーが `shiotsuchi dive "四半期報告"` を実行する
  Then そのノートが検索結果に含まれる

Scenario: OCR は初回インデックス時のみ実行される
  Given 画像ファイルが変更されていない
  When `shiotsuchi chart` を再実行する
  Then OCR は再実行されずキャッシュ済み結果を使う
```

## 受け入れ基準
- [ ] `![[*.pdf]]` の PDF からテキストを抽出してインデックスする
- [ ] `![[*.png/jpg]]` の画像に対して OCR を実行してインデックスする
- [ ] OCR 結果はキャッシュして再実行を防ぐ
- [ ] OCR 機能のオン/オフを設定できる

## 見積もり
13 ポイント（大型、分割検討）

## 技術的考慮事項
- PDF テキスト抽出: `pdf-extract` または `lopdf` クレート
- 画像 OCR: `tesseract` バインディング（`leptess` クレート）
- OCR は外部依存が増えるため feature flag で分離推奨

---

## ⚠️ 実装者向け注記

### このPBIの実装難易度について

**Epic レベルの大型 PBI です。**  
実装前にシニアエンジニアと分割方針を相談してください。

段階的に進める推奨順序：
1. **PDF テキスト抽出のみ実装**（Phase A）
2. **画像 OCR の実装**（Phase B、依存が増えるため分離）

### Phase A: PDF テキスト抽出

```toml
# core/Cargo.toml に追加（feature gate 推奨）
pdf-extract = { version = "0.7", optional = true }

[features]
ocr = ["dep:pdf-extract"]
```

実装箇所：
- `core/src/indexer.rs` の `index_directory` でファイル拡張子を `.pdf` にも対応させる
- PDF ファイルは `pdf_extract::extract_text(path)?` でテキストを取得してトークナイズする
- `file_cache` のキャッシュキーは通常の `.md` ファイルと同様に使える

### Phase B: 画像 OCR

Tesseract は OS レベルのインストールが必要（`brew install tesseract`）。  
`leptess` クレートはシステムの Tesseract ライブラリにリンクする。  
CI での動作確認が必要になるため、このフェーズは慎重に進めること。

### 落とし穴

- `![[image.png]]` は Obsidian の埋め込み記法。このパスを解決するには、ノートのあるディレクトリからの相対パスか、Vault 全体からの検索が必要。
- PDF の OCR キャッシュは `file_cache` テーブルで mtime ベースに行える（既存の仕組みを流用）。
- `pdf-extract` は一部の PDF（フォームや画像のみの PDF）でテキスト抽出に失敗する。エラーを無視してスキップする処理を入れること。

## Definition of Done
- [ ] PDF テキスト抽出・OCR テストがパスする
- [ ] コードレビュー完了
