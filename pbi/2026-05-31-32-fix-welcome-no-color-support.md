# PBI-32: ウェルカムメニューの NO_COLOR 対応

## ユーザーストーリー

モノクロ端末や `TERM=dumb` 環境で shiotsuchi を使うユーザーとして、ウェルカムメニューが ANSI エスケープシーケンスまみれで表示されず、読める状態で操作したい。なぜなら、既存の `dive.rs` は `NO_COLOR` 環境変数を尊重しているのに、ウェルカムメニューだけが無視しているからだ。

## ビジネス価値

- `NO_COLOR` 業界標準への準拠（https://no-color.org/）
- モノクロ端末・SSH from mobile・CI ログでの視認性確保
- 既存コード（dive.rs）との一貫性

## 既実装確認

```bash
# welcome.rs の ColorfulTheme 使用箇所
grep -c "ColorfulTheme" cli/src/commands/welcome.rs  # → 13
# dive.rs の NO_COLOR 対応
grep -n "NO_COLOR" cli/src/commands/dive.rs
# → 203行目: if query.is_empty() || std::env::var("NO_COLOR").is_ok() {
```

**結果:** welcome.rs は全13箇所で `ColorfulTheme::default()` を直指定。`NO_COLOR` のチェックは一切なし。dive.rs は `highlight_term` 関数で `NO_COLOR` をチェック済み。

## BDD受け入れシナリオ

```gherkin
Scenario: NO_COLOR 環境変数が設定されていると BasicTheme が使われる
  Given NO_COLOR=1 が設定されている
  When  ウェルカムメニューが表示される
  Then  Select / Confirm / Input の全 dialoguer コンポーネントが BasicTheme でレンダリングされる
  And   ANSI エスケープシーケンスが出力されない

Scenario: NO_COLOR 未設定時は従来通り ColorfulTheme
  Given NO_COLOR が設定されていない
  When  ウェルカムメニューが表示される
  Then  Select / Confirm / Input が ColorfulTheme でレンダリングされる（従来動作維持）
```

## 受け入れ基準
- [ ] `NO_COLOR=1` 環境でウェルカムメニューがカラーコードなしで表示される
- [ ] `NO_COLOR` 未設定時は従来通りカラー表示
- [ ] 全5種の dialoguer コンポーネント（Select, Confirm, Input）が対応

## 実装アプローチ

テーマ選択ヘルパー関数を追加し、全13箇所を置き換える:

```rust
/// dialoguer theme: Colorful in normal terminals, Basic when NO_COLOR is set.
fn dialoguer_theme() -> impl dialoguer::theme::Theme {
    if std::env::var("NO_COLOR").is_ok() {
        dialoguer::theme::BasicTheme
    } else {
        dialoguer::theme::ColorfulTheme::default()
    }
}
```

全 dialoguer 呼び出しを `ColorfulTheme::default()` → `dialoguer_theme()` に変更（13箇所）。

TDD:
1. RED: `NO_COLOR=1 cargo test` で色出力が抑制されることを確認するテスト
2. GREEN: ヘルパー関数追加 + 全13箇所置き換え
3. REFACTOR: 重複除去

## 見積もり

**2ポイント**（ヘルパー関数 + 13箇所の機械的置き換え + テスト）

## 技術的考慮事項

- **依存関係**: `dialoguer::theme::{BasicTheme, ColorfulTheme, Theme}` — すべて既存
- **テスタビリティ**: `std::env::set_var("NO_COLOR", "1")` で環境変数をセットしてテスト可能
- **注意**: dialoguer 0.12 の `BasicTheme` は存在するか確認する。`dialoguer::theme::SimpleTheme` の可能性あり

```bash
# BasicTheme の存在確認
grep -r "BasicTheme\|SimpleTheme" $(find . -path "*/dialoguer*" -name "*.rs" 2>/dev/null | head -5) 2>/dev/null
```

もし `BasicTheme` が存在しない場合は `SimpleTheme` または `ColorfulTheme { ..default() }` + NO_COLOR 条件分岐で対応。

## Definition of Done
- [ ] `dialoguer_theme()` ヘルパー関数が追加されている
- [ ] 全13箇所の dialoguer 呼び出しが `dialoguer_theme()` 経由になっている
- [ ] `NO_COLOR=1 cargo test` がグリーン
- [ ] 手動で NO_COLOR 有無両方の表示を確認済み
