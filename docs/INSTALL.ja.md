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

## 方法 A — cargo でインストール（最速）

git clone 不要で、最短でインストールできます。

### crates.io から

```sh
cargo install shiotsuchi shiotsuchi-mcp
```

### git から（最新 main ブランチ）

```sh
cargo install --git https://github.com/armaniacs/shiotsuchi-search shiotsuchi shiotsuchi-mcp
```

> **実行時にモデルが必要です。** `cargo install` はビルド時に Vaporetto トークナイザモデルを
> バイナリへ埋め込みません。`shiotsuchi` を実行する前に、モデルをダウンロードして
> `SHIOTSUCHI_MODEL_PATH` 環境変数で指定してください：
>
> ```sh
> # モデルをダウンロード（curl が必要）
> curl -sL "https://github.com/daac-tools/vaporetto-models/releases/download/v0.5.0/bccwj-suw+unidic_pos+kana.tar.xz" \
>   | tar -xJf - --strip-components=1 "bccwj-suw+unidic_pos+kana/bccwj-suw+unidic_pos+kana.model.zst"
> mkdir -p ~/.local/share/shiotsuchi
> mv bccwj-suw+unidic_pos+kana.model.zst ~/.local/share/shiotsuchi/
>
> # ~/.bashrc または ~/.zshrc に追加
> export SHIOTSUCHI_MODEL_PATH="$HOME/.local/share/shiotsuchi/bccwj-suw+unidic_pos+kana.model.zst"
> ```
>
> 設定後は、下記の **動作確認** と **基本的な使い方** の手順に進んでください。

## 方法 B — ソースからビルド（モデル埋め込み、推奨）

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

### 軽量ビルド（FTS5 のみ、セマンティック検索なし）

ONNX Runtime の互換性やバイナリサイズが問題になる環境では、`semantic` フィーチャーを無効にしてビルドできます。FTS5 キーワード検索とファイル監視はそのまま使えます。

```sh
cargo install --path cli --no-default-features
```

トレードオフ:

| 機能 | 通常ビルド | `--no-default-features` |
|------|-----------|------------------------|
| FTS5 キーワード検索 | 〇 | 〇 |
| ベクトル/セマンティック検索 | 〇 | ✗ |
| ファイル監視（scan） | 〇 | 〇 |
| バイナリサイズ | 大（ONNX Runtime 含む） | 小 |

MCP サーバーの軽量ビルド:

```sh
cargo install --path mcp --no-default-features
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
[database]
db_path = "/Users/yourname/.cache/shiotsuchi/db.sqlite3"

[vaults.default]
notes_dir = "/Users/yourname/Notes"
```

利用可能なすべての設定：

```toml
[database]
db_path = "/Users/yourname/.cache/shiotsuchi/db.sqlite3"

[vaults.default]
notes_dir  = "/Users/yourname/Notes"

[indexing]
snippet_lines       = 3
max_snippet_chars   = 1000
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

## ベクトル検索（セマンティック検索）モデルについて

`dive --mode vec` / `--mode hybrid` によるセマンティック検索を使うには、別途 ONNX Embedding モデルを配置する必要があります。

### 対応モデル

| モデル | 次元数 | 備考 |
|--------|--------|------|
| [Qwen/Qwen3-Embedding-0.6B](https://huggingface.co/Qwen/Qwen3-Embedding-0.6B) | 1024 | 推奨。多言語対応、軽量 |

### ダウンロードと配置

**ONNX モデルの前提条件**

`hf` CLI ツール（`huggingface-hub` に含まれます）が必要です。まずインストールしてください：

```sh
pip install huggingface-hub "optimum[onnxruntime]" sentence-transformers
```

HuggingFace にログイン（ゲート化モデルの場合は推奨）：

```sh
hf auth login
```

**方法 A — 手動ダウンロードと変換（推奨）**

HuggingFace の Qwen3-Embedding-0.6B モデルは `model.safetensors` と `tokenizer.json` を提供していますが、事前ビルドの ONNX ファイルは含まれていません。変換が必要です：

```sh
hf download Qwen/Qwen3-Embedding-0.6B model.safetensors --local-dir /tmp/qwen3-embed
hf download Qwen/Qwen3-Embedding-0.6B tokenizer.json --local-dir /tmp/qwen3-embed

# ONNX に変換（optimum-cli を使用）
optimum-cli export onnx -m Qwen/Qwen3-Embedding-0.6B /tmp/qwen3-onnx --task sentence-similarity --library-name sentence_transformers

# OR sentence-transformers を使用:
pip install sentence-transformers
python -c "
from sentence_transformers import SentenceTransformer
model = SentenceTransformer('Qwen/Qwen3-Embedding-0.6B')
model.save('/tmp/qwen3-onnx')
"

mkdir -p ~/.local/share/shiotsuchi
cp /tmp/qwen3-onnx/model.onnx ~/.local/share/shiotsuchi/model.onnx
cp /tmp/qwen3-embed/tokenizer.json ~/.local/share/shiotsuchi/
```

**方法 B — `make onnx`**

```sh
make onnx   # モデルをダウンロードし、ONNX がなければ変換手順を表示
```

**方法 C — `make prepare`**

両方のモデルを一度にダウンロード（ONNX には huggingface-hub が必要）：

```sh
make prepare  # トークナイザー + ONNX ファイルをダウンロード
```

**注意:** ONNX 埋め込みモデルは safetensors から変換する必要があります。`make onnx` スクリプトは HuggingFace リポジトリに事前ビルドの ONNX ファイルがあるか試み、見つからない場合は `model.safetensors` をダウンロードして変換手順を表示します。

### モデルパスの解決順序

以下の順で検索し、最初に見つかったファイルが使われます：

1. `--model-path /path/to/model.onnx`（CLI フラグ、最優先）
2. 環境変数 `SHIOTSUCHI_EMBED_MODEL_PATH`
3. `~/.local/share/shiotsuchi/model.onnx`（XDG デフォルト）

### 動作確認

```sh
shiotsuchi setup --check
shiotsuchi dive --mode hybrid "検索クエリ"
```

モデルが見つからない場合、`dive` は自動的に FTS モード（キーワード検索）にフォールバックします（`--mode vec` を明示した場合はエラー）。

## トラブルシューティング

| 症状 | 対処 |
|------|------|
| `cargo install` 後に `no model available` | モデルをダウンロードして `SHIOTSUCHI_MODEL_PATH` を設定してください（上記 方法 A 参照） |
| `command not found: shiotsuchi` | `~/.local/bin`（または `~/.cargo/bin`）を `PATH` に追加 |
| `rustc: command not found` | `curl https://sh.rustup.rs -sSf \| sh` で Rust をインストール |
| `curl: command not found` | パッケージマネージャで curl をインストール |
| モデルのダウンロードが失敗する | ネットワーク環境を確認するか、`models/bccwj-suw+unidic_pos+kana.model.zst` を手動で配置して `make build` を再実行 |
| 初回ビルドが遅い | 正常です — Rust は初回に全依存クレートをコンパイルします。2 回目以降は増分ビルドで高速化されます |

## 詳細ドキュメント

- [README.ja.md](../README.ja.md) — プロジェクト概要、機能、コマンド一覧
- [ref/cli.md](ref/cli.md) — 全コマンドとオプション
- [ref/architecture.md](ref/architecture.md) — 設計とデータモデル
- [ref/mcp.md](ref/mcp.md) — Claude Desktop 向け MCP サーバーの設定
- [docs/MODEL_LICENSES.md](docs/MODEL_LICENSES.md) — バンドルされているトークナイザモデルのライセンス情報
