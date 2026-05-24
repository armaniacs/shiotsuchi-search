# PBI: shiotsuchi doctor に対話的修復機能を追加

## ユーザーストーリー
shiotsuchi ユーザーとして、`shiotsuchi doctor` が問題を検出した場面でそのまま修復まで完了してほしい、なぜなら診断結果を見てから別のコマンドを調べて実行する手間を省きたいから

## ビジネス価値
- 診断→修復のワンストップ体験によりユーザーの離脱を防ぐ
- 設定ミスや DB 破損からの復旧時間を短縮する
- 成功指標: `shiotsuchi doctor` 実行後に別コマンドを叩く必要がなくなること

## BDD受け入れシナリオ

```gherkin
Scenario: Configの未知フィールドを検出して修復する
  Given config.toml の [indexing] に未知フィールド `snippet_lines` が存在する
  When ユーザーが `shiotsuchi doctor` を実行する
  Then doctor が `[!!]` で Config パースエラーを報告する
  And "Remove unknown field `snippet_lines` from [indexing]?" とプロンプトを表示する
  When ユーザーが "y" と回答する
  Then config.toml から `snippet_lines` フィールドが削除されている
  And バックアップファイル config.toml.bak.<timestamp> が作成されている
  And doctor が Config に対して `[ok]` を報告する

Scenario: DBが存在しない場合にインデックスを作成する
  Given データベースファイルが存在しない
  And vault ディレクトリに Markdown ファイルが存在する
  When ユーザーが `shiotsuchi doctor` を実行する
  Then doctor が `[..]` で DB not found を報告する
  And "Index your vault now?" とプロンプトを表示する
  When ユーザーが "y" と回答する
  Then データベースが作成されファイルがインデックスされる
  And doctor がファイル数とチャンク数を表示する

Scenario: ユーザーが修復を拒否した場合
  Given 未知フィールドを含む config が存在する
  When ユーザーが `shiotsuchi doctor` を実行する
  And プロンプトに "n" と回答する
  Then config ファイルは変更されない
  And バックアップファイルは作成されない
  And doctor は次のチェックに進む

Scenario: Vault ディレクトリ不在は修復を提案しない
  Given vault に存在しないディレクトリが設定されている
  When ユーザーが `shiotsuchi doctor` を実行する
  Then doctor が `[!!]` で vault を報告する
  And 修復プロンプトは表示されない
  And "Directory does not exist" のヒントが表示される

Scenario: DB破損時に再インデックスする
  Given データベースファイルが破損している（開けない）
  When ユーザーが `shiotsuchi doctor` を実行する
  Then doctor が `[!!]` で DB open failed を報告する
  And "Rebuild database from scratch?" とプロンプトを表示する
  When ユーザーが "y" と回答する
  Then 古いDBがバックアップされる
  And 新しくインデックスが作成される
  And doctor が `[ok]` を報告する
```

## 受け入れ基準
- [ ] 各チェックで問題検出時に `[y/N]` プロンプトが表示される（修復可能な問題のみ）
- [ ] ユーザーが y を選ぶと修復が実行され結果が表示される
- [ ] ユーザーが n を選ぶとスキップされ次のチェックに進む
- [ ] 修復前には必ずバックアップが作成される
- [ ] Vault ディレクトリ不在は修復を提案せずヒントのみ表示する
- [ ] Tokenizer/Embedder 未利用はヒントのみ表示する
- [ ] すべての修復が完了した（または不要だった）場合、"All checks passed." と表示される
- [ ] 修復に失敗した場合でも後続のチェックは継続される

## テスト戦略（t_wadaスタイル）

### E2Eテスト
- `shiotsuchi doctor` を未知フィールドあり config で実行し、プロンプト→修復の流れを手動確認
- `shiotsuchi doctor` を空 DB で実行し、インデックス作成の流れを手動確認

### 統合テスト
- `fix_config_unknown_fields`: 既知の不良 TOML を渡して正しく修復されるか検証
- `fix_db_not_found`: 一時ディレクトリで DB 作成 → stats 確認
- `fix_db_corrupt`: 壊れた DB ファイルに対して clean 相当の動作をするか検証

### 単体テスト
- エラーメッセージからの未知フィールド名抽出
- TOML `[indexing]` テーブルからの特定キー削除とシリアライズ結果の検証
- `ask()` ヘルパーのデフォルト値テスト（`default(false)` であること）

## 実装アプローチ
- **Outside-In**: シナリオ1（Config修復）から着手。E2Eレベルで挙動を確認し、内部ヘルパーを下りながら実装
- **Red-Green-Refactor**: 各ヘルパー関数ごとにテストを先に書き、パスさせてからリファクタリング
- **既存コードの再利用**: `clean.rs` の `backup_file` / `delete_db_files`、`util.rs` の `secure_parent_dir` をそのまま使う

## 見積もり
3〜5 ストーリーポイント（要チームでの見積もり）

## 技術的考慮事項
- 依存関係: 新規依存なし。`dialoguer` 0.12（既存）、`toml` 1.1（既存）
- テスタビリティ: `dialoguer::Confirm` はテスト用のモックがないため、`ask()` をテスト可能な形で分離するか検討（例: `#[cfg(test)]` で入力を注入可能にする）
- 非機能要件: なし

## Definition of Done
- [ ] 全 BDD シナリオが自動テストとして実装されパスする（手動確認のシナリオは除く）
- [ ] テストカバレッジが基準を満たす（config fix の単体テスト + DB 再インデックスの統合テスト）
- [ ] コードレビュー完了
- [ ] リファクタリング完了（グリーン後）
- [ ] `docs/superpowers/specs/2026-05-25-doctor-fix-design.md` の内容と実装が一致していること
