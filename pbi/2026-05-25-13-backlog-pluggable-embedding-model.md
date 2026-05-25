# PBI: 埋め込み（Embedding）モデルの差し替え対応

## ユーザーストーリー
高スペック PC を持つユーザーとして、より高精度な大型ローカルモデルや外部 API モデルに切り替えたい、なぜなら内蔵モデル固定では精度の改善余地がないから

## ビジネス価値
- ユーザーのリソースに合わせた精度・速度トレードオフの選択を可能に
- OpenAI / Cohere API 連携によるクラウドモデル利用の選択肢

## BDD 受け入れシナリオ

```gherkin
Scenario: 外部 ONNX モデルファイルを指定して使う
  Given config.toml に embedding_model_path を指定している
  When ノートをインデックスする
  Then 指定した ONNX モデルで埋め込みベクトルを生成する

Scenario: OpenAI API モデルを指定して使う
  Given config.toml に embedding_provider = "openai" と api_key を設定している
  When ノートをインデックスする
  Then OpenAI Embeddings API でベクトルを生成する
```

## 受け入れ基準
- [ ] config.toml でモデルプロバイダー（built-in / onnx-file / openai）を選択できる
- [ ] 各プロバイダーごとの設定項目（パス・API キー等）が定義できる
- [ ] デフォルトは内蔵モデル

## 見積もり
8 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/tokenizer.rs`（埋め込み生成箇所）、`cli/src/config.rs`
- 依存: Fix-2（semantic feature flag）

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
cat core/src/embedder.rs | head -60
grep -n "model_path\|SHIOTSUCHI_MODEL_PATH\|embed" core/src/embedder.rs | head -20
cat core/build.rs | head -40
```

現状の埋め込みモデルは `build.rs` でコンパイル時に `SHIOTSUCHI_MODEL_PATH` の ONNX モデルを埋め込んでいる。

### 実装手順

1. **`EmbedderConfig` enum を定義する**：
   ```rust
   pub enum EmbedderConfig {
       BuiltIn,                          // デフォルト（現状）
       OnnxFile { path: PathBuf },       // 外部 ONNX ファイル
       OpenAI { api_key: String, model: String },
   }
   ```

2. **`core/src/embedder.rs` を `EmbedderConfig` ベースで再設計する**  
   `Embedder::new(config: EmbedderConfig) -> Result<Self>` を実装する。

3. **OpenAI API 対応は `reqwest` クレートを追加して HTTP リクエストを実装する**  
   非同期（tokio）が必要になるため設計が複雑になる。このスプリントでは `OnnxFile` のみ実装しても良い。

4. **`config.toml` に `[embedder]` セクションを追加する**

### 落とし穴

- 埋め込みモデルが異なると、DB に格納済みのベクトルとの次元が合わなくなる。  
  モデルを変更した場合は **DB の全ベクトルを再生成する必要がある**（警告メッセージを表示すること）。
- `core/src/db.rs` の `file_cache` テーブルに `model_id` カラムが既にある。モデル変更検出に使えるか確認する：
  ```bash
  grep -n "model_id" core/src/db.rs | head -10
  ```

## Definition of Done
- [ ] 各プロバイダーのテストがパスする
- [ ] コードレビュー完了
