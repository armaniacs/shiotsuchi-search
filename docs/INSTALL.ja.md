# shiotsuchi-search インストールガイド

shiotsuchi-search は、[Vaporetto](https://github.com/daac-tools/vaporetto) × SQLite FTS5 で動作する、Markdown ノートボルト向けの高速日本語対応検索エンジンです。

## 必要なもの

- **Rust** 1.75 以降 — [rustup.rs](https://rustup.rs) からインストール
- **curl** — トークナイザモデルのダウンロードに使用
- **make** — macOS・Linux 標準で利用可能

インストール済みか確認：

```sh
rustc --version   # 1.75 以上であること
cargo --version
```

## インストール手順

### 1. リポジトリをクローン

```sh
git clone https://github.com/armaniacs/shiotsuchi-search.git
cd shiotsuchi-search
```

### 2. ビルドとインストール

```sh
make install
```

このコマンド 1 つで以下をすべて行います：

1. Vaporetto トークナイザモデルを `models/` にダウンロード（未取得の場合）
2. モデルをビルド時にバイナリへ埋め込み（`SHIOTSUCHI_EMBED_MODEL`）
3. 以下の優先順でバイナリをインストール：
   - `~/.local/bin/`（通常ユーザーの場合、最優先）
   - `~/.cargo/bin/`（存在する場合）
   - `/usr/local/bin/`（root 実行時、または `sudo` 使用時）

インストール後、以下の 2 つのコマンドが使えるようになります：

| コマンド | 用途 |
|---------|-----|
| `shiotsuchi` | CLI — インデックス作成・検索・監視 |
| `shiotsuchi-mcp` | Claude Desktop 向け MCP サーバー |

### 3. 動作確認

```sh
shiotsuchi --help
```

コマンドが見つからない場合は、`~/.local/bin` を `PATH` に追加してください：

```sh
# bash / zsh — ~/.bashrc または ~/.zshrc に追加
export PATH="$HOME/.local/bin:$PATH"
```

## インストール先を変更する

`/usr/local` などに入れたい場合は `PREFIX` を指定します：

```sh
sudo make install PREFIX=/usr/local
# または任意のパス
make install PREFIX=/opt/shiotsuchi
```

## アンインストール

```sh
make uninstall
# PREFIX を指定した場合
sudo make uninstall PREFIX=/usr/local
```

## 基本的な使い方

### ボルトをインデックス化

```sh
shiotsuchi chart --notes-dir ~/Notes
```

`~/Notes` を実際の Markdown ボルトのパスに置き換えてください。`.md` ファイルを走査し、内容をトークナイズして `~/.cache/shiotsuchi/db.sqlite3` に SQLite インデックスを書き込みます。

### 検索

```sh
shiotsuchi dive "プロジェクト計画"
```

ファイルパス・タイトル・マッチしたスニペットが表示されます。

### 設定ファイル（省略可）

毎回フラグを渡さなくて済むよう、`~/.config/shiotsuchi/config.toml` を作成できます：

```toml
[vault]
notes_dir = "/Users/yourname/Notes"
```

利用可能なすべての設定：

```toml
[vault]
notes_dir  = "/Users/yourname/Notes"
db_path    = "/Users/yourname/.cache/shiotsuchi/db.sqlite3"

[indexing]
snippet_lines       = 3
include_extensions  = ["md", "markdown"]
exclude_dirs         = ["node_modules"]

[watcher]
enabled = true
```

### ファイル変更の自動監視

ノートを編集するたびにインデックスを自動更新します：

```sh
shiotsuchi scan --notes-dir ~/Notes
```

## トークナイザモデルについて

Vaporetto モデル（`bccwj-suw+unidic_pos+kana`）はビルド時にバイナリへ埋め込まれるため、実行時に別途モデルファイルは不要です。モデルだけを先にダウンロードしたい場合は `make model` を使います。

## トラブルシューティング

| 症状 | 対処 |
|------|------|
| `command not found: shiotsuchi` | `~/.local/bin`（または `~/.cargo/bin`）を `PATH` に追加 |
| `rustc: command not found` | `curl https://sh.rustup.rs -sSf \| sh` で Rust をインストール |
| `curl: command not found` | パッケージマネージャで curl をインストール |
| モデルのダウンロードが失敗する | ネットワーク環境を確認するか、`models/bccwj-suw+unidic_pos+kana.model.zst` を手動で配置して `make build` を再実行 |
| 初回ビルドが遅い | 正常です — Rust は初回に全依存クレートをコンパイルします。2 回目以降は増分ビルドで高速化されます |

## 詳細ドキュメント

- [ref/cli.md](ref/cli.md) — 全コマンドとオプション
- [ref/architecture.md](ref/architecture.md) — 設計とデータモデル
- [ref/mcp.md](ref/mcp.md) — Claude Desktop 向け MCP サーバーの設定
- [docs/MODEL_LICENSES.md](docs/MODEL_LICENSES.md) — バンドルされているトークナイザモデルのライセンス情報
