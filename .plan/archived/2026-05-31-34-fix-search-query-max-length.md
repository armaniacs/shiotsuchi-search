# PBI-34: 検索クエリ入力に最大長バリデーションを追加する

## ユーザーストーリー

shiotsuchi の運用者として、ウェルカムメニューからの検索クエリ入力に最大長制限が欲しい。なぜなら、極端に長いクエリ文字列が FTS5 クエリ構築で過剰なメモリ確保を引き起こす可能性があり、防衛的な品質対策として上限を設けたいからだ。

## ビジネス価値

- リソース枯渇攻撃ベクトルの緩和（防御的セキュリティ）
- 誤操作（誤って長文ペースト）の防止

## 既実装確認

```bash
grep -n "interact_text\|validate\|max_length" cli/src/commands/welcome.rs
# → 296-298, 412-414 行目: dialoguer::Input::interact_text() を validate なしで使用
```

**結果:** welcome.rs 内の2箇所の検索クエリ入力にバリデーションなし。

## BDD受け入れシナリオ

```gherkin
Scenario: 通常長のクエリは受け付けられる
  Given ウェルカムメニューが表示されている
  When  ユーザーが検索クエリに "project plan" と入力する
  Then  クエリが受け付けられ、検索が実行される

Scenario: 最大長を超えるクエリは拒否される
  Given ウェルカムメニューが表示されている
  When  ユーザーが検索クエリに 500 文字の文字列を入力する
  Then  バリデーションエラーが表示される
  And   再入力を促される
```

## 受け入れ基準
- [ ] 200文字以下のクエリは正常に受け付けられる
- [ ] 200文字を超えるクエリはバリデーションエラーとなり再入力される
- [ ] 日本語マルチバイト文字でも正しくカウントされる

## 実装アプローチ

`dialoguer::Input` の `validate_with` メソッドを使用:

```rust
let query: String = dialoguer::Input::with_theme(&dialoguer_theme())
    .with_prompt("検索クエリを入力してください")
    .validate_with(|input: &String| -> Result<(), &str> {
        if input.chars().count() > 200 {
            Err("クエリは200文字以内で入力してください")
        } else {
            Ok(())
        }
    })
    .interact_text()?;
```

2箇所（`run_onboarding` の Step 3 と `run_single_command` の Search）に同じ修正を適用。

## 見積もり

**1ポイント**（2箇所に validate_with 追加）

## 落とし穴
- `.chars().count()` を使うこと（`.len()` はバイト数で計測してしまう）
- `max_length` ではなく `validate_with` を使う理由: `max_length` は dialoguer 0.12 に存在しない場合がある

## Definition of Done
- [ ] 2箇所の検索クエリ入力に200文字バリデーションが追加されている
- [ ] 200文字超の入力を拒否する
- [ ] 全テスト通過
