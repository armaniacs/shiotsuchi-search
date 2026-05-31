# PBI-36: ウェルカムメニューのユーザー表示文字列を messages.rs に移動する

## ユーザーストーリー

shiotsuchi-search の翻訳者として、将来の i18n 対応に備えて表示文字列が一箇所に集まっていて欲しい。なぜなら、現在は welcome.rs のコード中に30箇所以上のユーザー向け日本語文字列がハードコードされており、翻訳や変更が難しいからだ。

## ビジネス価値

- i18n 準備（全表示文字列を messages.rs に集約）
- コードと表示文字列の分離（単一責任原則）
- 文字列の一括管理・レビューが容易に

## 既実装確認

```bash
grep -n '"' cli/src/commands/welcome.rs | grep -E '"(オンボーディング|設定ファイル|インデックス|検索|Step|🔰|⚡|🎉|⚠️|✅|──|exit|init|setup|search|index|stats|doctor)' | wc -l
# → 30+ 箇所
```

**結果:** 30箇所以上のユーザー向け文字列が welcome.rs にハードコードされている。`messages.rs` に移動する。

## BDD受け入れシナリオ

```gherkin
Scenario: 全表示文字列が messages.rs から参照されている
  Given welcome.rs にユーザー向け文字列が存在する
  When  全箇所を messages.rs の定数に置き換える
  Then  ウェルカムメニューの動作が変わらない
  And  messages.rs に新しい定数が追加されている
```

## 受け入れ基準
- [ ] welcome.rs 内のすべてのユーザー向け日本語文字列が `messages.rs` の定数を参照するようになっている
- [ ] 表示内容が一切変わらない（視覚的リグレッションなし）
- [ ] `cargo test` がグリーン

## 実装アプローチ

1. `messages.rs` に `WELCOME_*` 定数を追加（例: `WELCOME_BANNER_ONBOARDING`, `WELCOME_MENU_CATEGORY_SETUP` など）
2. `welcome.rs` の全ハードコード文字列を対応する定数で置き換え
3. 視覚的リグレッションがないことを手動確認

**注意:** この PBI は純粋なリファクタリング。動作変更は一切含めない。

## 見積もり

**3ポイント**（30+箇所の定数化。機械的だが数が多い）

## リスク
- 置き換え漏れがあるとハードコード文字列が残る → 差分レビューで確認
- 改行を含む文字列（`\` で連結）は `messages.rs` でも同じ形式を維持すること

## Definition of Done
- [ ] welcome.rs にユーザー向け文字列のハードコードが残っていない
- [ ] 全テスト通過
- [ ] ウェルカムメニューの表示が変更前と同一であることを手動確認済み
