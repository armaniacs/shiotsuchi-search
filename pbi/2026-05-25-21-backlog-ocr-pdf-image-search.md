# PBI: PDF テキスト抽出検索（Phase A — 完了）

**Status: ✅ Phase A 実装完了**

## ユーザーストーリー
テキスト埋め込み PDF をノートに添付しているユーザーとして、その内容でも検索したい、なぜなら現状は Markdown テキストしかインデックスされず添付ファイルが検索対象外だから

## ビジネス価値
- `![[document.pdf]]` の内容を検索対象に追加
- PDF 内の情報を発見しやすくする

## スコープ（Phase A）
本 PBI では **テキスト埋め込み PDF** のテキスト抽出 + XY-Cut レイアウト解析 + インデックスを実装する。

以下のスコープは **含まない**（別 PBI で対応）:
- 画像ファイル (`*.png/jpg`) の OCR → **PBI-28**（VLM ベースの画像→Markdown）
- スキャン PDF（テキスト埋め込みなし）の OCR → **PBI-28**

## BDD 受け入れシナリオ

```gherkin
Scenario: 添付 PDF のテキストで検索できる
  Given ノートに `![[report.pdf]]` が埋め込まれており、PDF 内に "Hello" という語がある
  When ユーザーが `shiotsuchi dive "Hello"` を実行する
  Then そのノートが検索結果に含まれる

Scenario: PDF 抽出は初回インデックス時のみ実行される
  Given PDF ファイルが変更されていない
  When `shiotsuchi chart` を再実行する
  Then テキスト抽出は再実行されずキャッシュ済み結果を使う（file_cache の mtime + hash 照合）

Scenario: 設定で PDF 抽出を無効化できる
  Given config.toml に `[indexing]\nenable_pdf_extraction = false` と設定されている
  When `shiotsuchi chart` を実行する
  Then PDF ファイルは空コンテンツでインデックスされる（Inserted、Error ではない）
```

## 受け入れ基準
- [x] `*.pdf` のファイルからテキストを抽出してインデックスする
- [x] PDF 抽出結果は file_cache でキャッシュして再実行を防ぐ（mtime + hash）
- [x] PDF 抽出のオン/オフを設定できる（`enable_pdf_extraction`）
- [ ] ~~`![[*.png/jpg]]` の画像に対して OCR を実行してインデックスする~~ → PBI-28 へ移動

## 実装サマリー

### 採用技術
| コンポーネント | 採用技術 | 理由 |
|---|---|---|
| PDF パース | `pdfium-auto` (bundled) | Chrome 内蔵 PDFium エンジン。段組 PDF の読書順復元に必要な Bounding Box 座標を取得可能。Rust 純製の `pdf-extract` や `lopdf` は座標が取れず ToUnicode 処理も脆弱 |
| レイアウト解析 | XY-Cut（Rust 自前実装） | 段組認識・読書順復元 |
| feature flag | `pdf`（default に含む） | コンパイル時の分離。画像 OCR 追加時は別 feature として追加可能 |

### ファイル構成
- `core/src/pdf.rs`: RawChar/TextLine 型、cluster_to_lines、xycut_to_text、extract_text
- `core/src/indexer.rs`: `index_file_with_embedder` の `.pdf` 分岐 + config トグル
- `core/src/config.rs`: `IndexingConfig.enable_pdf_extraction`（デフォルト `true`）
- `core/src/models.rs`: `IndexConfig.enable_pdf_extraction`

### テストカバレッジ
- `cluster_to_lines` 単体テスト（同ライン統合・別ライン分離）
- `xycut_to_text` 単体テスト（単一カラム・2カラム・全幅タイトル）
- `extract_text` with hello.pdf fixture からのテキスト取得
- E2E: PDF インデックス → FTS5 検索（`test_index_pdf_text_is_searchable_with_pdf_feature`）
- 設定トグル: `enable_pdf_extraction = false` で空コンテンツインデックス
- グレースフルフォールバック: pdf feature OFF 時のバイナリ PDF 読み取り耐性
- config 後方互換: フィールド省略時に `true` と解釈される

### 設定例
```toml
# config.toml
[indexing]
enable_pdf_extraction = false   # PDF テキスト抽出を無効化（デフォルト: true）
```

### 既知の制限
- スキャン PDF（テキスト情報なし）からはテキスト抽出不可 → PBI-28 で対処
- 画像ファイルの OCR は未対応 → PBI-28 で対処
- Phase A でテキストが空の PDF も DB に残すのは、Phase B で `IndexResult::Updated` として上書きするための布石

## Definition of Done
- [x] PDF テキスト抽出テストがパスする（9 テスト + E2E）
- [x] 設定トグルテストがパスする（3 テスト）
- [x] 全テストパス（core: 319, cli: 124）
- [x] ワークスペース全体がビルド可能
- [ ] コードレビュー完了
