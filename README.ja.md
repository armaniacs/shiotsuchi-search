# Shiotsuchi-Search

[English](README.md)

> *Guiding your path through the data tide.*

Markdownノートvault（Obsidianなど）向けの高性能日本語対応全文検索エンジン。
[Vaporetto](https://github.com/daac-tools/vaporetto) × SQLite FTS5 を基盤とする。

> **注意:** この検索エンジンは日本語テキストに最適化されています。英語などの他言語の検索品質は保証されません。

## 特徴

- **サブ秒検索**: 10,000件以上のノートを高速検索
- **日本語対応トークナイザ**: Vaporettoによる形態素解析
- **複数インターフェース**: CLI、MCP（Claude Desktop）
- **インクリメンタルインデックス**: SHA-256ハッシュで変更ファイルのみ再インデックス

> **注意:** CLI の出力とヘルプは日本語に対応しています。使用方法の詳細は [docs/CLI-USE.ja.md](docs/CLI-USE.ja.md) を参照してください。

## コマンド

| コマンド | 説明 |
|---------|------|
| `chart` | Markdownファイルをインデックス（または再インデックス） |
| `check-ignore <path>` | パスが除外ルールにマッチするか確認 |
| `clean` | データベースをバックアップ・削除し、全 vault を再インデックス |
| `config` | インデックス設定の管理（detect-noise） |
| `config-migrate` | 設定ファイルを旧形式から新形式に変換 |
| `delete <path>` | インデックスからノートを削除（ファイル自体は削除されません） |
| `dive <query>` / `search <query>` | ノートを検索（fts/vec/hybrid モード、フィルタ、MMR） |
| `doctor` | 環境ヘルスチェックとインタラクティブ修復 |
| `dredge` | 旧バージョンの vault をチャンク形式に移行 |
| `init` | 設定ファイルの作成（対話型除外選択） |
| `log` | インデックス履歴を表示 |
| `scan` | ファイル変更を監視して自動再インデックス |
| `setup` | ONNX 埋め込みモデルのダウンロード・確認 |
| `synonym` | 同義語辞書の管理（追加・削除・一覧） |
| `tasks` | 全 vault のタスクチェックボックスを横断検索 |
| `tide` | vault 統計情報を表示（--json 対応） |
| `support` | ビルド情報と依存バージョンを表示 |
| `scan` | ファイル変更を監視して自動再インデックス |
| `tide` | vault の統計情報を表示 |

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
shiotsuchi chart --notes-dir ~/Notes
```

Claude Desktopを再起動して「プロジェクトについてノートを検索して」と聞いてみる。

## セキュリティとプライバシー

- データベースファイル（`db.sqlite3`）には、ノート本文（検索用にトークン化された形式）が**平文**で保存されます。ボルトに機密情報が含まれる場合は、適切なファイルパーミッション（例：`chmod 600`）を設定してください。
- MCP サーバーはボルトへの読み取り専用アクセスを公開します。信頼できる MCP クライアントのみに接続してください。

## 設定

`~/.config/shiotsuchi/config.toml`（`$XDG_CONFIG_HOME` が設定されている場合は `$XDG_CONFIG_HOME/shiotsuchi/config.toml`）:

```toml
[database]
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[vaults.default]
notes_dir = "/home/name/Notes"

[indexing]
snippet_lines = 3
max_snippet_chars = 1000
include_extensions = ["md", "markdown"]
exclude_dirs = ["node_modules"]
```

複数の vault を単一のデータベースで管理することもできます:

```toml
[database]
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[vaults.personal]
notes_dir = "/home/name/Documents/Personal"

[vaults.work]
notes_dir = "/home/name/Documents/Work"
```

> **旧形式:** v0.4.0 未満の設定ファイルは `[vault] notes_dir` / `[vault] db_path` 形式で、引き続き読み取り可能です。
> `shiotsuchi config-migrate` を実行すると新形式にアップグレードできます。

## パフォーマンス

| 指標 | 目標値 | 備考 |
|------|--------|------|
| インデックス速度 | ≥ 100 ファイル/秒 | SSD |
| 検索（1,000件） | ≤ 50ms | AND検索 |
| インデックス時メモリ | ≤ 100MB | ストリーミング処理 |

ベンチマーク実行:

```bash
SHIOTSUCHI_MODEL_PATH=models/bccwj-suw+unidic_pos+kana.model.zst \
  cargo bench -p shiotsuchi-core
```

## 詳細ドキュメント

- [docs/INSTALL.ja.md](docs/INSTALL.ja.md) — `cargo install` またはソースからのビルドとインストール
- [docs/CLI-USE.ja.md](docs/CLI-USE.ja.md) — CLI コマンド詳細リファレンス
- [docs/MCP-SETUP.ja.md](docs/MCP-SETUP.ja.md) — マルチvault MCP セットアップガイド
- [docs/FTS5.ja.md](docs/FTS5.ja.md) — FTS5 クエリ構文とヒント
- [CHANGELOG.md](CHANGELOG.md) — リリース履歴
- [ref/architecture.md](ref/architecture.md) — 設計とデータモデル

## ライセンス

Apache License 2.0

リリースバイナリには Vaporetto モデル `bccwj-suw+unidic_pos+kana.model.zst` が埋め込まれており、
このモデルは BSD-3-Clause ライセンスの下で提供されています。詳細は [docs/MODEL_LICENSES.md](docs/MODEL_LICENSES.md) を参照。
