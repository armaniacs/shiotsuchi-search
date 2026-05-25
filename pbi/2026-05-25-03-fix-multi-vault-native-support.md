# PBI: マルチ Vault ネイティブ対応（--vault フラグ + config.toml）

## ユーザーストーリー
仕事・プライベート・趣味と複数の Obsidian Vault を使い分けるユーザーとして、環境変数を手動で切り替えずに Vault を指定して検索したい、なぜなら現状は環境変数の手動切り替えが煩雑で日常使いに耐えないから

## ビジネス価値
- 複数 Vault ユーザーの日常操作コストを大幅削減
- 単一 MCP サーバーで複数 Vault を扱えるため Claude Desktop 設定が 1 エントリで完結
- プロダクトの実用ユーザー層を広げる

## BDD 受け入れシナリオ

```gherkin
Scenario: --vault フラグで Vault を指定して検索する
  Given config.toml に work と private の 2 つの vault が登録されている
  When ユーザーが `shiotsuchi dive --vault work "プロジェクト計画"` を実行する
  Then work vault の DB のみを検索した結果が返される

Scenario: デフォルト vault で従来通り動作する
  Given config.toml に default = "work" が設定されている
  When ユーザーが `shiotsuchi dive "検索語"` を実行する（--vault 省略）
  Then work vault を検索した結果が返される

Scenario: 存在しない vault ID を指定するとエラーになる
  Given config.toml に work と private の 2 つの vault が登録されている
  When ユーザーが `shiotsuchi dive --vault hobby "検索語"` を実行する
  Then "vault 'hobby' is not defined in config" 旨のエラーが表示される

Scenario: 旧来の単一 vault 設定でも動作する（後方互換）
  Given config.toml に [[vault]] セクションがなく、従来の notes_dir のみが設定されている
  When ユーザーが `shiotsuchi dive "検索語"` を実行する
  Then 従来通り動作し、マイグレーションを促すメッセージが表示される
```

## 受け入れ基準
- [ ] `config.toml` に `[[vault]]` セクション（id, notes_dir, db_path）を追加できる
- [ ] `default = "<id>"` でデフォルト vault を指定できる
- [ ] `dive`, `chart`, `scan` コマンドに `--vault <id>` フラグが追加される
- [ ] MCP ツールの `search_notes` 等に `vault` パラメータが追加される
- [ ] 旧来の単一 vault 設定でも動作が維持される

## テスト戦略（t_wada スタイル）

### E2E テスト
- `--vault work` で work vault のみを検索することを確認
- `--vault` 省略時にデフォルト vault が使われることを確認

### 統合テスト
- config.toml パース → vault 選択 → DB 接続のフロー
- MCP `vault` パラメータルーティング
- 旧設定フォーマットの後方互換動作

### 単体テスト
- vault id 解決ロジック（デフォルト、指定、存在しない ID）
- config.toml の `[[vault]]` デシリアライズ
- 旧設定検出ロジック

## 実装アプローチ
- **Outside-In**: E2E → 統合 → 単体の順でテストを先に書く
- **Red-Green-Refactor**: 各レイヤーで TDD サイクルを適用

## 見積もり
8 ポイント（要チームでの見積もり）

## 技術的考慮事項
- 影響ファイル: `cli/src/config.rs`、`cli/src/main.rs`、`mcp/src/handler.rs`、`ref/cli.md`、`ref/mcp.md`
- 後方互換: `SHIOTSUCHI_NOTES_DIR` / `SHIOTSUCHI_DB_PATH` 環境変数は当面維持
- 依存関係: なし

---

## ⚠️ 実装者向け注記（ジュニア開発者必読）

### 現状コードの確認

**マルチ Vault は既に実装されています。**

`cli/src/config.rs` を見ると以下が確認できます：
- `ShiotsuchiConfig` に `vaults: HashMap<String, VaultEntry>` フィールドが存在
- `resolved_vaults()` メソッドが新旧両フォーマットを解決する
- `resolved_db_path()` メソッドが存在
- `[vaults.work]` 形式の TOML 設定が既に動作する

`core/src/search.rs` の `search()` 関数にも `vault_filter: Option<&str>` が引数として存在します。

`cli/src/main.rs` の `Commands::Dive` を確認し、`--vault` フラグが既に存在するかどうかを確認してください。

### 実際にやること

```bash
# まず現状を把握する
grep -n "vault\|--vault" cli/src/main.rs | head -30
grep -n "vault\|--vault" cli/src/commands/dive.rs 2>/dev/null || grep -rn "vault" cli/src/commands/
```

1. **`--vault` フラグが未実装なら追加する**  
   `cli/src/commands/dive.rs`（または相当するファイル）の `DiveArgs` struct に：
   ```rust
   #[arg(long)]
   vault: Option<String>,
   ```

2. **`chart` コマンドの `--vault` フラグも同様に確認・追加する**

3. **MCP の `vault` パラメータを確認する**  
   `mcp/src/handler.rs` の `search_notes` ハンドラに vault パラメータが存在するか確認。

4. **存在しない vault ID を指定した場合のエラーメッセージを実装する**

### 落とし穴

- `resolved_vaults()` は存在するが CLI フラグとの接続が未実装の可能性がある。コードの「存在」と「機能する」を混同しないこと。
- `--vault` フラグを追加した場合、`global_notes_dir` など既存のグローバルフラグとの優先順位を決める必要がある。
- MCP の JSON-RPC スキーマに `vault` パラメータを追加した場合、既存の Claude Desktop 設定を壊さないか確認する。

## Definition of Done
- [ ] 全 BDD シナリオが自動テストとして実装されパスする
- [ ] 既存テストがすべてパスする
- [ ] コードレビュー完了
- [ ] リファクタリング完了（グリーン後）
- [ ] `ref/cli.md`・`ref/mcp.md`・README 更新済み
