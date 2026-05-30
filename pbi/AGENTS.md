# AGENTS.md

`./PBI-process.md` に、このディレクトリにある pbiファイルの取り扱いについて書いている。必ず読むこと。

## 取り組まないことに決めているもの

### PBI-15: MCP Read-Write 拡張

この PBI は **サポートしない**。

shiotsuchi-search はノートの検索エンジンであり、読み取り専用（Read-Only）を原則とする。
MCP 経由でのノート作成・編集・削除はプロジェクトの範囲外であり、ツールの哲学（「検索することに特化する」）
に反する。読み取り専用の検索結果の返却に専念する。

## 事前調査中・実装方針未確定のもの

### PBI-21: OCR PDF/画像検索

**edgequake/pdf2md** の採用が最有力。  
調査・検証済み次第、実装方針を確定する。
https://github.com/raphaelmansuy/edgequake-pdf2md 

## 実装順序に関する決定

### PBI-18: Backlink PageRank スコアリング

この PBI は **PBI-27（Obsidian プラグイン）の後に実装する**。
理由: Backlink 情報は Obsidian プラグイン経由で取得する方が効率的であり、
スタンドアロンでのリンク解析よりも精度が高い。

### PBI-13: 埋め込みモデルの差し替え（API 方式）

**Status: Completed in v0.4.12**

実装方策（実際に実装されたもの）:

- **API エンドポイント**: OpenAI 互換 API へのベース URL（例: `https://api.ai.sakura.ad.jp/v1/embeddings`）
- **モデル**: OpenAI 互換 API がサポートする任意のモデル名（例: `multilingual-e5-large`）
- **API キー**: 環境変数 `SHIOTSUCHI_API_KEY` で指定（プロバイダー非依存の命名）
- **実装方式**: `EmbedderBackend` enum で ONNX ローカル推論と HTTP API を統一

