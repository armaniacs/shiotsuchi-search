# PBI-35: オンボーディング完了画面のボックス幅を動的計算に修正する

## ユーザーストーリー

オンボーディングが完了したユーザーとして、完了画面がきれいに表示されて欲しい。なぜなら、現在の box-drawing は罫線の幅と文字列の幅が合っておらず、ターミナルでボックスが崩れて見えるからだ。

## ビジネス価値

- 視覚的な品質向上
- バージョン変更でメッセージが変わっても崩れない保守性

## 既実装確認

```bash
grep -n "オンボーディング完了\|══════" cli/src/commands/welcome.rs
# → 328-339行目: 完了画面の box-drawing（罫線46文字に対しコンテンツ33文字）
```

**結果:** 完了画面の box-drawing 幅がハードコードされており不整合がある。

## BDD受け入れシナリオ

```gherkin
Scenario: オンボーディング完了画面のボックスが正しく表示される
  Given オンボーディングが完了する
  Then  box-drawing の上罫線と下罫線の幅が一致する
  And  すべてのコンテンツ行が罫線内に収まっている
```

## 受け入れ基準
- [ ] box-drawing の幅が動的に計算され、上罫線・下罫線・コンテンツ行の幅が一致する
- [ ] メッセージを変更しても幅が自動調整される

## 実装アプローチ

`run_onboarding` の完了画面（welcome.rs:328-339）を修正。`show_banner()` と同じ動的パディング計算パターンを使用:

```rust
fn print_completion_box() {
    let lines = [
        "         🎉 オンボーディング完了！            ",
        "                                              ",
        "  これで shiotsuchi-search を使い始める準備が   ",
        "  整いました。                                ",
        "                                              ",
        "  メニューからさらに操作を選べます:            ",
        "    search  ノートを検索する                   ",
        "    index   再インデックスする                  ",
        "    stats   統計情報を表示する                 ",
        "    ...                                       ",
    ];
    let max_width = lines.iter().map(|l| l.len()).max().unwrap_or(50);
    let inner_w = max_width + 2;
    println!("╔{}╗", "═".repeat(inner_w));
    for line in &lines {
        let pad = inner_w.saturating_sub(line.len());
        println!("║{}{}║", line, " ".repeat(pad));
    }
    println!("╚{}╝", "═".repeat(inner_w));
}
```

## 見積もり

**1ポイント**

## Definition of Done
- [ ] 完了画面の box-drawing 幅が動的計算になっている
- [ ] `show_banner()` と同じパターンを使っている
- [ ] 全テスト通過
