# RAG 実装前分析: アーキテクチャ決定とリスク一覧

**日付:** 2026-05-12  
**対象:** plan-h6-RAG.md / plan-h7-MCP.md に基づく RAG 実装

---

## 1. アーキテクチャ決定事項

| 決定項目 | 選択 | 理由 |
|----------|------|------|
| DB 同時アクセス | 書き込み用 `Connection`（既存）＋ MCP 用読み取り専用 `Connection` を別途開く | WAL モード活用、依存追加ゼロ。`rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY` で実現 |
| スキーマ統合 | `notes_fts`/`notes_meta` 廃止、新スキーマに一本化 | テーブル 2 系統の管理分散を避ける。破壊的変更を許容 |
| ベクトル検索 | `sqlite-vec` クレート（C ソースを `cc` でビルド時コンパイル） | SQLite 1 ファイル完結。`rusqlite bundled` と同じ方式で追加の C 依存なし |
| 後方互換性 | 破壊的変更を許容 | ツール名リネーム・テーブル廃止を含むすべての変更に適用 |

---

## 2. 実装の依存順序（Critical Path）

**この順序を守ること。スキーマが固まるまで [2] 以降に触れない。**

```
[1] 新スキーマ + マイグレーション (db.rs)
        ↓
[2] chunker.rs（チャンク分割ロジック）
[3] embedder.rs（ONNX 推論パイプライン）   ← [2][3] は並走可能
        ↓
[4] 新 indexer.rs（chunker + embedder を統合）
        ↓
[5] 新 search.rs（fts / vec / hybrid の 3 モード）
        ↓
[6] CLI 更新（dive --mode、dredge、setup コマンド）
        ↓
[7] MCP 更新（search_local_notes、get_surrounding_context 等）
```

---

## 3. 実装リスクと対処方針

### R1: `fts_chunks` の DELETE 構文が特殊
`content='chunks'` を使う contentless FTS5 テーブルは通常の DELETE が使えない。

```sql
-- NG: DELETE FROM fts_chunks WHERE rowid = X
-- OK:
INSERT INTO fts_chunks(fts_chunks, rowid, content) VALUES('delete', X, old_content);
```

差分更新の実装で最も詰まりやすい箇所。削除前に `old_content` を `chunks` テーブルから取得してから FTS 削除する必要がある。

### R2: `vec_chunks` はファイル単位の一括削除不可
`vec0` テーブルは `WHERE file_path=?` のような結合削除ができない。

**対処:** `DELETE FROM chunks WHERE file_path=?` で id 一覧を先取得し、ループで `DELETE FROM vec_chunks WHERE chunk_id=?` する。トランザクション内で実施。

### R3: `user_version` マイグレーション
現行 DB は `user_version = 1`。新スキーマは `user_version = 2`。

**マイグレーション処理:**
1. `PRAGMA user_version` を読む
2. `= 1` なら旧テーブル（`notes_fts`、`notes_meta`）を DROP し、新テーブルを CREATE
3. `PRAGMA user_version = 2` を書く

**警告:** テストなしで本番 DB に当てると全インデックスが消える。必ず tempfile を使ったマイグレーションテストを先に書く。

### R4: `ort` の初期化コストは起動時 1 回だけ
ONNX Runtime セッション初期化は数百 ms かかる。

**対処:** MCP サーバー起動時に `Arc<EmbedderSession>` として初期化し、全ハンドラで共有する。リクエストごとの初期化は禁止。

### R5 / R14: モデルファイルのパス規約（統合決定）
`model.onnx` と `tokenizer.json` は**同じディレクトリ**に置く規約とする。

**パス探索の優先順位:**

| 優先度 | 方法 |
|--------|------|
| 1 | `--model-path /path/to/model.onnx`（CLI フラグ） |
| 2 | `SHIOTSUCHI_MODEL_PATH` 環境変数 |
| 3 | `~/.local/share/shiotsuchi/models/model.onnx`（XDG） |

`tokenizer.json` は常に `model.onnx` と同じディレクトリを参照する。パス解決ロジックは 1 本化。

**`shiotsuchi setup` の案内文:**
```
モデルファイルを以下のディレクトリに配置してください:
  ~/.local/share/shiotsuchi/models/

必要なファイル（HuggingFace から手動ダウンロード）:
  - model.onnx
  - tokenizer.json
```

### R6: Mean Pooling は Rust で手実装
ONNX モデル出力は `[1, seq_len, 1024]` の hidden states。

**処理フロー:**
1. `attention_mask` で有効トークンを選択
2. 加重平均（Mean Pooling）で `[1024]` ベクトルに圧縮
3. L2 正規化（`||v|| = 1` にする）→ コサイン類似度 = 内積 が成立

`ort` の出力テンソルを `ndarray` 等で処理する。

### R7 / R12: `fts_chunks` で `snippet()` / `highlight()` が使えない
contentless FTS5 テーブルは SQLite の補助関数が仕様上サポートされない。

**対処:** スニペット生成は Rust 側で実装する。`chunks.content` からクエリトークンの周辺 N 文字を切り出す。現行の `extract_snippet()` 関数を新スキーマ向けに流用・改修する。

### R8: `rebuild_index` はバックグラウンド実行必須
全ノート再ベクトル化は数分かかる可能性がある。

**対処:** `tokio::spawn` でバックグラウンド実行し、MCP は即座に「再構築を開始しました」を返す。`notifications/progress` で進捗を随時通知する。

### R9: MCP は DB を毎リクエスト開き直している（現行の問題）
`handler.rs` が `call_tool` ごとに `NoteDatabase::open()` している。

**対処:** `mcp/src/main.rs` の起動時に読み取り専用 Connection を 1 回開き、`Arc<ReadonlyDb>` としてハンドラに渡す。ハンドラのシグネチャを変更する。

### R10: `read_full_note` ツールの廃止
現行 MCP ツール `read_full_note` は設計文書に存在しない。

**対処:** 破壊的変更として廃止する。`get_surrounding_context` で代替。`tools.rs` のテストも合わせて削除。

### R11: MCP ツール名のリネーム
| 現行 | 新 |
|------|-----|
| `search_vault` | `search_local_notes` |
| `vault_status` | `index_status` |
| `read_full_note` | 廃止 |

`tools.rs`・`handler.rs`・全テストのリネームが必要。

### R13: モデルの入手方法
`shiotsuchi setup` は自動ダウンロードを行わない（`reqwest` 等の HTTP クライアントを追加しない）。

HuggingFace から手動ダウンロードを案内するのみ。`setup` コマンドの責務:
1. `~/.local/share/shiotsuchi/models/` ディレクトリを作成
2. ダウンロード先 URL と配置ファイル名を表示
3. 配置済みの場合は SHA-256 ハッシュ検証を実行

### R15: ベクトルインデックス未構築時の fallback
`scan` / `chart` 実行時にモデルファイルが見つからない場合:

**対処:** エラー終了せず、以下のメッセージを表示して FTS5 のみでインデックス・検索を継続する。

```
[warn] モデルファイルが見つかりません。ベクトル検索は無効です。
       'shiotsuchi setup' を実行してモデルを配置してください。
       キーワード検索（FTS5）のみで動作します。
```

`dive --mode vec` または `--mode hybrid` 指定時はエラー（モデルなしでは実行不可）。`--mode fts` はモデルなしでも常に動作する。

---

## 4. 設計文書への反映が必要な項目

以下は plan-h6-RAG.md / plan-h7-MCP.md に**まだ記載されていない**内容:

| 項目 | 反映先 |
|------|--------|
| `tokenizer.json` の配置規約（model.onnx と同ディレクトリ） | plan-h6-RAG.md § 7-2 |
| `fts_chunks` の contentless DELETE 構文 | plan-h6-RAG.md § 6 |
| `vec_chunks` の id ループ削除パターン | plan-h6-RAG.md § 6 |
| スニペット生成を Rust 側で行う旨 | plan-h6-RAG.md § 5 |
| MCP ツール名リネーム一覧 | plan-h7-MCP.md § 1 |
| `read_full_note` 廃止 | plan-h7-MCP.md § 1 |
| `rebuild_index` のバックグラウンド実行 | plan-h7-MCP.md § 3 |
| vec 未構築時 fallback の動作仕様 | plan-h6-RAG.md § 7-2 |
| MCP Connection を起動時 1 回だけ開く設計 | plan-h7-MCP.md § 3 |
