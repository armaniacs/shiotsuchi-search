# PBI: HTTP サーバーモード追加（`shiotsuchi serve`）

## ユーザーストーリー
Obsidian プラグインやブラウザから shiotsuchi の検索機能を使いたい、なぜなら CLI は非エンジニアユーザーにはハードルが高く、HTTP API を提供することで UI フロントエンドから利用できるようになるから

## ビジネス価値
- Obsidian プラグイン（別リポジトリ）のバックエンドとして機能する
- ブラウザベースの検索 UI への展開が可能になる
- MCP と並存し、ユースケースに応じた選択肢を提供する

## 前提条件
- なし（既存の検索機能をそのまま HTTP API で公開する）

## BDD 受け入れシナリオ

```gherkin
Scenario: HTTP 経由で検索できる
  Given `shiotsuchi serve` がポート 7171 で起動している
  When ブラウザから `GET /api/search?q=project+plan` をリクエストする
  Then JSON 形式で検索結果が返される

Scenario: 統計情報を取得できる
  Given `shiotsuchi serve` が起動している
  When `GET /api/stats` をリクエストする
  Then インデックス済みファイル数やVault 情報が返される

Scenario: CORS リクエストが許可される
  Given `shiotsuchi serve` が起動している
  When Origin ヘッダが `http://localhost` であるリクエストが来る
  Then `Access-Control-Allow-Origin` ヘッダが含まれたレスポンスが返される

Scenario: ポートが既に使用中の場合にエラーになる
  Given ポート 7171 が他のプロセスで使用中である
  When `shiotsuchi serve` を実行する
  Then ポート番号を指定するようエラーメッセージが表示される
```

## 受け入れ基準
- [ ] `shiotsuchi serve` コマンドが追加される
- [ ] デフォルトポートは 7171（`--port` オプションで変更可能）
- [ ] `GET /api/search?q=<query>&limit=<n>` で検索結果を JSON で返す
- [ ] `GET /api/stats` で統計情報を JSON で返す
- [ ] `GET /api/list` でインデックス済みファイル一覧を JSON で返す
- [ ] CORS が `localhost` にのみ許可される
- [ ] 設定ファイルの `[server]` セクションでポートを変更可能
- [ ] 起動時に URL をログに出力する
- [ ] Ctrl+C でグレースフルシャットダウンする

## テスト戦略（t_wada スタイル）

### Unit Test
- HTTP ハンドラ関数のレスポンス形式テスト
- CORS ヘッダの生成ロジックテスト

### Integration Test
- サーバー起動 → リクエスト → レスポンス検証の E2E テスト
- ポート競合時のエラーハンドリングテスト

## 実装アプローチ

### 使用ライブラリ
- **axum**: HTTP フレームワーク（tokio 生態系、軽量）
- **tower-http**: CORS ミドルウェア

### エンドポイント設計

| メソッド | パス | レスポンス |
|---------|------|-----------|
| GET | `/api/search?q=<query>&limit=<n>` | `{"results": [...], "count": n}` |
| GET | `/api/stats` | `{"total_files": n, "total_chunks": n, ...}` |
| GET | `/api/list` | `{"files": [{"path": "...", "modified_at": "..."}]}` |

### ファイル構成
```
cli/src/
  commands/
    serve.rs      # サーバーコマンド実装
  main.rs         # Commands enum に Serve を追加
core/src/
  server.rs       # axum ルーティング + ハンドラ（新規）
```

### 設定スキーマ追加
```toml
[server]
port = 7171
host = "127.0.0.1"
```

## 見積もり
8 ポイント

## 技術的考慮事項

### MCP との関係
- `shiotsuchi serve` と `shiotsuchi mcp` は **並存する**
- どちらも同一の SQLite DB を Read Only で参照する
- 使い分け: serve は UI フロントエンド向け、MCP は AI アシスタント向け

### セキュリティ
- バインドアドレスはデフォルト `127.0.0.1` のみ（外部アクセス不可）
- CORS は `localhost` のみ許可
- 認証はなし（ローカル利用を前提）

### 既存コードとの連携
- `core/src/search.rs` の `search()` 関数をそのまま利用
- `core/src/db.rs` の DB アクセスを `AppState` でラップ
- `core/src/config.rs` に `ServerConfig` を追加

---

## 実装者向け注記

### 現状コードの確認

```bash
# 既存のコマンド一覧確認
grep -n "enum Commands" cli/src/main.rs -A 30

# 検索関数のシグネチャ確認
grep -n "pub fn search" core/src/search.rs

# DB アクセス方法の確認
grep -n "pub fn open" core/src/db.rs
```

### 実装手順

1. **`core/src/config.rs` に `ServerConfig` を追加**
   ```rust
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct ServerConfig {
       pub port: u16,
       pub host: String,
   }
   
   impl Default for ServerConfig {
       fn default() -> Self {
           Self {
               port: 7171,
               host: "127.0.0.1".to_string(),
           }
       }
   }
   ```

2. **`core/src/server.rs` を新規作成**
   ```rust
   use axum::{routing::get, Router, Json, extract::State};
   use std::sync::Arc;
   
   pub struct AppState {
       pub db: crate::db::Database,
   }
   
   pub fn create_router(state: Arc<AppState>) -> Router {
       Router::new()
           .route("/api/search", get(handle_search))
           .route("/api/stats", get(handle_stats))
           .route("/api/list", get(handle_list))
           .with_state(state)
   }
   ```

3. **`cli/src/commands/serve.rs` を新規作成**
   ```rust
   use clap::Parser;
   use std::sync::Arc;
   
   #[derive(Parser)]
   pub struct ServeArgs {
       #[arg(short, long, default_value_t = 7171)]
       port: u16,
   }
   
   pub async fn execute(args: ServeArgs, config: &Config) -> anyhow::Result<()> {
       let db = Database::open(&config.db_path())?;
       let state = Arc::new(AppState { db });
       let app = create_router(state);
       
       let addr = format!("127.0.0.1:{}", args.port);
       println!("Listening on http://{}", addr);
       
       let listener = tokio::net::TcpListener::bind(&addr).await?;
       axum::serve(listener, app).await?;
       Ok(())
   }
   ```

4. **`cli/src/main.rs` の Commands enum に追加**
   ```rust
   #[derive(Subcommand)]
   enum Commands {
       // ... 既存コマンド
       /// Start HTTP API server
       Serve(commands::serve::ServeArgs),
   }
   ```

5. **CORS ミドルウェアの追加**
   ```rust
   use tower_http::cors::{CorsLayer, Any};
   
   let cors = CorsLayer::new()
       .allow_origin("http://localhost".parse::<HeaderValue>().unwrap())
       .allow_methods(Any)
       .allow_headers(Any);
   ```

### 落とし穴

1. **ポート競合時のエラーハンドリング**
   - `TcpListener::bind` が失敗した場合、ユーザーに分かりやすいエラーメッセージを表示する
   - `anyhow::bail!("Port {} is already in use. Specify a different port with --port", args.port)`

2. **DB の接続プーリング**
   - SQLite は同時書き込みに制限があるため、`search()` は Read Only で問題ない
   - 将来的に書き込みが必要になった場合は `r2d2` 等のコネクションプールを検討

3. **シャットダウンの処理**
   - Ctrl+C (SIGINT) をキャッチし、グレースフルシャットダウンする
   - `tokio::signal::ctrl_c()` を使用

## Definition of Done
- [ ] `shiotsuchi serve` が `--port` オプション付きで動作する
- [ ] `/api/search`, `/api/stats`, `/api/list` が正しい JSON を返す
- [ ] CORS ヘッダが正しく設定される
- [ ] テストがパスする
- [ ] コードレビュー完了
