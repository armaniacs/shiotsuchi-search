---
title: "shiotsuchi-search — 日本語Markdownノートをサブ秒で全文検索するCLIツール"
published: false
description: "インストールから検索まで。CLI・MCP・マルチvault・セマンティック検索まで対応した日本語検索エンジンの使い方"
date: 2026-06-02
tags: ["rust", "cli", "obsidian", "search-engine"]
---

[shiotsuchi-search](https://github.com/armaniacs/shiotsuchi-search) はRustで書いた日本語対応のMarkdownノート検索エンジンです。ObsidianやLogseqで溜まったノートを、ターミナルからサブ秒で全文検索できます。

```sh
cargo install shiotsuchi
shiotsuchi
```

インストールしてサブコマンドなしで起動すると、インタラクティブなウェルカム画面が表示されます。カーソルキーで操作を選ぶだけでセットアップから検索まで完了します。

:::message
スクリーンショット挿入箇所: `shiotsuchi` 起動直後のウェルカム画面
:::

## できること

| 機能 | コマンド |
|------|---------|
| ノートを全文検索 | `shiotsuchi search "クエリ"` |
| インデックス作成・更新 | `shiotsuchi index` |
| ファイル変更を自動検知 | `shiotsuchi watch` |
| タスクチェックボックスを横断検索 | `shiotsuchi tasks "レビュー"` |
| vault 統計を表示 | `shiotsuchi stats` |
| 環境診断 | `shiotsuchi doctor` |
| Claude Desktop と連携（MCP） | `shiotsuchi-mcp` |

## セットアップ

### 1. 設定ファイルを作る

```sh
shiotsuchi init
```

対話形式で設定ファイルを作成します。ノートのディレクトリと除外したいフォルダを選ぶだけです。設定ファイルは `~/.config/shiotsuchi/config.toml` に保存されます。

```toml
[database]
db_path = "~/.cache/shiotsuchi/db.sqlite3"

[vaults.personal]
notes_dir = "/Users/yaar/Notes"
```

### 2. インデックスを作る

```sh
shiotsuchi index
```

初回インデックスは数百〜数千ファイルの規模で数十秒かかります。2回目以降はSHA-256ハッシュで変更ファイルだけを再インデックスするため、ほぼ瞬時に終わります。

### 3. 検索する

```sh
shiotsuchi search "プロジェクト計画"
```

結果は該当箇所のスニペット付きで表示されます。Vaporettoによる形態素解析のおかげで、「プロジェクト」「計画」を別々の単語として認識するため、「プロジェクトの計画書」「計画を立てた」といったバリエーションもヒットします。

## 検索モード

`--mode` オプションで検索方式を切り替えられます。

| モード | 説明 | コマンド例 |
|--------|------|-----------|
| `fts`（デフォルト） | FTS5 BM25によるキーワード検索 | `shiotsuchi search "締め切り"` |
| `vec` | ベクトル類似度によるセマンティック検索 | `shiotsuchi search "明日やること" --mode vec` |
| `hybrid` | FTS + Vec の RRF マージ | `shiotsuchi search "会議" --mode hybrid` |

セマンティック検索を使うには事前に `shiotsuchi setup` でONNX埋め込みモデルをダウンロードするか、OpenAI等のAPIキーを設定します。

## マルチvault

複数のノートディレクトリを1つのデータベースで管理できます。

```toml
[vaults.personal]
notes_dir = "/Users/yaar/Notes/Personal"

[vaults.work]
notes_dir = "/Users/yaar/Notes/Work"
```

`--vault work` で特定のvaultだけを対象に検索できます。省略すると全vaultを横断します。

## Claude Desktop との連携（MCP）

`shiotsuchi-mcp` をMCPサーバーとして登録すると、Claude Desktopから自然言語でノートを検索できます。

`~/Library/Application Support/Claude/claude_desktop_config.json` に追加します。

```json
{
  "mcpServers": {
    "shiotsuchi": {
      "command": "/usr/local/bin/shiotsuchi-mcp",
      "env": {
        "SHIOTSUCHI_NOTES_DIR": "/Users/yaar/Notes",
        "SHIOTSUCHI_DB_PATH": "/Users/yaar/.cache/shiotsuchi/db.sqlite3"
      }
    }
  }
}
```

Claude Desktopを再起動して「プロジェクトについてノートを検索して」と話しかけると、shiotsuchiがノートを検索して結果を返します。

:::message
スクリーンショット挿入箇所: Claude Desktop でノート検索しているようす
:::

## ファイル変更の自動検知

```sh
shiotsuchi watch
```

`watch` コマンドを起動しておくと、ノートを保存するたびに自動でインデックスが更新されます。Obsidianで書いたそばからCLIで検索できる状態になります。

## インデックスをリセットする

セットアップに失敗した場合や、インデックスを作り直したい場合は `shiotsuchi clean` を使います。既存のデータベースを `.bak.<timestamp>` 形式でバックアップしてから、全ノートを再インデックスします。途中でエラーが起きても元のデータベースは残ります。

```sh
shiotsuchi clean
```

---

shiotsuchi-searchのコアとなる技術的な仕組み（Vaporetto×FTS5の設計）は別記事で書きます。日本語ノートの検索に困っていたら、ぜひ試してみてください。
