# shiotsuchi-search MCP サーバーの使い方

shiotsuchi-search は MCP（Model Context Protocol）サーバー (`shiotsuchi-mcp`) を提供します。LLM がこのサーバーを介して Markdown vault を直接検索できます。このガイドでは、1つまたは複数の vault に対する設定とクライアント登録の手順を説明します。

> インストール手順は [docs/INSTALL.ja.md](INSTALL.ja.md) を参照してください。

---

## 基本概念

| 用語 | 意味 |
|------|------|
| **vault** | Markdown ファイルを格納したディレクトリ（Obsidian のノートフォルダなど） |
| **インデックス** | `shiotsuchi chart` で構築する SQLite データベース |
| **MCP サーバー** | `shiotsuchi-mcp` — LLM のツール呼び出しに応答する stdio プロセス |

MCP サーバー 1 プロセス = vault 1 つです。複数の vault を検索するには、vault ごとに 1 プロセスを起動してクライアントに登録します。

---

## Step 1 — vault をインデックス化する

MCP サーバーがクエリに応答するには、事前にインデックスを作成する必要があります。

```sh
shiotsuchi chart --notes-dir ~/Personal
```

2 つ目の vault:

```sh
shiotsuchi chart --notes-dir ~/Work
```

インデックス（SQLite DB）のデフォルト保存先は `~/.cache/shiotsuchi/db.sqlite3` です。vault ごとに別の DB パスを使いたい場合は、後述の設定ファイルで指定します。

---

## Step 2 — vault ごとに設定ファイルを作成する

`shiotsuchi-mcp` は `--config` で指定した TOML ファイルを読み込みます。フォーマットは CLI の設定ファイルと共通です。

### Personal vault 用

```toml
# ~/.config/shiotsuchi/personal.toml
notes_dir = "/Users/yourname/Personal"
db_path   = "/Users/yourname/.cache/shiotsuchi/personal.db"
```

### Work vault 用

```toml
# ~/.config/shiotsuchi/work.toml
notes_dir = "/Users/yourname/Work"
db_path   = "/Users/yourname/.cache/shiotsuchi/work.db"
```

それぞれの DB パスを指定して再インデックス:

```sh
shiotsuchi chart --notes-dir ~/Personal --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi chart --notes-dir ~/Work     --db-path ~/.cache/shiotsuchi/work.db
```

`--config` を省略した場合は `~/.config/shiotsuchi/config.toml` を参照し、それも存在しない場合はビルトインのデフォルト値を使います。

---

## Step 3 — MCP クライアントにサーバーを登録する

### Claude Desktop

設定ファイルを編集します。

- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`

```json
{
  "mcpServers": {
    "shiotsuchi-personal": {
      "command": "shiotsuchi-mcp",
      "args": ["--config", "/Users/yourname/.config/shiotsuchi/personal.toml"]
    },
    "shiotsuchi-work": {
      "command": "shiotsuchi-mcp",
      "args": ["--config", "/Users/yourname/.config/shiotsuchi/work.toml"]
    }
  }
}
```

Claude Desktop を再起動すると、2 つの vault が独立したツール名前空間として利用できます。

### Claude Code（CLI）

```sh
claude mcp add shiotsuchi-personal -- shiotsuchi-mcp --config ~/.config/shiotsuchi/personal.toml
claude mcp add shiotsuchi-work     -- shiotsuchi-mcp --config ~/.config/shiotsuchi/work.toml
```

登録確認:

```sh
claude mcp list
```

### 汎用 MCP クライアント

stdio MCP サーバーをサポートする任意のクライアントから直接起動できます。

```sh
shiotsuchi-mcp --config ~/.config/shiotsuchi/personal.toml
```

サーバーは stdin から JSON-RPC リクエストを受け取り、stdout にレスポンスを返します（MCP プロトコルバージョン 2024-11-05）。

---

## 利用可能なツール

接続後、LLM は vault ごとに以下の 3 つのツールを呼び出せます。

| ツール | 説明 |
|--------|------|
| `search_vault` | キーワードやフレーズでノートを検索。パス・スニペット・スコアを返す |
| `read_full_note` | vault 内の相対パスで指定したノートの全文を取得 |
| `vault_status` | インデックスの統計情報（ノート数・最終更新日時・DB サイズ）を取得 |

### 使用例

```
ユーザー: Work の Q3 予算について書いたメモを教えて
→ LLM が shiotsuchi-work で search_vault(query: "Q3 予算") を呼び出す
→ LLM が read_full_note(path: "finance/q3-review.md") でノート全文を取得する
```

```
ユーザー: Personal の写真に関する最近のメモを要約して
→ LLM が shiotsuchi-personal で search_vault(query: "写真") を呼び出す
```

---

## インデックスを最新に保つ

ノートを追加・編集したあとは `shiotsuchi chart` を再実行するか、ウォッチャーで継続的に更新します。

```sh
shiotsuchi scan --notes-dir ~/Personal --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi scan --notes-dir ~/Work     --db-path ~/.cache/shiotsuchi/work.db
```

---

## トラブルシューティング

| 症状 | 対処 |
|------|------|
| LLM が「結果なし」と返す | `shiotsuchi chart` でインデックスを（再）作成する |
| `shiotsuchi-mcp: command not found` | バイナリが `PATH` に含まれているか確認（[INSTALL.ja.md](INSTALL.ja.md) 参照） |
| 起動時に設定ファイルのパースエラー | TOML の構文を確認。`notes_dir` と `db_path` は絶対パスで記述する |
| 違う vault が検索される | 各 `mcpServers` エントリの `--config` パスを確認する |
| 追加したノートが検索されない | `shiotsuchi chart` を再実行するか `shiotsuchi scan` を起動する |

---

## 関連ドキュメント

- [README.ja.md](../README.ja.md) — プロジェクト概要、機能、コマンド一覧
- [docs/INSTALL.ja.md](INSTALL.ja.md) — バイナリのビルドとインストール
- [ref/cli.md](../ref/cli.md) — CLI コマンドとオプション一覧
- [ref/mcp.md](../ref/mcp.md) — MCP プロトコルの詳細
- [ref/architecture.md](../ref/architecture.md) — 設計とデータモデル
