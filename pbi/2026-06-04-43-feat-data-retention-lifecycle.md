# PBI-43: データ保持ライフサイクル管理

## ユーザーストーリー
shiotsuchi 利用者として、インデックスデータの保持期間を設定し古いデータを自動削除することがほしい、なぜならノートを削除しても DB にチャンクが残り続けるためプライバシーとストレージ効率の観点で問題があるから

## ビジネス価値
- GDPR/CCPA 対応の基盤（データ削除権への追従）
- 長期的な DB 肥大化の防止
- 削除済みノートのデータが永続しないことへのユーザー信頼

## 発端
Checking Team レビュー（`plans/2026-05-27-0530-review-improve-branch.md`）の High 指摘。データ保持・削除のライフサイクル管理が存在しない。

## 前提条件
- `file_cache` にファイルパスとタイムスタンプが記録されている
- マルチ Vault 対応済み（vault ごとに保持期間を設定可能）

## 設計判断（Linear DEV-37 コメントより）

> **`retention_days` は明示指定必須。デフォルトでは削除しない。**
>
> @iron: 「古いデータの自動削除は確かに大事だが、それは90日とか1年とか明示するようにしてほしい。明示しない場合には、削除しない。デフォルトは削除しないこととする。」

つまり:
- `retention_days` が未設定（`None`）= 何も削除しない
- `retention_days = 90` と書いたときのみ、90日経過したファイルが削除対象になる
- 安全側に倒す設計

## BDD 受け入れシナリオ

```gherkin
Scenario: retention_days 未設定 = 削除なし
  Given `retention_days` が設定されていない
  And 古いファイルが存在する
  When `shiotsuchi prune --expired` を実行する
  Then 何も削除されない
  And 「retention_days が未設定のため何もしませんでした」とメッセージが表示される

Scenario: 保持期間を過ぎたファイルが削除される
  Given `retention_days = 90` が設定されている
  And 91 日前にインデックスされたファイルが存在する
  When `shiotsuchi prune --expired` を実行する
  Then そのファイルのチャンクが DB から削除されている
  And そのファイルの file_cache エントリが削除されている
  And tag_counts が適切にデクリメントされている

Scenario: 保持期間内のファイルは削除されない
  Given `retention_days = 90` が設定されている
  And 60 日前にインデックスされたファイルが存在する
  When `shiotsuchi prune --expired` を実行する
  Then そのファイルのデータは維持されている

Scenario: 全データ削除（purge_all）
  When `shiotsuchi clean --purge-all` を実行する
  Then 全 vault のデータが削除される
  And config は維持される

Scenario: vault ごとに異なる保持期間
  Given vault "work" の保持期間が 180 日
  And vault "personal" の保持期間が 90 日
  And 120 日前のファイルが両 vault に存在する
  When `shiotsuchi prune --expired` を実行する
  Then "work" のファイルは維持されている
  And "personal" のファイルは削除されている
```

## 受け入れ基準
- [ ] `retention_days` 設定項目が `[indexing]` セクションに追加される（デフォルト: `None` = 無制限、削除しない）
- [ ] `retention_days` が未設定の場合、`prune --expired` は何もせず終了する
- [ ] `purge_expired()` メソッドが `NoteDatabase` に追加される
- [ ] `purge_all_user_data()` メソッドが `NoteDatabase` に追加される
- [ ] `shiotsuchi prune` に `--expired` / `--purge-all` フラグが追加される
- [ ] 削除時に tag_counts、chunks、FTS/vec、tasks、file_cache、note_links が一貫して削除される
- [ ] purge_all は config を削除しない

## テスト戦略（TDD）

### ユニットテスト（core/src/db.rs）
- `test_purge_expired_removes_old_files`
- `test_purge_expired_keeps_recent_files`
- `test_purge_all_user_data_clears_all`
- `test_purge_expired_respects_vault_rentention`
- `test_purge_expired_no_retention_set_keeps_all`

### 統合テスト（CLI）
- `test_prune_expired_command`
- `test_clean_purge_all_command`

## 実装アプローチ

### DB メソッド追加
```rust
// core/src/db.rs
impl NoteDatabase {
    /// 保持期間を過ぎたファイルを全 vault から削除する。
    /// retention_days: vault 名 → 日数のマップ。None の vault は無期限。
    pub fn purge_expired(&self, retention_days: &HashMap<String, u32>) -> Result<usize> { ... }

    /// 全 vault の全ユーザーデータを削除する（config は維持）。
    pub fn purge_all_user_data(&self) -> Result<()> { ... }
}
```

### 設定追加
```rust
// core/src/config.rs
pub struct IndexingConfig {
    // ... existing fields ...
    /// データ保持日数。設定しない場合は無期限。
    /// vault ごとに上書き可能: vaults.xxx.retention_days
    pub retention_days: Option<u32>,
}
```

### CLI 拡張
- `shiotsuchi prune` に `--expired` フラグを追加
- `shiotsuchi clean` に `--purge-all` フラグを追加（確認プロンプト付き）

### 使用する既存機構
- `delete_file_fully()` — 既存の atomic 削除メソッドを流用
- `list_cached_paths()` — 全ファイル一覧を取得し保持期間と比較

## 見積もり
5 ポイント（2-3日）

## 技術的考慮事項
- `delete_file_fully()` は tag_counts、chunks、FTS/vec、tasks、file_cache、note_links をトランザクション内で削除する — これを流用する
- WAL mode で動作中に purge が走っても安全であること
- `purge_all_user_data()` は VACUUM を最後に実行してディスク領域を回収するオプションがあると良い

## 実装者向け注記

### 現状コードの確認
```bash
# 既存の delete_file_fully 実装
grep -n "fn delete_file_fully" core/src/db.rs -A 50

# 既存の prune コマンド
grep -rn "prune\|dredge" cli/src/commands/ -A 10

# 既存の IndexingConfig
grep -n "struct IndexingConfig" core/src/config.rs -A 30
```

### 実装手順
1. `core/src/config.rs` の `IndexingConfig` に `retention_days: Option<u32>` を追加
2. `core/src/db.rs` に `purge_expired()` を実装（`list_cached_paths()` → `delete_file_fully()`）
3. `core/src/db.rs` に `purge_all_user_data()` を実装（全 vault の全ファイルを削除）
4. `cli/src/commands/dredge.rs` に `--expired` フラグを追加
5. `cli/src/commands/clean.rs` に `--purge-all` フラグを追加（確認プロンプト付き）
6. テストを追加
7. `make test` で全テストパス確認

### 落とし穴
- `purge_all_user_data()` は DB を削除して再作成するのではなく、全テーブルのデータを DELETE すること。config は維持する
- `delete_file_fully()` は既に vault_name でスコープされているので、vault をまたいだ削除はループで処理する
- 大量のファイル削除後の WAL ファイル肥大化に注意。最後に `wal_checkpoint()` を呼ぶ
