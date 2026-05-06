# Shiotsuchi-Search Phase 7: MCP Inspector によるインタラクティブテスト

**Goal:** MCP Inspector を使い、`shiotsuchi-mcp` が Claude などの AI クライアントからどう見えるかをブラウザ GUI で検証する。Phase 6 の自動テストでは確認できない「GUI での直感的な操作感」と「Claude Desktop への接続」を手動で確認する。

**前提条件:**
- Phase 6 完了済み — Vitest 統合テストが全件 PASS していること
- `/tmp/shiotsuchi-test-vault/` と `.db.sqlite3` が Phase 6 の手順で作成済みであること
- Node.js（`npx` が使えること）

> **Phase 6 との使い分け:**
> Phase 6（Vitest）は CI/CD で自動実行するリグレッション防止。
> Phase 7（Inspector）はプロトコルの振る舞いを目で確認し、Claude Desktop に接続するための手動デバッグ。

---

## 実装状況サマリー

### 未実施

このフェーズはすべて手動作業。自動化テストなし。

---

## MCP Inspector とは

MCP Inspector は、MCP サーバーが「Claude などの AI クライアントからどう見えるか」をシミュレートし、ブラウザ上の GUI で対話的にテストできる公式のデバッグツール。

**最大のメリット:** Claude Desktop を再起動せずにサーバーの動作を確認できる。通常、Claude Desktop にサーバーを登録するとコード修正のたびにアプリを再起動する必要があるが、Inspector であればサーバープロセスを立ち上げ直すだけで即座に反映される。

インストール不要。`npx` で直接実行する。

> **デバッグの注意:** MCP は stdio を使って通信するため、`shiotsuchi-mcp` のログは **必ず stderr** に出す必要がある。stdout に `println!` を混入させるとプロトコルが壊れる。Inspector の下部パネルで stderr ログを確認できる。

---

## Task 1: MCP Inspector を起動する

Phase 6 で作成した Vault をそのまま使う。

- [ ] **Step 1: Inspector を起動する**

```bash
# Phase 6 で構築済みの Vault を使う
SHIOTSUCHI_NOTES_DIR=/tmp/shiotsuchi-test-vault \
SHIOTSUCHI_DB_PATH=/tmp/shiotsuchi-test-vault/.db.sqlite3 \
npx @modelcontextprotocol/inspector \
  ./target/release/shiotsuchi-mcp
```

ターミナルに `URL: http://localhost:5173`（ポートは環境による）が表示される。

- [ ] **Step 2: ブラウザでアクセスする**

表示された URL をブラウザで開く。左パネルに接続状態、右パネルにツール一覧が表示される。

---

## Task 2: Tools タブの確認

### ① `tools/list` — ツール一覧

- [ ] 以下の 3 つが表示されること:
  - `search_vault` — `query: string (required)` の入力フォームが自動生成される
  - `read_full_note` — `path: string (required)` の入力フォームが自動生成される
  - `vault_status` — 引数なし

### ② `search_vault` のインタラクティブテスト

- [ ] `query` = `"プロジェクト"` → **Run Tool**

  期待するレスポンス形式:
  ```json
  {
    "content": [{
      "type": "text",
      "text": "[{\"path\":\"plan.md\",\"title\":\"プロジェクト計画\",\"snippet\":\"...\",\"score\":-0.xx}]"
    }]
  }
  ```

- [ ] `query` = `"Rust SQLite"` → `architecture.md` が上位にヒットすること

- [ ] `query` = `"zzznomatchxxx99999"` → 空配列 `[]`（`isError` なし）

- [ ] `query` = `""` → 空配列 `[]`（クラッシュしない）

### ③ `read_full_note` のインタラクティブテスト

- [ ] `path` = `"plan.md"` → ノート全文が返る

- [ ] `path` = `"../../../etc/passwd"` → `isError: true` が返る（Vault 外アクセスを拒否）

- [ ] `path` = `"/etc/passwd"` → `isError: true` が返る（絶対パスを拒否）

### ④ `vault_status` のインタラクティブテスト

- [ ] 引数なしで **Run Tool** → 以下を含むテキストが返る:
  - `Total notes: 3`
  - `DB size: NNN bytes`
  - `Last indexed: YYYY-MM-DD...`

---

## Task 3: エラーハンドリングと stderr ログの確認

- [ ] **`RUST_LOG=debug` で詳細ログを確認する**

```bash
RUST_LOG=debug \
SHIOTSUCHI_NOTES_DIR=/tmp/shiotsuchi-test-vault \
SHIOTSUCHI_DB_PATH=/tmp/shiotsuchi-test-vault/.db.sqlite3 \
npx @modelcontextprotocol/inspector \
  ./target/release/shiotsuchi-mcp
```

Inspector 下部のログパネルに Rust の `env_logger` 出力が表示されること。

- [ ] **DB が存在しない状態でのエラー確認**

Inspector を再起動し、`SHIOTSUCHI_DB_PATH` を存在しないディレクトリ内のパス（例: `/tmp/nosuchdir/no.sqlite3`）に変更する。`search_vault` を呼び出したとき、クラッシュではなく `isError: true` のレスポンスが返ること。

---

## Task 4: Claude Desktop への接続

MCP Inspector での確認が済んだら、実際の Claude Desktop に接続する。

- [ ] **Step 1: バイナリをインストールする**

```bash
make install PREFIX=~/.local   # ~/.local/bin に配置
# または
sudo make install              # /usr/local/bin に配置
```

- [ ] **Step 2: Claude Desktop の設定ファイルを編集する**

`~/Library/Application Support/Claude/claude_desktop_config.json` に追記:

```json
{
  "mcpServers": {
    "shiotsuchi": {
      "command": "/Users/yaar/.local/bin/shiotsuchi-mcp",
      "env": {
        "SHIOTSUCHI_NOTES_DIR": "/path/to/your/obsidian-vault",
        "SHIOTSUCHI_DB_PATH": "/path/to/your/obsidian-vault/.shiotsuchi.sqlite3"
      }
    }
  }
}
```

> `SHIOTSUCHI_DB_PATH` を Vault 内に置く場合は `.gitignore` に追加すること。

- [ ] **Step 3: Vault をインデックスする**

```bash
SHIOTSUCHI_MODEL_PATH=$(pwd)/models/bccwj-suw+unidic_pos+kana.model.zst \
  ./target/release/shiotsuchi chart \
    --notes-dir /path/to/your/obsidian-vault \
    --db-path /path/to/your/obsidian-vault/.shiotsuchi.sqlite3
```

- [ ] **Step 4: Claude Desktop を再起動して動作確認する**

Claude Desktop を完全終了（Quit）してから再起動する。チャットで:

```
私のノートで「プロジェクト」に関連するものを検索してください。
```

期待: `search_vault` ツールが呼び出され、関連ノートのリストが返る。

---

## チェックリスト（完了条件）

| 項目 | 確認 |
|------|------|
| Inspector で 3 つのツールが表示される | ☐ |
| `search_vault` で日本語検索が動作する | ☐ |
| `search_vault` で英語検索が動作する | ☐ |
| 空クエリ・存在しないキーワードで `isError` なし | ☐ |
| `read_full_note` でノート全文が取得できる | ☐ |
| パストラバーサル・絶対パスが `isError: true` で拒否される | ☐ |
| `vault_status` で統計情報が返る | ☐ |
| DB 不在時に `isError: true`（クラッシュしない） | ☐ |
| `RUST_LOG=debug` で stderr ログが Inspector に表示される | ☐ |
| Claude Desktop から `search_vault` が呼び出せる | ☐ |

---

## Next Steps

Phase 8（将来）: 実ユーザーフィードバックに基づく改善
- 検索精度の調整（BM25 パラメータ）
- `search_vault` への `limit` パラメータ追加
- ベンチマーク結果に基づくパフォーマンス改善
