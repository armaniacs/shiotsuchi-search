# CLI の使い方 — shiotsuchi

`shiotsuchi` は Markdown ノート vault のインデックス作成・検索・ウォッチャー機能を提供するコマンドラインツールです。

> インストール手順は [docs/INSTALL.ja.md](INSTALL.ja.md) を参照してください。

---

## インタラクティブウェルカム画面

サブコマンドを指定せずに `shiotsuchi` だけを実行すると、インタラクティブなウェルカム画面が表示されます:

```sh
shiotsuchi
```

以下の機能を提供します:

- **ウェルカムバナー** — クイックスタートガイド（init → index → search）を表示
- **オンボーディングウィザード** — セットアップ手順をステップバイステップで案内
- **カテゴリ別コマンドメニュー** — コマンド名を覚えていなくても選択して実行可能

ウェルカム画面は環境に応じて内容が変わります:

| 状態 | 動作 |
|------|------|
| 初回起動（設定ファイルなし） | 「🚀 オンボーディングを開始」を表示 → init → index → search を一緒に実行 |
| 設定ファイルあり、DB なし | 「⚡ オンボーディングを続ける」を表示 → 設定作成をスキップして index → search から開始 |
| 設定ファイル＋DB あり | 「🚀 クイックオンボーディング」を表示 → 再インデックス＋検索体験 |

カーソルキーでメニュー項目を選択し、Enter で決定します。`init`、`index`、`search` は実行後に「次のステップに進みますか？」と確認し、スムーズに作業を続けられます。

> パイプや CI などの非 TTY 環境では、インタラクティブメニューの代わりにテキストガイダンスが表示されます。

## クイックスタート

```sh
# 1. 設定ファイルを作成する
shiotsuchi init --notes-dir ~/Notes

# 2. vault をインデックス化する
shiotsuchi index

# 3. 検索する
shiotsuchi search "プロジェクト計画"
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

### `index` — vault をインデックス化する

vault 内のすべての `.md` ファイルを走査し、内蔵の Vaporetto モデルでトークナイズして SQLite インデックスを構築します。

```sh
shiotsuchi index --notes-dir ~/Notes
```

`index` を再実行しても安全です。ファイルハッシュを比較し、変更があったファイルだけを更新します。

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--notes-dir` | `.` | vault のルートディレクトリ |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | SQLite インデックスのパス |
| `--verbose` | オフ | ファイルごとの処理状況を表示 |
| `--quiet` | オフ | サマリー出力を抑制 |
| `--vault` | — | 特定のボールトのみインデックスする（例: `--vault work`） |

### 除外パターン

インデックス対象からファイルを除外するには、以下の 2 つの方法があります（どちらも同じ glob 構文を使用）：

1. **`config.toml`:** `[indexing]` セクションの `exclude_dirs` に設定します。
2. **`.shiotsuchiignore`:** Vault ルートディレクトリに `.shiotsuchiignore` ファイルを配置します。

パターンは `*`（任意の文字列）、`**`（再帰的）、`?`（任意の1文字）、`[abc]`（文字クラス）をサポートします。

```sh
# .shiotsuchiignore の例
node_modules
*.tmp
private/
draft_*
```

両方のソースのパターンはインデックス時にマージされます。

### `check-ignore` — 除外パターンの診断

指定された相対パスが `exclude_dirs` または `.shiotsuchiignore` によって除外されるかを確認します。

```sh
shiotsuchi check-ignore "node_modules/foo.md"
# ✗ 除外: node_modules/foo.md
#   理由: config.toml の exclude_dirs (pattern: node_modules)

shiotsuchi check-ignore "doc/manual.md"
# ✓ 除外なし: doc/manual.md
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `<パス>` | — | 確認する相対パス（例: `private/notes.md`） |
| `--vault` | 最初のボールト | 除外ルールを確認するボールト |

---

### `search` — ノートを検索する

全文キーワード検索（FTS5 BM25）、ベクトル検索（セマンティック）、またはハイブリッドモードでインデックスを検索します。検索結果はファイルパス・見出し・スニペットとともに表示されます。

```sh
shiotsuchi search "週次レビュー"
shiotsuchi search "Q3 予算" --limit 5
shiotsuchi search "プロジェクト計画" --mode vec         # ベクトル検索
shiotsuchi search "会議" --mode hybrid --alpha 0.3       # vec 重視ハイブリッド
shiotsuchi search "アプリ開発" --fuzzy                    # あいまい検索
shiotsuchi search "計画" --tag project --since 2026-01-01  # フロントマターフィルタ
shiotsuchi search "AWS" --mmr --lambda 0.7               # 多様化リランキング
shiotsuchi dive "プロジェクト計画"                       # search のエイリアス（旧名）
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--limit` | 20 | 最大結果件数 |
| `--mode` | `hybrid`（モデルなしの場合は `fts`） | 検索モード: `fts` / `vec` / `hybrid` |
| `--format` | `table` | 出力フォーマット: `table` / `json` / `json-pretty` |
| `--vault` | — | 特定のボールトに絞り込む（例: `--vault work`） |
| `--tag` | — | フロントマターのタグで絞り込む（例: `--tag project`） |
| `--since` | — | フロントマターの日付で絞り込む、ISO 8601 形式（例: `--since 2026-01-01`） |
| `--fuzzy` | off | Unicode NFKC 正規化 + 大文字小文字の正規化を行い表記揺れを吸収 |
| `--alpha` | 0.5 | ハイブリッドのブレンド比率（0.0=ベクトルのみ、1.0=FTS のみ） |
| `--mmr` | off | MMR 多様化リランキングを有効化 |
| `--lambda` | 0.5 | MMR の関連性と多様性のバランス（0.0=多様性重視、1.0=関連性重視） |
| `--threshold` | — | 最小スコア閾値。FTS/Vec: 閾値以上のスコアを除外。Hybrid: 閾値未満のスコアを除外。 |
| `--model-path` | — | ONNX 埋め込みモデルファイルのパス（設定・環境変数を上書き） |

> **ANSI ハイライト:** マッチした検索語はテーブル形式出力で強調表示されます。`NO_COLOR=1` またはパイプへのリダイレクトで色を無効化できます。

**検索モード:**

| モード | 説明 | モデル |
|--------|------|--------|
| `fts` | FTS5 BM25 によるキーワード検索。全角/半角正規化対応。 | 不要 |
| `vec` | ベクトル類似度によるセマンティック検索。`--model-path` または設定が必要。 | 必須 |
| `hybrid` | デフォルト。FTS + Vec の Reciprocal Rank Fusion（RRF）。モデルがない場合は FTS にフォールバック。 | 任意 |

**MMR（Maximal Marginal Relevance）:**

`--mmr` を有効にすると、関連性と多様性を両立するよう結果がリランキングされます。Lambda でバランスを調整:
- `--lambda 1.0`: 関連性のみ（通常の順位と同じ）
- `--lambda 0.5`: 均等バランス（デフォルト）
- `--lambda 0.0`: 多様性最大化

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

### `watch` — ファイル変更を監視する

vault ディレクトリの変更を監視し、インデックスを自動更新します。

```sh
shiotsuchi watch --notes-dir ~/Notes
```

ターミナルで常駐させるか、バックグラウンドサービスとして登録して使います。連続した編集はデバウンスしてからインデックスを更新します。

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--notes-dir` | config / `.` | 監視する vault のルート |
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | 更新対象のインデックス |
| `--vault` | — | 特定のボールトのみ監視する（例: `--vault work`） |

---

### `stats` — vault の統計情報を表示する

ノート総数・最終インデックス日時・DB サイズ・タグ TOP10・総文字数を表示します。

```sh
shiotsuchi stats
shiotsuchi stats --json   # JSON 出力
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | 統計を読み込むデータベース |
| `--json` | off | 統計情報を JSON 形式で出力 |

---

### `synonym` — 同義語辞書を管理する

FTS5 クエリ展開のための同義語（シソーラス）エントリを管理します。エントリは `~/.config/shiotsuchi/thesaurus.toml` に保存されます。

```sh
shiotsuchi synonym add AWS "Amazon Web Services"
shiotsuchi synonym add AWS "アマゾンウェブサービス"
shiotsuchi synonym list
shiotsuchi synonym remove AWS
```

辞書ファイルは初回使用時に自動生成されます。エントリは起動時に `config.toml` の synonyms とマージされます（専用ファイルが優先）。

| サブコマンド | 説明 |
|-------------|------|
| `add <単語> <同義語>...` | 同義語ペアを追加（単語 → 1つ以上の同義語） |
| `remove <単語>` | 単語のエントリを削除 |
| `list` | 登録済みの全エントリを一覧表示 |

---

### `tasks` — 全ノートのタスクを横断検索する

全インデックスノートから Markdown タスクチェックボックス（`- [ ]` / `- [x]`）を検索します。

```sh
shiotsuchi tasks                          # 未完了タスクを一覧表示
shiotsuchi tasks "レビュー"                # キーワードで絞り込み
shiotsuchi tasks --all                    # 完了済みタスクも含めて表示
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `<キーワード>` | — | タスク内容で絞り込み（部分一致） |
| `--all` | off | 完了済みタスク（`- [x]`）も含める |

---

### `clean` — データベースをバックアップして再インデックス

現在のデータベースファイルをタイムスタンプ付きでバックアップし、削除した上で全 vault をゼロから再インデックスします。

```sh
shiotsuchi clean
```

バックアップファイルはデータベースと同じディレクトリに作成されます:
- `db.sqlite3.bak.<タイムスタンプ>`
- `db.sqlite3-wal.bak.<タイムスタンプ>`（存在する場合）
- `db.sqlite3-shm.bak.<タイムスタンプ>`（存在する場合）

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | バックアップ・再作成対象のデータベース |

---

### `config-migrate` — 設定ファイルの形式をアップグレード

設定ファイルを旧 `[vault]` 形式から新 `[database]` + `[vaults.xxx]` 形式に変換します。書き換え前にタイムスタンプ付きの `.bak` バックアップを作成します。

```sh
shiotsuchi config-migrate
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--config` | `~/.config/shiotsuchi/config.toml` | 設定ファイルのパス |

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

### `list` — インデックス履歴を表示する

直近にインデックスされたファイルをタイムスタンプ付きで一覧表示します。

```sh
shiotsuchi list
```

| オプション | デフォルト | 説明 |
|-----------|-----------|------|
| `--db-path` | `~/.cache/shiotsuchi/db.sqlite3` | 履歴を読み込むデータベース |

---

### `doctor` — 環境の健全性チェックと対話的修復

設定ファイル・データベース・Vaporetto トークナイザ・ONNX エンベッダ・vault ディレクトリの各コンポーネントが正常に動作するか一括確認します。

端末で実行すると、修復可能な問題を検出した際に `[y/N]` で対話的に修復するかどうかを尋ねます。非 TTY 環境（パイプ、CI など）では従来通り診断のみ行います。

```sh
shiotsuchi doctor
```

**修復可能な問題:**

| 問題 | プロンプト | 動作 |
|------|-----------|------|
| Config の `[indexing]` に未知フィールド | 未知フィールドを削除しますか？ | 該当キーを削除し、タイムスタンプ付きバックアップを作成 |
| Config が旧 `[vault]` 形式 | 新しい形式に移行しますか？ | `[database]` + `[vaults.xxx]` 形式に変換、バックアップ作成 |
| データベースが存在しない | 今すぐインデックスを作成しますか？ | データベースを作成し全 vault をインデックス |
| データベースが開けない/壊れている | 最初から再構築しますか？ | 破損DBをバックアップ後、削除して再インデックス |

**修復不可の問題** はプロンプトを表示せずヒントメッセージのみ出力します（例: トークナイザ欠如、エンベッダ欠如、vault ディレクトリ不在）。

対話的修復の出力例:

```
[!!] Config: /home/name/.config/shiotsuchi/config.toml (parse error: unknown field `snippet_lines`)
    Remove unknown field(s) `snippet_lines` from [indexing]? [y/N] y
  Backup saved to: config.toml.bak.1712345678.000000
[ok] Config: fixed
[ok] Database: /home/name/.cache/shiotsuchi/db.sqlite3 (1,234 files, 5,678 chunks)
[ok] Tokenizer: Vaporetto model loaded
[..] Embedder: ONNX model not found (vector search disabled)
     [hint] Run `shiotsuchi setup` to install the embedder model.
[ok] Vault 'default': /home/name/Notes

All checks passed.
```

---

### `completion` — Shell 補完スクリプトの生成

`shiotsuchi` のサブコマンドとフラグに対応した補完スクリプトを出力します。シェルの rc ファイルで source してください。

```sh
# Bash
source <(shiotsuchi completion bash)

# Zsh（~/.zshrc に追加）
shiotsuchi completion zsh > /usr/local/share/zsh/site-functions/_shiotsuchi

# Fish
shiotsuchi completion fish > ~/.config/fish/completions/shiotsuchi.fish

# PowerShell
shiotsuchi completion powershell | Out-String | Invoke-Expression
```

---

## 設定ファイル

`~/.config/shiotsuchi/config.toml`（または `$XDG_CONFIG_HOME/shiotsuchi/config.toml`）を作成しておくと、毎回フラグを指定する手間が省けます。

### 新形式（v0.4.0+）

```toml
[database]
db_path = "~/.cache/shiotsuchi/db.sqlite3"
vault_default = "personal"          # 省略時のデフォルト vault

[vaults.personal]
notes_dir = "/Users/name/Documents/Personal"

[vaults.work]
notes_dir = "/Users/name/Documents/Work"

[indexing]
snippet_lines       = 3
max_snippet_chars   = 1000
include_extensions  = ["md", "markdown"]
exclude_dirs        = ["node_modules"]
auto_exclude_hidden = true
follow_links        = false
dynamic_threshold   = 5
user_dictionary     = ["Vaporetto", "shiotsuchi"]  # カスタムトークン

# 同義語（`shiotsuchi synonym` でも管理可）
[synonyms]
AWS = ["Amazon Web Services", "アマゾンウェブサービス"]

# 検索チューニング（オプション）
hybrid_alpha       = 0.5   # ブレンド比率 (0.0=vecのみ, 1.0=FTSのみ)
semantic_threshold = 0.75  # 最小スコア閾値
```

### `[vlm]` セクション

スキャン済みPDFやテキスト抽出が空になるPDFに対するVLMベースの抽出を制御します。`vlm` Cargo feature フラグとAPIキー環境変数が必要です。

| フィールド | 型 | デフォルト | 説明 |
|-----------|------|---------|------|
| `enabled` | bool | `false` | VLM抽出を有効にする |
| `provider` | string | `"openai"` | VLMプロバイダー: `openai`, `anthropic`, `bedrock`, `gemini`, `ollama` |
| `model` | string | `"gpt-4.1-nano"` | ビジョンモデル名 |
| `max_pages_per_doc` | int | — | 1ドキュメントあたりの最大ページ数（省略時は無制限） |

**設定例:**

```toml
[vlm]
enabled = true
provider = "openai"
model = "gpt-4.1-nano"
```

### 旧形式（v0.4.0 未満、読み取り互換あり）

```toml
[vault]
notes_dir = "/home/name/Notes"
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"
```

> **移行:** `shiotsuchi config-migrate` を実行すると旧 `[vault]` 形式から新形式にアップグレードできます。
> 書き換え前にタイムスタンプ付きの `.bak` バックアップが作成されます。

### `[embedder]` セクション

セマンティックインデックスに使用する埋め込みモデルを指定します。このセクションを省略するか `provider = "built-in"` にすると、標準のモデル解決順序（`SHIOTSUCHI_EMBED_MODEL_PATH` 環境変数 → `~/.local/share/shiotsuchi/model.onnx`）が使われます。

| フィールド | 型 | デフォルト | 説明 |
|-----------|-----|----------|------|
| `provider` | string | `"built-in"` | プロバイダー。`"built-in"`（内蔵・標準解決）、`"onnx-file"`（独自 ONNX ファイル）、`"api"`（OpenAI 互換 API） |
| `path` | string | — | `provider = "onnx-file"` の場合に必須。ONNX モデルファイルの絶対パス（`tokenizer.json` と同じディレクトリに配置） |
| `endpoint` | string | — | `provider = "api"` の場合に必須。OpenAI 互換 API のベース URL（例: `https://api.ai.sakura.ad.jp/v1/embeddings`） |
| `model` | string | — | `provider = "api"` の場合に必須。使用するモデル名（例: `multilingual-e5-large`） |
| `api_key` | string | — | `provider = "api"` の場合のフォールバック API キー。優先度は `SHIOTSUCHI_API_KEY` 環境変数が高く、セキュリティのためそちらの使用を推奨 |

**カスタム ONNX モデルの例:**

```toml
[embedder]
provider = "onnx-file"
path = "/path/to/my-model/model.onnx"
```

**API プロバイダーの例（さくらAI）：**

```toml
[embedder]
provider = "api"
endpoint = "https://api.ai.sakura.ad.jp/v1/embeddings"
model = "multilingual-e5-large"
```

> **セキュリティ:** `provider = "api"` を使う場合は、API キーを `config.toml` の `api_key` に書くのではなく、`SHIOTSUCHI_API_KEY` 環境変数で設定してください。config にキーが書かれていると CLI が警告を出します。

> **モデル変更について:** インデックス後にモデルを変更すると、既存のベクトル埋め込みが互換性を失います。`shiotsuchi index` で全ファイルを再インデックスしてください。インデックス時にモデル変更が検出されると警告が表示されます。

### 複数 vault の例

```toml
[database]
db_path = "/home/name/.cache/shiotsuchi/db.sqlite3"

[vaults.personal]
notes_dir = "/home/name/Documents/Personal"

[vaults.work]
notes_dir = "/home/name/Documents/Work"
```

> **注:** `exclude_patterns` フィールドは v0.2.9 で `exclude_dirs` にリネームされました。
> 既存の config で `exclude_patterns` を使っている場合、`exclude_dirs` にキー名を変更してください。

CLI フラグは常に設定ファイルの値より優先されます。

---

## 複数の vault を使い分ける

複数の vault は単一の SQLite データベースを共有します。各チャンクは `vault_name` でタグ付けされ、検索結果にはどの vault に属するかが表示されます。すべてのコマンドはデフォルトで設定済みの全 vault を対象に動作します。

### 設定例

```toml
[database]
db_path = "~/.cache/shiotsuchi/db.sqlite3"
vault_default = "personal"

[vaults.personal]
notes_dir = "/Users/name/Documents/Personal"

[vaults.work]
notes_dir = "/Users/name/Documents/Work"
```

### インデックス作成

```sh
# 両方の vault をインデックス化
shiotsuchi index
```

### 検索

すべての vault を横断して検索します。MCP ハンドラはオプションの `vault` パラメータでフィルタリングできます。

```sh
# 全 vault を検索
shiotsuchi search "Q3 予算"
```

### ウォッチャー

```sh
# 設定済みの全 vault を監視
shiotsuchi watch
```

### クリーン（バックアップ + 再インデックス）

```sh
# DB をバックアップ・削除し、全 vault を再インデックス
shiotsuchi clean
```

---

## CLI と MCP サーバーの連携

CLI がインデックスを構築・管理し、MCP サーバーが LLM からの検索要求に応えます。

典型的なワークフロー:

1. **インデックス作成** — `shiotsuchi index`（初回または定期実行）
2. **ウォッチャー起動** — `shiotsuchi watch`（書き込みに追従してインデックスを最新化）
3. **LLM から検索** — `shiotsuchi-mcp` が Claude などのクライアントからのツール呼び出しに応答

CLI と MCP サーバーは同じ SQLite データベースを共有します。WAL モードが有効なため、同時アクセスしても競合しません。

> MCP サーバーの設定（Claude Desktop・Claude Code CLI・汎用クライアント）は [docs/MCP-SETUP.ja.md](MCP-SETUP.ja.md) を参照してください。

---

## トラブルシューティング

| 症状 | 対処 |
|------|------|
| `command not found: shiotsuchi` | `~/.local/bin`（または `~/.cargo/bin`）を `PATH` に追加（[INSTALL.ja.md](INSTALL.ja.md) 参照） |
| `search` で結果が返らない | `shiotsuchi index` でインデックスを作成してから再試行する |
| `search` でインデックスが見つからないエラー | `--db-path` が `index` で指定したパスと一致しているか確認する |
| 追加したノートが検索されない | `index` を再実行するか `watch` を起動してウォッチャーを有効にする |
| 設定ファイルが読み込まれない | パスが `~/.config/shiotsuchi/config.toml` になっているか確認。TOML 構文エラーは警告としてログに出力される |

---

## 関連ドキュメント

- [README.ja.md](../README.ja.md) — プロジェクト概要、機能、コマンド一覧
- [docs/INSTALL.ja.md](INSTALL.ja.md) — バイナリのビルドとインストール
- [docs/MCP-SETUP.ja.md](MCP-SETUP.ja.md) — MCP 経由で LLM からインデックスを検索する
- [ref/cli.md](../ref/cli.md) — コマンドリファレンス（全フラグ一覧）
- [ref/architecture.md](../ref/architecture.md) — 設計とデータモデル
