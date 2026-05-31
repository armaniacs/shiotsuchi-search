# PBI-31: Search→オンボーディング遷移で config_exists がハードコードされている

## ユーザーストーリー

設定ファイルはあるがデータベースが未作成のユーザーがメニューから search を選んだときに、オンボーディングの Step 1（設定ファイル作成）がスキップされて欲しい。なぜなら、設定ファイルは既に存在するのに「設定ファイルを作成しますか？」と聞かれると、製品の動作に混乱するからだ。

## ビジネス価値

- 既存ユーザーの認知負荷を減らす（冗長な確認を表示しない）
- オンボーディングフローの正確性を高める

## 既実装確認

```bash
grep -n "run_onboarding.*false.*false" cli/src/commands/welcome.rs
# → 423行目: run_onboarding(false, false, ...) がハードコード
```

**結果:** 未修正。`run_single_command` の `MenuChoice::Search` 分岐（welcome.rs:423）で `run_onboarding(false, false, ...)` と呼び出しており、config_exists が常に `false`。呼び出し元で `config_path.exists()` を確認してから渡すべき。

## BDD受け入れシナリオ

```gherkin
Scenario: config 存在 + DB 未存在で search を実行すると Step 1 がスキップされる
  Given ~/.config/shiotsuchi/config.toml が存在する
  And   データベースファイルが存在しない
  When  ユーザーがメニューから "search" を選択する
  Then  「データベースが見つかりません」と表示される
  And   「オンボーディングを開始しますか？」と確認される
  When  ユーザーが「はい」を選択する
  Then  Step 1（設定ファイル作成）を表示せず、Step 2（インデックス）から開始される
```

## 受け入れ基準
- [ ] config 存在 + DB 未存在で search → onboarding 遷移時、config_exists=true で呼ばれる
- [ ] 上記シナリオで Step 1 がスキップされる
- [ ] config 未存在 + DB 未存在では Step 1 が表示される（従来動作維持）

## 実装アプローチ

`cli/src/commands/welcome.rs:423` の1行を修正:

```rust
// 修正前 (423行目):
run_onboarding(false, false, cfg, config_path, raw_notes_dir, raw_db_path)?;

// 修正後:
let has_config = config_path.exists();
run_onboarding(has_config, false, cfg, config_path, raw_notes_dir, raw_db_path)?;
```

TDDサイクル:
1. RED: テストを書く → 修正前の動作をキャプチャ（config 存在時に Step 1 が表示される）
2. GREEN: 上記1行を修正
3. REFACTOR: 不要

## 見積もり

**1ポイント**（1行の修正＋テスト）

## 技術的考慮事項

- 依存関係: なし（welcome.rs 内のみの修正）
- テスタビリティ: `config_path.exists()` のモックは困難なため、手動確認または `tempfile` を使ったテスト

## Definition of Done
- [ ] 上記1行の修正が反映されている
- [ ] `cargo test -p shiotsuchi` がグリーン
- [ ] 手動で config 有無両方のシナリオを確認済み
