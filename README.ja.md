# Shiotsuchi-Search

[English](README.md)

> *Guiding your path through the data tide.*

Markdownノートvault（Obsidianなど）向けの高性能日本語対応全文検索エンジン。
[Vaporetto](https://github.com/daac-tools/vaporetto) × SQLite FTS5 を基盤とする。

## 特徴

- **サブ秒検索**: 10,000件以上のノートを高速検索
- **日本語対応トークナイザ**: Vaporettoによる形態素解析
- **複数インターフェース**: CLI、MCP（Claude Desktop）
- **インクリメンタルインデックス**: SHA-256ハッシュで変更ファイルのみ再インデックス

> **注意:** 現在、コマンド出力とエラーメッセージは英語のみ対応しています。日本語ローカライズは将来的に追加される可能性があります。

## クイックスタート

### 1. トークナイザモデルのダウンロード

```bash
./scripts/download-model.sh
```

### 2. vaultのインデックス作成

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  shiotsuchi chart --notes-dir ~/Notes
```

DBのデフォルト保存先: `~/.cache/shiotsuchi/db.sqlite3`（`$XDG_CACHE_HOME` が設定されている場合は `$XDG_CACHE_HOME/shiotsuchi/db.sqlite3`）

### 3. 検索

```bash
shiotsuchi dive "プロジェクト計画"
```

## コマンド

| コマンド | 説明 |
|---------|------|
| `chart` | Markdownファイルをインデックス（または再インデックス） |
| `dive <query>` | ノートを検索（AND検索、JSON出力） |
| `tide` | vault の統計情報を表示 |
| `scan` | ファイル変更を監視して自動再インデックス |
| `log` | インデックス履歴を表示 |
| `delete <path>` | インデックスからノートを削除（ファイル自体は削除されません） |

## Claude Desktop 連携（MCP）

`~/Library/Application Support/Claude/claude_desktop_config.json` に追加:

```json
{
  "mcpServers": {
    "shiotsuchi": {
      "command": "/usr/local/bin/shiotsuchi-mcp",
      "env": {
        "SHIOTSUCHI_NOTES_DIR": "/home/name/Notes",
        "SHIOTSUCHI_DB_PATH": "/home/name/.cache/shiotsuchi/db.sqlite3"
      }
    }
  }
}
```

先にvaultをインデックスしておく:

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  shiotsuchi chart --notes-dir ~/Notes
```

Claude Desktopを再起動して「プロジェクトについてノートを検索して」と聞いてみる。

## セキュリティとプライバシー

- データベースファイル（`db.sqlite3`）には、ノート本文（検索用にトークン化された形式）が**平文**で保存されます。ボルトに機密情報が含まれる場合は、適切なファイルパーミッション（例：`chmod 600`）を設定してください。
- MCP サーバーはボルトへの読み取り専用アクセスを公開します。信頼できる MCP クライアントのみに接続してください。

## 設定

`~/.config/shiotsuchi/config.toml`（`$XDG_CONFIG_HOME` が設定されている場合は `$XDG_CONFIG_HOME/shiotsuchi/config.toml`）:

```toml
[vault]
notes_dir = "/home/name/Notes"
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[indexing]
snippet_lines = 3
include_extensions = ["md", "markdown"]
exclude_patterns = [".obsidian", ".git", "node_modules"]
```

## ソースからビルド

```bash
git clone https://github.com/your-org/shiotsuchi-search
cd shiotsuchi-search
make build
```

`make build` はトークナイザモデルが未ダウンロードの場合は自動取得し、モデルを埋め込んだリリースバイナリをビルドする。

ビルド成果物（`target/release/`）:
- `shiotsuchi` — CLI
- `shiotsuchi-mcp` — Claude Desktop MCP サーバー

### インストール

```bash
make install                    # /usr/local/bin にインストール
make install PREFIX=~/.local    # ~/.local/bin にインストール
```

### make ターゲット一覧

| ターゲット | 内容 |
|-----------|------|
| `make build` | リリースバイナリをビルド（モデル埋め込み） |
| `make test` | 全ワークスペーステストを実行 |
| `make bench` | Criterion ベンチマークを実行 |
| `make install` | `$(PREFIX)/bin` にバイナリをインストール |
| `make uninstall` | インストール済みバイナリを削除 |
| `make clean` | ビルド成果物を削除 |

## テストの実行

```bash
make test
```

## パフォーマンス

| 指標 | 目標値 | 備考 |
|------|--------|------|
| インデックス速度 | ≥ 100 ファイル/秒 | SSD |
| 検索（1,000件） | ≤ 50ms | AND検索 |
| インデックス時メモリ | ≤ 100MB | ストリーミング処理 |

ベンチマーク実行:

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  cargo bench -p obsidian-shiotsuchi-vault-core
```

## ライセンス

Apache License 2.0

リリースバイナリには Vaporetto モデル `bccwj-suw+unidic_pos+kana.model.zst` が埋め込まれており、
このモデルは BSD-3-Clause ライセンスの下で提供されています。詳細は [docs/MODEL_LICENSES.md](docs/MODEL_LICENSES.md) を参照。
