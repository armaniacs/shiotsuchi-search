# PBI: タスク（チェックボックス）横断検索モード

## ユーザーストーリー
複数のノートにタスクを散らばらせているユーザーとして、全ノートの未完了タスクを一覧で見たい、なぜなら今どのタスクが残っているか把握するのが困難だから

## ビジネス価値
- 全ノートの TODO を横断的に集約・検索できる
- タスク管理のために Obsidian を使うユーザーの生産性向上

## BDD 受け入れシナリオ

```gherkin
Scenario: 全ノートの未完了タスクを一覧表示する
  Given Vault 内に未完了チェックボックス `- [x]` が散在している
  When ユーザーが `shiotsuchi tasks` を実行する
  Then 全ノートの未完了タスクの一覧（ノート名・タスク内容）が表示される

Scenario: キーワードでタスクを絞り込む
  When ユーザーが `shiotsuchi tasks "レビュー"` を実行する
  Then "レビュー" を含む未完了タスクのみが表示される

Scenario: 完了済みタスクも含めて表示する
  When ユーザーが `shiotsuchi tasks --all` を実行する
  Then 完了済み `- [x]` も含めた全タスクが表示される
```

## 受け入れ基準
- [x] `shiotsuchi tasks` サブコマンドを追加する
- [x] `- [x]` と `- [x]` を解析してインデックス化する
- [x] キーワード絞り込みと `--all` フラグに対応する

## 見積もり
5 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/indexer.rs`、`core/src/db.rs`（tasks テーブル）、`cli/src/main.rs`
- タスクは専用テーブルに格納するか FTS5 の column フィルタで対応

---

## ⚠️ 実装者向け注記

### CLI コマンド名

既存コマンド体系に合わせて `shiotsuchi tasks`（または `shiotsuchi haul`）を追加する。  
コマンド名は `cli/src/main.rs` の `Commands` enum に追加する。

### 実装手順

1. **タスク抽出関数を実装する**（`core/src/indexer.rs` または新ファイル）：
   ```rust
   pub struct TaskItem {
       pub file_path: String,
       pub line_number: usize,
       pub completed: bool,
       pub content: String,  // "- [x] タスクの内容"
   }
   
   fn extract_tasks(content: &str, file_path: &str) -> Vec<TaskItem> {
       content.lines().enumerate()
           .filter_map(|(i, line)| {
               if line.trim_start().starts_with("- [x] ") {
                   Some(TaskItem { completed: false, ... })
               } else if line.trim_start().starts_with("- [x] ") {
                   Some(TaskItem { completed: true, ... })
               } else { None }
           })
           .collect()
   }
   ```

2. **`core/src/db.rs` に `tasks` テーブルを追加する**：
   ```sql
   CREATE TABLE IF NOT EXISTS tasks (
       id          INTEGER PRIMARY KEY,
       vault_name  TEXT NOT NULL,
       file_path   TEXT NOT NULL,
       line_number INTEGER NOT NULL,
       completed   INTEGER NOT NULL DEFAULT 0,
       content     TEXT NOT NULL
   );
   ```

3. **`shiotsuchi tasks` コマンドを追加する**（`cli/src/commands/tasks.rs` 新規作成）

### 落とし穴

- `- [X]`（大文字 X）も完了扱いにする（Obsidian は両方を完了として扱う）。
- タスクの `content` は `- [x] ` プレフィックスを除いた本文のみ格納する（表示時に再付与する）。
- `chart` コマンド実行時に tasks テーブルも更新する必要がある。既存の indexer フローに組み込むこと。

## Definition of Done
- [x] タスク検索のテストがパスする
- [x] コードレビュー完了
