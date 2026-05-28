# PBI: 埋め込み（Embedding）モデルの差し替え対応

**Status: ✅ Implemented in v0.4.12**

## ユーザーストーリー
高スペック PC を持つユーザーとして、より高精度な大型ローカルモデルや外部 API モデルに切り替えたい、なぜなら内蔵モデル固定では精度の改善余地がないから

## ビジネス価値
- ユーザーのリソースに合わせた精度・速度トレードオフの選択を可能に
- OpenAI / Cohere API 連携によるクラウドモデル利用の選択肢

## BDD 受け入れシナリオ

```gherkin
Scenario: 外部 ONNX モデルファイルを指定して使う
  Given config.toml に [embedder] provider = "onnx-file" と path を指定している
  When ノートをインデックスする
  Then 指定した ONNX モデルで埋め込みベクトルを生成する

Scenario: OpenAI 互換 API モデルを指定して使う
  Given config.toml に [embedder] provider = "api" と endpoint, model を設定している
  And SHIOTSUCHI_API_KEY 環境変数が設定されている
  When ノートをインデックスする
  Then 指定した API エンドポイントでベクトルを生成する
```

## 受け入れ基準
- [x] config.toml でモデルプロバイダー（built-in / onnx-file / api）を選択できる
- [x] 各プロバイダーごとの設定項目（パス・エンドポイント・モデル名・API キー等）が定義できる
- [x] デフォルトは内蔵モデル（built-in）

## 見積もり
8 ポイント

## 技術的考慮事項（実装済み）
- 影響ファイル: `core/src/embedder.rs`, `core/src/api_embedder.rs`, `core/src/config.rs`, `cli/src/commands/chart.rs`, `cli/src/commands/scan.rs`
- HTTP クライアント: `ureq`（同期、軽量）— `reqwest` ではなく `ureq` を採用
- API キー環境変数: `SHIOTSUCHI_API_KEY`（当初 `SAKURA_AI_API_KEY` を検討したが、プロバイダー非依存に変更）
- 依存: semantic feature flag（既存）

---

## ⚠️ 実装者向け注記

### 実装概要（v0.4.12 で完了）

1. **`EmbedderConfig` enum に `Api` バリアントを追加**
   ```rust
   pub enum EmbedderConfig {
       #[default]
       BuiltIn,
       OnnxFile { path: PathBuf },
       Api {
           endpoint: String,
           model: String,
           api_key: Option<String>,
       },
   }
   ```

2. **`EmbedderBackend` enum でローカル/API を統一**
   既存の `Embedder` 構造体をファサード化し、内部で `EmbedderBackend::Onnx` / `EmbedderBackend::Api` を切り替え。呼び出し側（indexer, search, watcher）は変更なし。

3. **`ApiClient`（`core/src/api_embedder.rs`）**
   - OpenAI 互換 `/v1/embeddings` 形式
   - バッチリクエスト（100件/リクエスト上限）
   - 60秒タイムアウト
   - `model_id` は `endpoint + model` の SHA-256 ハッシュ（変更検出対応）

4. **API キー解決**
   - 優先順位: `SHIOTSUCHI_API_KEY` 環境変数 > `config.toml` の `api_key`
   - config にキーがある場合、CLI は警告を表示

5. **`create_embedder()` メソッド**
   `EmbedderConfig` に追加したファクトリメソッド。`BuiltIn`/`OnnxFile` → `Embedder::load()`、`Api` → `Embedder::from_api_client()`

6. **モデル変更検出**
   `get_dominant_model_id()`（v0.4.12 で前段実装済み）を流用し、API プロバイダーでも `model_id` ベースの変更検出が動作。

### 落とし穴（対応済み）

- 埋め込みモデルが異なると、DB に格納済みのベクトルとの次元が合わなくなる。  
  → `get_dominant_model_id()` + `WARN_MODEL_CHANGED` でインデックス時に警告を表示済み。
- `file_cache` テーブルの `model_id` カラムを変更検出に使用済み。

### ドキュメント更新

- `ref/cli.md` — `[embedder]` セクションに `api` プロバイダーのフィールド表を追加
- `ref/core.md` — `EmbedderConfig` 型定義を更新
- `docs/CLI-USE.ja.md`, `docs/INSTALL.ja.md`, `README.ja.md` — 日本語版に API 設定手順を追加
- `CHANGELOG.md` — v0.4.12 セクションに記載

## Definition of Done
- [x] 各プロバイダーのテストがパスする（core 302 tests, CLI 124 tests all pass）
- [x] コードレビュー完了（subagent-driven development + spec/code quality reviews）
- [x] ユーザ向けドキュメント更新完了
