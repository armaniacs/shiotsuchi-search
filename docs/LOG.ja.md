# ログの読み方・使い方 — shiotsuchi

本ドキュメントでは `shiotsuchi` のログ出力の読み方、制御方法、および設計判断の理由を説明します。

> **前提:** shiotsuchi は v0.4.20 以降、ログ出力に `tracing` crate を使用しています。以前の `log` + `env_logger` から移行済みです。

---

## 目次

- [基本操作: RUST_LOG](#基本操作-rust_log)
- [ログの出力先](#ログの出力先)
- [ログフォーマットの読み方](#ログフォーマットの読み方)
  - [CLI のフォーマット](#cli-のフォーマット)
  - [HTTP サーバーのフォーマット](#http-サーバーのフォーマット)
  - [MCP サーバーのフォーマット](#mcp-サーバーのフォーマット)
  - [index_directory span](#index_directory-span)
- [よくある使用例](#よくある使用例)
- [設計判断: なぜ tracing か](#設計判断-なぜ-tracing-か)
  - [なぜ log ではなく tracing か](#なぜ-log-ではなく-tracing-か)
  - [なぜ stdout ではなく stderr か](#なぜ-stdout-ではなく-stderr-か)
  - [なぜ初期化が crate ごとに違うか](#なぜ初期化が-crate-ごとに違うか)
  - [なぜ LogTracer bridge が必要か](#なぜ-logtracer-bridge-が必要か)
  - [なぜ HTTP サーバーだけ特別か](#なぜ-http-サーバーだけ特別か)

---

## 基本操作: RUST_LOG

`RUST_LOG` 環境変数でログレベルを制御します。値のフォーマットは以下の通りです:

```sh
# すべてのクレートの warn 以上を出力（デフォルト）
RUST_LOG=warn shiotsuchi index

# shiotsuchi_core の debug 以上を出力
RUST_LOG=shiotsuchi_core=debug shiotsuchi index

# 複数クレートのフィルタをカンマ区切りで指定
RUST_LOG=shiotsuchi_core=debug,shiotsuchi=info shiotsuchi index

# すべてのクレートの trace 以上（最も詳細）
RUST_LOG=trace shiotsuchi index

# tower-http のトレースログ（HTTP リクエストの詳細）
RUST_LOG=tower_http=trace shiotsuchi serve
```

利用可能なログレベル（低→高）:

| レベル | 用途 |
|--------|------|
| `error` | 回復不能なエラー。ユーザーへの表示と同時にログ出力される |
| `warn` | 回復可能な問題。検索のフォールバック、パーミッション設定失敗等 |
| `info` | 情報。HTTP リクエスト、MCP ツール呼び出し、インデックス完了等 |
| `debug` | デバッグ情報。ファイル除外理由、バックリンク更新等 |
| `trace` | トレース。現時点では未使用（将来の拡張用） |

### `--verbose` / `-v` フラグ

CLI では `--verbose` フラグを指定すると、`RUST_LOG` 未設定時のデフォルトが `warn` から `debug` に変わります。

```sh
# RUST_LOG 未設定、verbose なし → warn 以上のみ出力
shiotsuchi index

# RUST_LOG 未設定、verbose あり → debug 以上を出力
shiotsuchi index --verbose

# RUST_LOG が明示的に設定されている場合は verbose フラグより優先
RUST_LOG=info shiotsuchi index --verbose   # info 以上のみ（verbose は無視）
```

---

## ログの出力先

| サブシステム | 出力先 | 理由 |
|-------------|--------|------|
| CLI | stderr | stdout は検索結果等のユーザー向け出力用 |
| HTTP サーバー | stderr | stdout はサーバープロセス管理用（systemd 等） |
| MCP サーバー | stderr | **stdout は JSON-RPC プロトコル専用。絶対に混入してはいけない** |

### MCP サーバーの特別な注意

**MCP サーバーは絶対に stdout にログを出力してはいけません。** stdout は Claude Desktop 等の MCP クライアントとの JSON-RPC 通信に使用されます。1バイトでもログが混入するとプロトコルが破綻します。

このため MCP サーバーの `tracing-subscriber` 初期化では明示的に `.with_writer(std::io::stderr)` を指定しています。さらに defense-in-depth として `with_ansi(false)` も設定し、エスケープシーケンスがログファイルに混入するのを防いでいます。

```rust
// mcp/src/main.rs の初期化コード
tracing_log::LogTracer::init().ok();
tracing_subscriber::fmt()
    .with_writer(std::io::stderr)
    .with_ansi(false)
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
```

---

## ログフォーマットの読み方

### CLI のフォーマット

CLI では `compact()` + `with_target(false)` を指定しています。モジュールパスを省略し、1行あたり1イベントのコンパクトな形式です。

```
2026-06-06T15:00:00.123456Z WARN shiotsuchi_core::indexer: File path "..." outside vault root "..."
2026-06-06T15:00:00.123789Z WARN shiotsuchi_core::indexer: Skipping invalid exclude pattern "invalid[": glob parse error
```

フィールドの意味:

| フィールド | 例 | 説明 |
|-----------|-----|------|
| タイムスタンプ | `2026-06-06T15:00:00.123456Z` | ISO 8601 形式の UTC 時刻（マイクロ秒精度） |
| レベル | `WARN` | ログレベル。5文字で右詰め |
| モジュール | `shiotsuchi_core::indexer` | イベントを発行した Rust モジュールのパス |
| メッセージ | `File path "..." outside vault root "..."` | 自由形式のメッセージ |

### HTTP サーバーのフォーマット

HTTP サーバーでは `TraceLayer` が span ベースの構造化ログを出力します。1リクエストに対して span 開始・終了の2行が出力されます。

```
2026-06-06T15:00:00.123456Z  INFO request{request_id="a1b2c3d4-e5f6-7890-abcd-ef1234567890" method=GET path=/api/v1/health}: tower_http::trace::on_response: status=200 latency_ms=2
```

この行には構造化フィールドが含まれます:

| フィールド | 例 | 説明 |
|-----------|-----|------|
| `request_id` | `a1b2c3d4-e5f6-7890-abcd-ef1234567890` | リクエストごとに一意の UUID。クライアントが `x-request-id` ヘッダーで指定した値が伝播される |
| `method` | `GET` | HTTP メソッド |
| `path` | `/api/v1/health` | リクエストパス |
| `status` | `200` | HTTP ステータスコード |
| `latency_ms` | `2` | 処理時間（ミリ秒） |

### MCP サーバーのフォーマット

MCP サーバーではツール呼び出し時に構造化ログを出力します。

```
2026-06-06T15:00:00.123456Z  INFO shiotsuchi_mcp::handler: MCP tool called tool="search_local_notes"
```

| フィールド | 例 | 説明 |
|-----------|-----|------|
| `tool` | `"search_local_notes"` | 呼び出されたツール名 |

### index_directory span

`shiotsuchi index` 実行時、`index_directory` 関数は `#[tracing::instrument]` によって自動的に span が生成されます。

```
2026-06-06T15:00:00.000000Z  INFO index_directory{vault_count=3}: shiotsuchi_core::indexer: started
2026-06-06T15:00:10.000000Z  INFO index_directory{vault_count=3}: shiotsuchi_core::indexer: 10 inserted, 2 updated, 0 skipped, 0 errors
```

span 名の後の `{vault_count=3}` は span のフィールドで、設定されている vault の数を示します。

---

## よくある使用例

### インデックス処理の進捗確認

```sh
# インデックス処理の詳細ログを確認
RUST_LOG=shiotsuchi_core=debug shiotsuchi index
```

出力例:
```
2026-06-06T15:00:00.123456Z WARN shiotsuchi_core::indexer: Skipping invalid exclude pattern "[": glob parse error
2026-06-06T15:00:01.456789Z DEBUG shiotsuchi_core::indexer: Excluded node_modules (matched exclude pattern)
2026-06-06T15:00:05.123456Z  INFO index_directory{vault_count=2}: shiotsuchi_core::indexer: 150 inserted, 3 updated, 12 skipped, 0 errors
```

### 検索のフォールバック理由を確認

```sh
RUST_LOG=shiotsuchi_core=warn shiotsuchi search "日本語"
```

出力例:
```
2026-06-06T15:00:00.123456Z WARN shiotsuchi_core::search: Hybrid search vec component failed (embedding error), falling back to FTS only
```

### HTTP リクエストのトレース

```sh
# サーバー起動
RUST_LOG=tower_http=trace shiotsuchi serve

# 別ターミナルでリクエスト
curl -i http://localhost:7171/api/v1/health
```

サーバーの stderr 出力例:
```
2026-06-06T15:00:00.123456Z  INFO request{request_id="a1b2c3d4-e5f6-7890-abcd-ef1234567890" method=GET path=/api/v1/health}: tower_http::trace::on_response: status=200 latency_ms=2
```

レスポンスヘッダー例:
```
x-request-id: a1b2c3d4-e5f6-7890-abcd-ef1234567890
```

### MCP サーバーのデバッグ

```sh
# Claude Desktop との通信をデバッグ
RUST_LOG=info shiotsuchi-mcp

# ツール呼び出しのログのみを表示
RUST_LOG=shiotsuchi_mcp=info shiotsuchi-mcp
```

MCP サーバーの場合、stdout に JSON-RPC メッセージのみが出力され、stderr にログが出力されることを確認するには:

```sh
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | RUST_LOG=info shiotsuchi-mcp 2>/tmp/mcp.log
cat /tmp/mcp.log   # ← ここにログ
# stdout には JSON のみ
```

### レスポンスタイムの遅いエンドポイントの特定

```sh
RUST_LOG=tower_http=trace shiotsuchi serve 2>&1 | grep latency_ms
```

出力例:
```
2026-06-06T15:00:00.123456Z  INFO request{...}: tower_http::trace::on_response: status=200 latency_ms=2340
2026-06-06T15:00:01.456789Z  INFO request{...}: tower_http::trace::on_response: status=200 latency_ms=5
```

最初のリクエストが 2.3 秒かかっていることが特定できます。

---

## 設計判断: なぜ tracing か

### なぜ log ではなく tracing か

移行前は `log` crate + `env_logger` を使用していました。`tracing` に移行した理由は以下の通りです:

| 観点 | `log` | `tracing` |
|------|-------|-----------|
| 構造化データ | 非対応（文字列のみ） | 構造化フィールドをネイティブサポート |
| Span | なし | `#[instrument]` で関数実行のコンテキストを自動生成 |
| パフォーマンス | コンパイル時フィルタなし | 無効な span/イベントはゼロコスト（コンパイル時に削除） |
| エコシステム | axum/tower-http の TraceLayer と連携不可 | tower-http の TraceLayer / OpenTelemetry / Loki と統合可能 |
| 非同期 | 非対応 | `tokio` / `async` のトレースをネイティブサポート |

構造化ログが重要な理由は、ログ集約基盤（Loki, CloudWatch Logs, Datadog 等）でのクエリ実行です。

- `log`: `"Hybrid search vec component failed (embedding error), falling back to FTS only"` — 全文検索しかできない
- `tracing`: `status=200 latency_ms=2340` → `{status}"200"` でフィルタ、`latency_ms > 1000` でアラート

### なぜ stdout ではなく stderr か

CLI ツールの診断ログは**常に stderr に出力する**のが Unix 哲学の標準です。これにより、パイプラインで標準出力の結果だけを抽出できます。

```sh
# ログは stderr に流れ、検索結果だけがファイルに書き込まれる
shiotsuchi search "プロジェクト" > results.json
```

MCP サーバーの場合はさらに重要です。stdout は JSON-RPC プロトコルとして使用されるため、1バイトのログ混入も許されません。

### なぜ初期化が crate ごとに違うか

3つのバイナリクレート（cli, mcp, HTTP サーバー）は**それぞれ独立した main 関数を持つ**ため、それぞれの用途に最適化した `tracing-subscriber` の初期化を行っています。

| クレート | 初期化の特徴 | 理由 |
|---------|-------------|------|
| **cli** | `.compact().with_target(false)` + `try_from_default_env().unwrap_or_else(...)` | 人間が読むコンパクトな形式。`RUST_LOG` 未設定時は `-v` フラグで制御 |
| **mcp** | `.with_writer(stderr).with_ansi(false)` + `from_default_env()` + `LogTracer::init()` | stdout 保護が最優先。エスケープコード禁止。core の `log::` 呼び出しも捕捉 |
| **HTTP** | （サーバー側では subscriber 初期化なし。CLI の `shiotsuchi serve` が cli の subscriber を使用） | `TraceLayer` が span を生成し、CLI の subscriber が出力する |
| **core** | （ライブラリのため subscriber を初期化しない） | ライブラリクレートは subscriber を前提とせず、設定されていなければ no-op になる |

### なぜ LogTracer bridge が必要か

`tracing_log::LogTracer` は `log` crate のマクロ呼び出しを `tracing` のイベントとして転送するブリッジです。

MCP サーバーでのみ `LogTracer::init()` を呼び出しています。これは以下の理由によります:

- MCP サーバーは最も厳密な stdout 保護が必要
- `tracing-subscriber` 単体では `log::warn!` 互換性がない（`log` crate の `set_logger` を呼ばないため）
- `LogTracer::init()` により、依存クレート（`shiotsuchi-core` の移行前コード等）の `log::` 呼び出しも stderr に出力される
- CLI は独自の subscriber 初期化でカバー、core はライブラリのため不要

### なぜ HTTP サーバーだけ特別か

HTTP サーバーは `tower-http` の `TraceLayer` を使用してリクエスト/レスポンスの span を生成します。この設計は以下の理由からです:

- **リクエストID**: `SetRequestIdLayer` が各リクエストに UUID を付与。クライアントが `x-request-id` ヘッダーを指定すればその値が伝播される
- **レイテンシ計測**: `TraceLayer` がリクエストの処理時間を自動計測。ハンドラーの変更なしにログに出力可能
- **エラー追跡**: ログとレスポンスの `x-request-id` を突き合わせて、「どのリクエストが遅かったか」「どのリクエストでエラーが出たか」を特定できる

```rust
// レイヤー構成（ServiceBuilder で組み立て）
// 1. SetRequestIdLayer — UUID を生成してリクエストに付与（最外層、最初に実行）
// 2. TraceLayer      — リクエストの span を作成、レスポンス時に status/latency を記録
// 3. PropagateRequestIdLayer — request ID をレスポンスヘッダー x-request-id に伝播（最内層）
```

レイヤーの順序は重要です:
1. まず `SetRequestIdLayer` が UUID またはクライアント指定の ID をリクエストに設定
2. 次に `TraceLayer` がその ID を使って span を作成
3. 最後に `PropagateRequestIdLayer` がレスポンスに `x-request-id` ヘッダーを追加
