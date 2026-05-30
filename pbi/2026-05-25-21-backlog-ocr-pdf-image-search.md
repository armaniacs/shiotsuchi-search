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
1. **PDF テキスト抽出 + XY-Cut レイアウト解析の実装**（Phase A = 本 PBI）
2. **VLM ベースの PDF Markdown 化**（Phase B = 別 PBI、API キー必須のため分離）

### Phase A: PDF テキスト抽出 + XY-Cut レイアウト解析（本 PBI の実装方針）

**なぜ `pdf-extract` ではなく `pdfium-render` を選んだか:**
- 段組（マルチカラム）PDF の読書順を正しく復元するには、文字の **Bounding Box（座標情報）** が必要
- Rust 純製の `pdf-extract` や `lopdf` は座標が取れず、フォントエンコーディング（ToUnicode マップ）の処理も脆弱でスタックしやすい
- `pdfium-render` は Chrome 内蔵の PDFium エンジンの Rust バインディング。世界中の怪しい PDF を処理してきた圧倒的な堅牢性を持つ
- `bundled` feature により、単一バイナリで完結（+30〜40 MB だが外部依存ゼロ）

**実装アーキテクチャ（3 レイヤー）:**

```
pdfium-render (bundled)   ← PDF パース: 文字座標・テキスト取得
       ↓
XY-Cut レイアウト解析     ← 段組認識・読書順復元（Rust 自前実装）
       ↓
index_file_with_embedder  ← 既存キャッシュ・FTS5 に流し込む
```

```toml
# core/Cargo.toml に追加
pdfium-render = { version = "0.8", features = ["bundled"], optional = true }

[features]
default = ["watcher", "async-index", "semantic", "pdf"]
pdf = ["dep:pdfium-render"]
```

詳細設計: `docs/superpowers/specs/2026-05-30-pdf-text-extraction-design.md`

### Phase B: VLM ベースの PDF Markdown 化（別 PBI）

スキャン PDF（テキスト埋め込みなし）や複雑なレイアウトの PDF に対応するため、VLM（GPT-4、Claude 等）を使って画像 → Markdown 変換するアプローチ。**別 PBI-28 として管理**（API キー必須、ページ単位でコストが発生するため本 PBI とは分離）。参照: `pbi/2026-05-30-28-backlog-vlm-pdf-markdown.md`

Phase A でテキストが空の PDF も DB に残すのは、Phase B で `IndexResult::Updated` として上書きするための布石。

### 落とし穴

- `![[image.png]]` は Obsidian の埋め込み記法。本 PBI では PDF をファイルとして直接インデックス（埋め込み記法の解析は不要）。
- PDF の再インデックス防止は `file_cache` テーブルで mtime + file_size ベースに行える（既存の仕組みを流用）。
- `pdfium-render` で個別ページのパースが失敗した場合はそのページをスキップし、残りページのテキストで処理を継続すること。

## Definition of Done
- [ ] PDF テキスト抽出・OCR テストがパスする
- [ ] コードレビュー完了
