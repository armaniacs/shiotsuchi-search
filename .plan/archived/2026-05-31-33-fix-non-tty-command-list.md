# PBI-33: 非TTY時のメッセージに利用可能コマンド一覧を表示する

## ユーザーストーリー

CI やパイプ経由で `shiotsuchi` を使うユーザーとして、非TTY環境でコマンドを間違えたときに `--help` を実行しなくても何のコマンドがあるか知りたい。なぜなら、現在は「--help を参照」としか表示されず、ワンステップ多いからだ。

## ビジネス価値

- 非TTY環境（CI、パイプ、スクリーンリーダー）でのユーザー体験向上
- Accessibility Advocate の指摘対応

## 既実装確認

```bash
grep -n "WELCOME_NON_TTY_HINT\|WELCOME_NON_TTY_NO_CONFIG" cli/src/messages.rs
# → WELCOME_NON_TTY_HINT: サブコマンドを指定するか --help を参照
# → WELCOME_NON_TTY_NO_CONFIG: 設定ファイルが見つかりません
```

**結果:** 非TTYメッセージは `--help` への誘導のみ。コマンド一覧はなし。

## BDD受け入れシナリオ

```gherkin
Scenario: 非TTY + config 未存在でコマンド一覧が表示される
  Given パイプ経由で `shiotsuchi` が実行される
  And   config が存在しない
  When  サブコマンドが指定されていない
  Then  設定ファイルが見つからない旨のメッセージが表示される
  And   利用可能なコマンド一覧（init, index, search, ...）が表示される
  And   終了コード 0

Scenario: 非TTY + config 存在でコマンド一覧が表示される
  Given パイプ経由で `shiotsuchi` が実行される
  And   config が存在する
  When  サブコマンドが指定されていない
  Then  利用可能なコマンド一覧が表示される
  And   終了コード 0
```

## 受け入れ基準
- [ ] 非TTYのガイダンスメッセージに主要コマンド一覧（6〜8コマンド）が含まれる
- [ ] config 有無両方のケースで表示される

## 実装アプローチ

`cli/src/messages.rs` に定数を追加し、`cli/src/commands/welcome.rs` の非TTY分岐で出力する:

```rust
// messages.rs
pub const WELCOME_NON_TTY_COMMAND_LIST: &str = "\
利用可能なコマンド:
  init    設定ファイルを作成・編集する
  index   ノートをインデックスする
  search  ノートを検索する
  watch   ファイル変更を監視する
  stats   統計情報を表示する
  doctor  環境の状態を診断する
詳細は `shiotsuchi --help` を参照してください。";

// welcome.rs - run_welcome 非TTY分岐
if !config_path.exists() {
    eprintln!("{}", messages::WELCOME_NON_TTY_NO_CONFIG);
    println!("{}", messages::WELCOME_NON_TTY_COMMAND_LIST);
} else {
    println!("{}", messages::WELCOME_NON_TTY_COMMAND_LIST);
}
```

## 見積もり

**1ポイント**（定数追加 + 分岐修正）

## Definition of Done
- [ ] 非TTYメッセージにコマンド一覧が含まれている
- [ ] `echo "" | cargo run -p shiotsuchi` で確認
- [ ] 全テスト通過
