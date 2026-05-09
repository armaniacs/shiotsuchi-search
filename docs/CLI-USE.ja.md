# CLI の使い方 — shiotsuchi

`shiotsuchi` は Markdown ノート vault のインデックス作成・検索・ウォッチャー機能を提供するコマンドラインツールです。

> インストール手順は [docs/INSTALL.ja.md](INSTALL.ja.md) を参照してください。

---

## クイックスタート

```sh
# 1. 設定ファイルを作成する
shiotsuchi init --notes-dir ~/Notes

# 2. vault をインデックス化する
shiotsuchi chart

# 3. 検索する
shiotsuchi dive "プロジェクト計画"
```

> すべてのコマンドに `--verbose` フラグが利用可能です。デバッグレベルのログ（ファイルごとの処理詳細、SQL クエリなど）を出力します。トラブルシューティング時に便利です。

---

## コマンド

### `init` — 設定ファイルを作成する

`~/.config/shiotsuchi/config.toml`（または `$XDG_CONFIG_HOME/shiotsuchi/config.toml`）をデフォルト設定で生成します。TTY で対話的に実行すると、vault をスキャンして除外候補（`node_modules` や `dist`、`templates` などのディレクトリ）を検出し、2段階の選択 UI を提示します。CI やスクリプトなどの非対話環境では `--yes` を使ってすべての候補を自動承認できます。

```sh
# 対話モード（デフォルト）
shiotsuchi init --notes-dir ~/Notes

# 非対話モード（CI・スクリプト向け）
shiotsuchi init --notes-dir ~/Notes --yes

# 既存設定を最新候補で再生成
shiotsuchi init --notes-dir ~/Notes --force --yes
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--notes-dir` | `.` | config に保存する vault のルートディレクトリ |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | config に保存するデータベースのパス |
| `--force` | オフ | 既存の設定ファイルを上書きする（タイムスタンプ付き `.bak` バックアップを作成） |
| `--yes` | オフ | 非対話モード: 検出した除外候補をすべて自動承認 |

---

### `chart` — vault をインデックス化する

vault 内のすべての `.md` ファイルを走査し、内蔵の Vaporetto モデルでトークナイズして SQLite インデックスを構築します。

```sh
shiotsuchi chart --notes-dir ~/Notes
```

`chart` を再実行しても安全です。ファイルハッシュを比較し、変更があったファイルだけを更新します。

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--notes-dir` | `.` | vault のルートディレクトリ |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | SQLite インデックスのパス |
| `--verbose` | オフ | ファイルごとの処理状況を表示 |
| `--quiet` | オフ | サマリー出力を抑制 |

---

### `dive` — ノートを検索する

インデックスに対して全文 AND 検索を実行し、スニペット付きの結果を返します。

```sh
shiotsuchi dive "週次レビュー"
shiotsuchi dive "Q3 予算" --limit 5
shiotsuchi dive "ミーティング" --json        # レガシー: --format json と同等
shiotsuchi dive "ミーティング" --format json-pretty
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--notes-dir` | config / `.` | スニペットのパス解決に使用 |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | 検索対象のインデックス |
| `--limit` | 20 | 最大結果件数 |
| `--json` | オフ | コンパクトな JSON 配列を出力（`--format json` のレガシー別名） |
| `--format` | `table` | 出力形式: `table` / `json` / `json-pretty` |

結果フィールド: `path`、`title`、`snippet`、`score`。

---

### `delete` — インデックスからノートを削除する

SQLite インデックスから、vault 内の相対パスで指定したノートエントリを削除します。パスは ディレクトリトラバーサル（`..`）と vault 外部への脱出を防ぐため検証されます。ファイルが既にディスク上に存在しない場合は、DB エントリを直接削除します。

```sh
shiotsuchi delete meeting/notes.md
```

| 引数 | 説明 |
|------|------|
| `<path>` | vault 内の相対パス（例: `meeting/notes.md`） |

**グローバルオプション**（すべてのコマンドで利用可能）:

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--notes-dir` | config / `.` | パス解決に使用する vault ルート |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | 操作対象のデータベース |

---

### `scan` — ファイル変更を監視する

vault ディレクトリの変更を監視し、インデックスを自動更新します。

```sh
shiotsuchi scan --notes-dir ~/Notes
```

ターミナルで常駐させるか、バックグラウンドサービスとして登録して使います。連続した編集はデバウンスしてからインデックスを更新します。

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--notes-dir` | config / `.` | 監視する vault のルート |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | 更新対象のインデックス |

---

### `tide` — vault の統計情報を表示する

ノート総数・最終インデックス日時・DB サイズを表示します。

```sh
shiotsuchi tide
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | 統計を読み込むデータベース |

---

### `config detect-noise` — 除外候補をスキャンする

vault をスキャンして既知のノイズパターンに一致するディレクトリ、または多くの Markdown ファイルを含むディレクトリを検出し、人間が読める形式でレポートを出力します。設定ファイルは**変更しません** — 検出した候補を反映するには `shiotsuchi init --force` を実行してください。

```sh
shiotsuchi config detect-noise --notes-dir ~/Notes
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--notes-dir` | config | スキャン対象の vault ルート |

出力例:

```
Exclusion candidates in /Users/yourname/Notes:
  1. node_modules [known] (142 files)
  2. dist [known] (3 files)
  3. archive [known] (0 files)
  4. generated_docs [dynamic] (15 files)
```

---

### `log` — インデックス履歴を表示する

直近にインデックスされたファイルをタイムスタンプ付きで一覧表示します。

```sh
shiotsuchi log
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | 履歴を読み込むデータベース |

---

## 設定ファイル

`~/.config/shiotsuchi/config.toml`（または `$XDG_CONFIG_HOME/shiotsuchi/config.toml`）を作成しておくと、毎回フラグを指定する手間が省けます。

```toml
[vault]
notes_dir = "/Users/yourname/Notes"
db_path   = "/Users/yourname/.cache/shiotsuchi/db.sqlite3"

[indexing]
snippet_lines       = 3
include_extensions  = ["md", "markdown"]
exclude_dirs        = ["node_modules"]
auto_exclude_hidden = true
follow_links        = false
dynamic_threshold   = 5

[watcher]
debounce_ms = 500
enabled     = true
```

> **注:** `exclude_patterns` フィールドは v0.2.9 で `exclude_dirs` にリネームされました。
> 既存の config で `exclude_patterns` を使っている場合、`exclude_dirs` にキー名を変更してください。

CLI フラグは常に設定ファイルの値より優先されます。

---

## 複数の vault を使い分ける

インデックス 1 つ = vault 1 つです。`--db-path` で各コマンドを正しいインデックスに向けます。

### 例: Personal と Work の vault

インデックス作成:

```sh
shiotsuchi chart --notes-dir ~/Personal --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi chart --notes-dir ~/Work     --db-path ~/.cache/shiotsuchi/work.db
```

検索:

```sh
shiotsuchi dive "写真旅行"  --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi dive "Q3 予算"   --db-path ~/.cache/shiotsuchi/work.db
```

ウォッチャー（それぞれ別のターミナルまたはバックグラウンドで実行）:

```sh
shiotsuchi scan --notes-dir ~/Personal --db-path ~/.cache/shiotsuchi/personal.db
shiotsuchi scan --notes-dir ~/Work     --db-path ~/.cache/shiotsuchi/work.db
```

---

## CLI と MCP サーバーの連携

CLI がインデックスを構築・管理し、MCP サーバーが LLM からの検索要求に応えます。

典型的なワークフロー:

1. **インデックス作成** — `shiotsuchi chart`（初回または定期実行）
2. **ウォッチャー起動** — `shiotsuchi scan`（書き込みに追従してインデックスを最新化）
3. **LLM から検索** — `shiotsuchi-mcp` が Claude などのクライアントからのツール呼び出しに応答

CLI と MCP サーバーは同じ SQLite データベースを共有します。WAL モードが有効なため、同時アクセスしても競合しません。

> MCP サーバーの設定（Claude Desktop・Claude Code CLI・汎用クライアント）は [docs/MCP-SETUP.ja.md](MCP-SETUP.ja.md) を参照してください。

---

## トラブルシューティング

| 症状 | 対処 |
|------|------|
| `command not found: shiotsuchi` | `~/.local/bin`（または `~/.cargo/bin`）を `PATH` に追加（[INSTALL.ja.md](INSTALL.ja.md) 参照） |
| `dive` で結果が返らない | `shiotsuchi chart` でインデックスを作成してから再試行する |
| `dive` でインデックスが見つからないエラー | `--db-path` が `chart` で指定したパスと一致しているか確認する |
| 追加したノートが検索されない | `chart` を再実行するか `scan` を起動してウォッチャーを有効にする |
| 設定ファイルが読み込まれない | パスが `~/.config/shiotsuchi/config.toml` になっているか確認。TOML 構文エラーは警告としてログに出力される |

---

## 関連ドキュメント

- [docs/INSTALL.ja.md](INSTALL.ja.md) — バイナリのビルドとインストール
- [docs/MCP-SETUP.ja.md](MCP-SETUP.ja.md) — MCP 経由で LLM からインデックスを検索する
- [ref/cli.md](../ref/cli.md) — コマンドリファレンス（全フラグ一覧）
- [ref/architecture.md](../ref/architecture.md) — 設計とデータモデル
