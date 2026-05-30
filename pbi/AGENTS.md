# AGENTS.md

`./PBI-process.md` に、このディレクトリにある pbiファイルの取り扱いについて書いている。必ず読むこと。

## 取り組まないことに決めているもの

### PBI-15: MCP Read-Write 拡張

この PBI は **サポートしない**。

shiotsuchi-search はノートの検索エンジンであり、読み取り専用（Read-Only）を原則とする。
MCP 経由でのノート作成・編集・削除はプロジェクトの範囲外であり、ツールの哲学（「検索することに特化する」）
に反する。読み取り専用の検索結果の返却に専念する。

## 完了・アーカイブ済み

### PBI-21: PDF テキスト抽出検索

**Phase A (PDF テキスト抽出 + XY-Cut レイアウト解析) — 完了・アーカイブ済み。**

採用技術:
- `pdfium-render` + `pdfium-auto` (bundled): Chrome 内蔵 PDFium エンジンの Rust バインディング
- XY-Cut レイアウト解析: Rust 自前実装（段組認識・読書順復元）
- feature flag: `pdf`（default に含む）
- 設定トグル: `IndexingConfig.enable_pdf_extraction`

実装詳細:
- `core/src/pdf.rs`: RawChar/TextLine 型、cluster_to_lines、xycut_to_text、extract_text
- `index_file_with_embedder` で `.pdf` 拡張子を特別処理
- デフォルトの include_extensions に `pdf` を追加
- E2E テスト: hello.pdf のテキストが FTS5 検索可能なことを確認済み

**画像 OCR は別 PBI（Phase B）として分割。**
参照: `pbi/2026-05-30-28-backlog-vlm-pdf-markdown.md`

### PBI-13: 埋め込みモデルの差し替え（API 方式）

**Completed in v0.4.12**

実装方策（実際に実装されたもの）:

- **API エンドポイント**: OpenAI 互換 API へのベース URL（例: `https://api.ai.sakura.ad.jp/v1/embeddings`）
- **モデル**: OpenAI 互換 API がサポートする任意のモデル名（例: `multilingual-e5-large`）
- **API キー**: 環境変数 `SHIOTSUCHI_API_KEY` で指定（プロバイダー非依存の命名）
- **実装方式**: `EmbedderBackend` enum で ONNX ローカル推論と HTTP API を統一

