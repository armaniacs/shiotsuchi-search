# FTS5 — SQLite 全文検索エンジン

> **FTS5** (Full-Text Search version 5) は SQLite に組み込まれた全文検索エンジンです。shiotsuchi-search の検索機能の中核を担っています。

---

## FTS5 とは

FTS5 は SQLite の **仮想テーブルモジュール** であり、以下の機能を提供します：

- **全文インデックス** — 転置インデックスによる高速なキーワード検索
- **BM25 ランキング** — 組み込みの関連性スコアリング（Okapi BM25 アルゴリズム）
- **フレキシブルなクエリ構文** — 前方一致・フレーズ検索・NEAR 検索
- **インクリメンタル更新** — インデックス再構築不要で行の追加・削除が可能
- **外部依存不要** — SQLite 本体に含まれている

FTS5 は FTS4 / FTS3 の後継であり、SQLite 3.9.0（2015年10月）以降に標準搭載されています。

```sql
-- FTS5 仮想テーブルの作成
CREATE VIRTUAL TABLE notes_fts USING fts5(
    title,
    body,
    tokenize='unicode61 remove_diacritics 0'
);

-- BM25 ランキングによる検索
SELECT title, rank FROM notes_fts
WHERE body MATCH '"project" AND "planning"'
ORDER BY rank;
```

---

## なぜ Shiotsuchi Search は FTS5 を選んだのか

| 要件 | FTS5 の対応 |
|------|-------------|
| 1万ノート超でもサブ秒検索 | 転置インデックス + BM25 ランキング |
| 外部データベースサーバ不要 | SQLite は組み込み型 — 設定不要、単一ファイル |
| CLI と MCP の同時アクセス | WAL モードにより書き込み中の読み取りが可能 |
| クロスプラットフォーム | macOS / Linux / Windows ですべて動作 |
| 差分インデックス | 行の追加・削除だけで済み、インデックス再構築不要 |
| プライバシー重視 | 完全ローカル — クラウドもネットワークも不要 |

### 採用しなかった代替案

| 代替案 | 採用しなかった理由 |
|--------|-------------------|
| **Elasticsearch / Meilisearch** | 別途サーバープロセスが必要。ローカルノート検索にはオーバースペック |
| **Apache Lucene / Tantivy** | Rust 製の純正検索エンジンだが依存関係が大きい。SQLite がすでにある |
| **自作の転置インデックス** | 車輪の再発明。FTS5 は実績があり、十分に文書化されている |
| **ripgrep / grep** | 差分インデックス不可。毎回全ファイルをスキャンする必要がある |

**結論**: FTS5 は軽量・ゼロ管理でありながら、10万ノート級のボルトでも十分な速度を発揮します。

---

## アーキテクチャ：Vaporetto × FTS5

FTS5 の組み込みトークナイザ（`unicode61` など）は日本語の分かち書きに対応していません。日本語は単語の区切りにスペースを使わないため、標準的な空白分割では正しく機能しません。

Shiotsuchi Search は **2段階パイプライン** でこの問題を解決します：

```
ユーザークエリ: "プロジェクト計画"
                    │
                    ▼
         ┌─────────────────────┐
         │  Vaporetto          │  日本語トークナイザ（Rust）
         │  （プロセス内）      │  単語に分割
         └─────────┬───────────┘
                    │
                    ▼
         トークン列: "プロジェクト 計画"
                    │
                    ▼
         ┌─────────────────────┐
         │  FTS5 MATCH クエリ  │  '"プロジェクト" AND "計画"'
         │  BM25 ランキング付き │
         └─────────┬───────────┘
                    │
                    ▼
         ┌─────────────────────┐
         │  SQLite 結果セット  │  path + title + snippet + score
         └─────────────────────┘
```

この設計のポイント：

- **日本語の分割は Vaporetto が担当**（Rust プロセス内で動作）
- **転置インデックスとランキングは FTS5 が担当** — すでに分割済みのテキストを `unicode61` トークナイザで処理
- **プラットフォーム依存の .so / .dylib が不要** — カスタム FTS5 拡張を C で書いて配布する必要がない

### なぜ FTS5 のカスタムトークナイザ拡張を使わないのか

FTS5 はカスタムトークナイザを C 拡張としてロードできます。しかし、Vaporetto ベースの SQLite 拡張を配布するにはプラットフォームごとの共有ライブラリ（Linux なら `.so`、macOS なら `.dylib`）が必要になります。Rust 側でトークナイズしてからスペース区切りのトークンを FTS5 の `body` 列に格納することで、システム全体が単一のポータブルバイナリにコンパイルされます。

---

## データベーススキーマ

Shiotsuchi Search は **3テーブル構成** を採用しています：

```sql
-- FTS5 仮想テーブル（外部コンテンツ → chunks）
CREATE VIRTUAL TABLE fts_chunks USING fts5(
    tokenized_content,
    content='chunks',
    content_rowid='id',
    tokenize='unicode61 remove_diacritics 0'
);

-- チャンク本体ストレージ
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_path TEXT,
    chunk_index INTEGER,
    parent_header TEXT,
    content TEXT,
    tokenized_content TEXT,
    vault_name TEXT,
    tags TEXT,
    frontmatter_date TEXT,
    title TEXT,
    emphasized_text TEXT
);

-- インクリメンタルインデックスキャッシュ
CREATE TABLE file_cache (
    vault_name TEXT,
    path TEXT,
    hash TEXT,
    mtime INTEGER,
    model_id TEXT,
    file_size INTEGER DEFAULT 0,
    backlink_count INTEGER DEFAULT 0,
    char_count INTEGER DEFAULT 0,
    PRIMARY KEY (vault_name, path)
);
```

**なぜ3テーブルなのか？** `chunks` テーブルにコンテンツをチャンク単位で格納し、`fts_chunks` は `content='chunks'`（外部コンテンツモード）として冗長な保存を回避します。`file_cache` はファイルごとのハッシュとメタデータを追跡し、インクリメンタルインデックスを実現します。

---

## クエリ形式

ユーザークエリは Vaporetto でトークン化され、FTS5 の MATCH 構文に変換されます：

| ユーザー入力 | トークン化後 | FTS5 クエリ |
|-------------|-------------|-------------|
| `東京 検索` | `東京 検索` | `"東京" AND "検索"` |
| `プロジェクト計画` | `プロジェクト 計画` | `"プロジェクト" AND "計画"` |
| `明日の天気` | `明日 の 天気` | `"明日" AND "の" AND "天気"` |

各トークンは引用符で囲まれ、`AND` で結合されます。トークン内の引用符は `""` としてエスケープされます。

```rust
// AND クエリを構築する疑似コード
fn and_query(text: &str) -> String {
    let tokens: Vec<&str> = tokenizer.split(text).collect();
    tokens.iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}
```

---

## BM25 ランキング

FTS5 は **Okapi BM25** アルゴリズムで検索結果をランク付けします。`rank` 列の値が小さいほど一致度が高いことを示します。スコアは以下の要素に基づいて計算されます：

- **TF（Term Frequency）** — 文書内での語句の出現頻度
- **IDF（Inverse Document Frequency）** — コーパス全体での語句の希少性
- **文書長の正規化** — 短い文書ほどスコアが上がる補正

FTS5 の BM25 実装は汎用テキスト向けにチューニングされており、日本語トークン化済みのコンテンツでもそのまま良好に機能します。

---

## WAL モードによる同時アクセス

Shiotsuchi Search はデータベース接続時に SQLite の **WAL（Write-Ahead Logging）** モードを有効にします：

```rust
conn.execute_batch("PRAGMA journal_mode=wal;")?;
```

これにより以下が可能になります：

- **同時読み取り** — CLI と MCP サーバーが同時に検索できる
- **書き込みの非ブロッキング** — インデックス中でも検索がブロックされない
- **パフォーマンス向上** — fsync のオーバーヘッド低減

---

## よくある質問

### FTS5 と通常の SQLite 検索の違いは？

SQLite の標準機能でも `LIKE` 句や `GLOB` 句によるパターンマッチは可能ですが、全文インデックスは提供しません。FTS5 は `CREATE VIRTUAL TABLE ... USING fts5` で転置インデックスを構築し、BM25 によるランキングや高速な AND/OR 検索を実現します。

### FTS5は曖昧検索に対応していますか？

FTS5自体は編集距離ベースの曖昧検索をサポートしていませんが、shiotsuchiはアプリケーションレベルで `--fuzzy` フラグによるUnicode NFKC正規化とASCII小文字化による曖昧検索を提供しています。

### FTS5 のインデックスは別ファイルに保存されますか？

いいえ。FTS5 のインデックスは SQLite データベースファイル（`.sqlite3`）の中に内部的に格納されます。別途インデックスファイルが作成されることはありません。

### データベースを直接操作できますか？

はい。データベースファイルは `~/.cache/shiotsuchi/db.sqlite3`（または設定した `db_path`）にあります。以下のように直接照会できます：

```sh
sqlite3 ~/.cache/shiotsuchi/db.sqlite3
```

```sql
-- インデックス済みチャンク一覧
SELECT file_path, title FROM chunks ORDER BY rowid DESC LIMIT 10;

-- 直接検索
SELECT file_path, title, rank FROM fts_chunks
WHERE fts_chunks MATCH '"検索" AND "エンジン"'
ORDER BY rank;
```

---

## 参考リンク

- [SQLite FTS5 公式ドキュメント](https://www.sqlite.org/fts5.html)（英語）
- [Okapi BM25 — Wikipedia](https://ja.wikipedia.org/wiki/Okapi_BM25)
- [Vaporetto — Rust 製日本語トークナイザ](https://github.com/daac-tools/vaporetto)
- [アーキテクチャ概要](../ref/architecture.md)
- [コアライブラリリファレンス](../ref/core.md)
