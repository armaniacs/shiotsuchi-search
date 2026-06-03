# PBI: HTTP API 認証メカニズム追加

## ユーザーストーリー
システム管理者として、HTTP API サーバーに認証機能がほしい、なぜなら `--host 0.0.0.0` で外部バインドした場合、ノートデータが無認証でアクセス可能なため

## ビジネス価値
- 外部ネットワーク公開時のセキュリティ保護
- GDPR Article 32（適切な技術的保護措置）への対応
- ポートフォワーディングや Docker 環境での安全な利用

## 前提条件
- HTTP API サーバー (`shiotsuchi serve`) が実装済みであること

## BDD 受け入れシナリオ

```gherkin
Scenario: API キー認証でリクエストが許可される
  Given 環境変数 `SHIOTSUCHI_SERVER_API_KEY` に API キーが設定されている
  And `shiotsuchi serve --host 0.0.0.0` が起動している
  When `X-API-Key: <valid-key>` ヘッダー付きで `GET /api/v1/health` をリクエストする
  Then 200 OK レスポンスが返される

Scenario: API キーなしでリクエストが拒否される
  Given 環境変数 `SHIOTSUCHI_SERVER_API_KEY` に API キーが設定されている
  And `shiotsuchi serve --host 0.0.0.0` が起動している
  When API キーヘッダーなしで `GET /api/v1/health` をリクエストする
  Then 401 Unauthorized レスポンスが返される

Scenario: localhost バインド時は認証不要
  Given `shiotsuchi serve` がデフォルト（127.0.0.1）で起動している
  When API キーヘッダーなしで `GET /api/v1/health` をリクエストする
  Then 200 OK レスポンスが返される

Scenario: SHIOTSUCHI_API_KEY と SHIOTSUCHI_SERVER_API_KEY は分離されている
  Given 環境変数 `SHIOTSUCHI_API_KEY` に埋め込み用キーが設定されている
  And 環境変数 `SHIOTSUCHI_SERVER_API_KEY` が未設定
  And `shiotsuchi serve --host 0.0.0.0` が起動している
  When API キーヘッダーなしで `GET /api/v1/health` をリクエストする
  Then 401 Unauthorized レスポンスが返される
```

## 受け入れ基準
- [ ] `SHIOTSUCHI_SERVER_API_KEY` 環境変数が設定されている場合、外部バインド時に API キー認証が有効になる
- [ ] `X-API-Key` ヘッダーでリクエスト認証を行う
- [ ] localhost バインド時は認証がスキップされる
- [ ] 認証失敗時は `401 Unauthorized` + `{"error": {"code": "UNAUTHORIZED", "message": "..."}}` を返す
- [ ] `SHIOTSUCHI_SERVER_API_KEY` は HTTP サーバー専用。埋め込み API 用の `SHIOTSUCHI_API_KEY` とは完全に分離
- [ ] API キーは config.toml に保存せず、環境変数のみで管理する

## テスト戦略（t_wada スタイル）

### Unit Test
- 認証ミドルウェアのテスト（有効キー、無効キー、キーなし）
- localhost バインド時のスキップ確認

### Integration Test
- サーバー起動 → 認証付きリクエスト → レスポンス検証

## 実装アプローチ

### 使用ライブラリ
- `tower-http` のミドルウェアパターンで実装

### 設計
```
SHIOTSUCHI_SERVER_API_KEY 環境変数
    ↓
AppState { api_key: Option<String> }
    ↓
auth_middleware (X-API-Key チェック)
    ↓
create_router (既存のルーティング)
```

- `AppState` に `api_key: Option<String>` を追加
- `SHIOTSUCHI_SERVER_API_KEY` から読み込み（`SHIOTSUCHI_API_KEY` とは別）
- localhost バインド時は `api_key` を `None` にし、認証をスキップ
- axum の `Extension` で API キーをハンドラに渡す

## 見積もり
5 ポイント

## 技術的考慮事項

### 環境変数の分離設計

| 環境変数 | 用途 | 例 |
|---------|------|-----|
| `SHIOTSUCHI_API_KEY` | 埋め込み API / VLM API（外部サービスへの送信） | OpenAI, Anthropic, Gemini |
| `SHIOTSUCHI_SERVER_API_KEY` | HTTP サーバー認証（受信リクエストの保護） | クライアントからのアクセス制御 |

**なぜ分離するか:**
- `SHIOTSUCHI_API_KEY` は既に埋め込み API (`api_embedder.rs`) と VLM (`vlm.rs`) で使用中
- 同じキーを「送信認証」と「受信認証」に使い回すのはセキュリティ原則に反する
- ユーザーが埋め込み API 用に設定したキーが、意図せず HTTP サーバーの認証キーとしても機能する危険がある

### セキュリティ
- API キーは平文でログに出力しない
- レート制限との組み合わせ推奨
- HTTPS は別途プロキシ（nginx 等）で対応

### 既存コードとの連携
- `cli/src/commands/serve.rs` に `--api-key` オプション追加（または環境変数のみ）
- `core/src/server/handlers.rs` に認証レイヤーを追加
- `core/src/server/mod.rs` で `auth_middleware` を定義
- `core/src/server/types.rs` に `ApiError::Unauthorized` を追加

## Definition of Done
- [ ] 認証付きリクエストが 200 を返す
- [ ] 認証なしリクエストが 401 を返す
- [ ] localhost バインド時は認証不要
- [ ] `SHIOTSUCHI_API_KEY` と `SHIOTSUCHI_SERVER_API_KEY` が分離されている
- [ ] テストがパスする
