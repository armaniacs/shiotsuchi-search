# AGENTS.md

`./PBI-process.md` に、このディレクトリにある pbiファイルの取り扱いについて書いている。必ず読むこと。

## 取り組まないことに決めているもの

### PBI-15: MCP Read-Write 拡張

この PBI は **サポートしない**。

shiotsuchi-search はノートの検索エンジンであり、読み取り専用（Read-Only）である。
MCP 経由によるノートの作成・編集・削除は厳密に禁止する。検索機能と読み取り専用の検索結果の返却に専念する。

## 完了・アーカイブ済み

### PBI-27: Obsidian コミュニティプラグイン化

**分割・アーカイブ済み — Rust 側は PBI-37 として独立、TypeScript 側は別リポジトリで管理。**

分割方針:
- **Rust 側**（このリポジトリ）: `shiotsuchi serve` — HTTP ローカルサーバーモード追加 → `pbi/2026-06-03-37-feat-serve-http-server.md`
- **TypeScript 側**（別リポジトリ: `shiotsuchi-obsidian`）: Obsidian プラグイン本体

### PBI-21: PDF テキスト抽出検索

**Phase A (PDF テキスト抽出 + XY-Cut レイアウト解析) — 完了・アーカイブ済み。**

採用技術:
- `pdfium-render` + `pdfium-auto` (bundled): Chrome 内蔵 PDFium エンジンの Rust バインディング
- XY-Cut レイアウト解析: Rust 自前実装（段組認識・読書順復元）
- feature flag: `pdf` — デフォルトで有効（enabled by default）
- 設定トグル: `IndexingConfig.enable_pdf_extraction`（デフォルト: true、説明: true のとき PDF のテキスト抽出を実行する）

実装詳細:
- `core/src/pdf.rs`: RawChar/TextLine 型、cluster_to_lines、xycut_to_text、extract_text
- `index_file_with_embedder` で `.pdf` 拡張子を特別処理
- `pdf` は既存の `include_extensions` リストに追記する（上書きしない）：`include_extensions = default_include_extensions ∪ {'pdf'}`。
- E2E テスト: hello.pdf のテキストが FTS5 検索可能なことを確認済み
- PDF 抽出が失敗した場合は、ファイルをメタデータのみでインデックスし、抽出エラーをログに記録して再試行キューに追加する。画像ベースのページは Phase B（OCR）で処理予定のため、`OCR_REQUIRED` フラグを付与する。
- 抽出が部分的にしか成功しない場合は、成功したテキストをインデックスし、失敗したページを `extraction_errors` に記録して再処理キューに入れる。
- PDF 内のテキスト抽出可能ページは Phase A で処理し、画像ベースページは Phase B として個別キューに登録する。Phase A はページ粒度で処理を分離する。

**画像 OCR は別 PBI（Phase B）として分割。**
参照: `pbi/2026-05-30-28-backlog-vlm-pdf-markdown.md`

### PBI-28: VLM ベース PDF Markdown 化（スキャン PDF 対応）

**Completed — `vlm` feature を CLI デフォルトに追加、テスト追加済み。**

採用技術:
- `edgequake-pdf2md` v0.9: VLM API（OpenAI/Anthropic/Gemini/Ollama）経由で PDF→Markdown 変換
- `edgequake-llm` v0.6.23: 複数 LLM プロバイダーの抽象化
- feature flag: `vlm` — CLI デフォルトビルドに含める（`cli/Cargo.toml:38`）
- 設定トグル: `VlmConfig.enabled`（デフォルト: false）
- API キー: `SHIOTSUCHI_API_KEY` または `<PROVIDER>_API_KEY`

実装詳細:
- `core/src/vlm.rs`: `extract_text_with_vlm()` 実装 + `#[cfg(not(feature = "vlm"))]` スタブ
- `core/src/indexer.rs:446-470`: ネイティブ PDF 抽出が空の場合に VLM フォールバック
- `core/src/config.rs:218-244`: `VlmConfig` 構造体
- `core/tests/integration_test.rs`: feature コンパイルテスト + mtime キャッシュテスト

### PBI-13: 埋め込みモデルの差し替え（API 方式）

**Completed in v0.4.12**

実装方策（実際に実装されたもの）:

- **API エンドポイント**: OpenAI 互換 API へのベース URL（例: `https://api.ai.sakura.ad.jp/v1/embeddings`）
- **モデル**: OpenAI 互換 API の公開仕様で動作確認されているモデル名のみ使用可。推奨例は `multilingual-e5-large`。
- **API キー**: 環境変数 `SHIOTSUCHI_API_KEY` で指定（プロバイダー非依存の命名）
- **実装方式**: `EmbedderBackend` enum で ONNX ローカル推論と HTTP API を統一
- モデル名でエラーが発生した場合は、事前定義されたフォールバックモデル（例: `multilingual-e5-large`）に自動切替し、切替をログに記録する。
- もし `SHIOTSUCHI_API_KEY` が未設定または認証エラーが返った場合は、HTTP API 呼び出しを行わず、明確なエラーメッセージを出力して失敗させる。利用可能なら ONNX ローカル推論に自動的にフォールバックし、ログに記録する。

