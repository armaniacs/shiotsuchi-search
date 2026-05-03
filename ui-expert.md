# UI/UX エキスパート レビュー結果

## スコア: 85/100

## 指摘事項

### [Medium] `scan` コマンドが起動・状態フィードバックを一切出さずにブロックする
- 場所: `cli/src/commands/scan.rs:16-31`
- 影響: ユーザーが `shiotsuchi scan` を実行するとターミナルが空白のままフリーズしたように見える。ファイル監視が開始されたのか、プロセスがハングしたのか判断できず、不安を与える。長時間プロセスに対する最低限のフィードバックが欠如している。
- 対処: 監視開始時に `Watching <notes_dir> for changes. Press Ctrl+C to stop.` のような1行を標準出力する。

### [Medium] MCP `vault_status` の `last_indexed` が人間非可読な Unix timestamp 数字列で出力される
- 場所: `mcp/src/handler.rs:45-50`
- 影響: Claude Desktop 等の MCP クライアント経由でユーザーが統計を確認した際、`Last indexed: 1714723200` のような生の Unix タイムスタンプが返る。同じ情報を出力する CLI `tide` コマンドでは `YYYY-MM-DD HH:MM:SSZ` 形式（`format_timestamp`）なのに、MCP インターフェースでフォーマットが統一されていない。ユーザーは最終インデックス時刻が読めない。
- 対処: `cli/src/commands/log.rs` の `format_timestamp` と同じ日時フォーマットに統一する。

### [Medium] CLI `dive` のデフォルト出力が人間にとって読みにくい JSON のみ
- 場所: `cli/src/commands/dive.rs:35-44`
- 影響: `--json` フラグなしでも `serde_json::to_string_pretty` で JSON 配列を出力する。README のクイックスタートでは人間が直接 `shiotsuchi dive "project plan"` を実行する例になっており、検索結果の path/title/snippet/score が入り組んだ構造で表示されると、ターミナル上で探しにくく可読性が低い。
- 対処: デフォルトで人間向けフォーマット（例: 箇条書きや簡易テーブル）を採用し、`--json` で構造化データを出力するように切り替える。

## 確認済みの良好点
- `dive` 実行時に DB が未作成の場合、`Run shiotsuchi chart to index your vault first.` と具体的な次のアクションを案内するエラーメッセージが親切で一貫している。
- MCP ツール `search_vault` / `read_full_note` / `vault_status` の description と inputSchema が明確で、Claude Desktop 側のツール利用了承フローで役立つ。
- `read_full_note` でのパストラバーサル対策（`..` や `/` の拒否、canonicalize 後のプレフィックス検証）とエラーメッセージが的確。
- README（英語/日本語）の構成・用語・手順が対応しており、文書間の一貫性が保たれている。
