# PBI-29: CLIコマンドに直感的な標準名エイリアスを追加する

## ユーザーストーリー

shiotsuchi-search の新規ユーザーとして、`index`・`search`・`watch` といった標準的なコマンド名を使いたい。なぜなら、海のメタファー（`chart`・`dive`・`scan`）は世界観として面白いが、初見で何のコマンドか分からず学習コストが高いからだ。

## ビジネス価値

- 初見ユーザーの「コマンドが分からない」離脱を防ぐ
- ドキュメントなしで操作できるユーザー数が増える
- 世界観（海メタファー）は既存ユーザー向けに残るため後退なし

## 既実装確認

`dive` には既に `search` エイリアスが存在する（`cli/src/main.rs:97`）。
残りの5コマンド + `config-migrate` のサブコマンド化が未対応。

```bash
grep -n "alias\|visible_alias" cli/src/main.rs
```

## BDD受け入れシナリオ

```gherkin
Scenario: 新規ユーザーが index コマンドでインデックスを構築する
  Given shiotsuchi-search がインストールされている
  When ユーザーが `shiotsuchi index --notes-dir ~/Notes` を実行する
  Then `shiotsuchi chart --notes-dir ~/Notes` と同じ結果になる
  And ヘルプ画面に `index` が主コマンドとして表示される

Scenario: ユーザーが旧コマンド名（chart）を使っても動く
  Given 旧来のユーザーが `shiotsuchi chart --notes-dir ~/Notes` を実行する
  When コマンドが処理される
  Then エラーなく動作する（後方互換性維持）

Scenario: watch コマンドでファイル監視を開始する
  Given ノートディレクトリが存在する
  When ユーザーが `shiotsuchi watch` を実行する
  Then `shiotsuchi scan` と同じファイル監視が開始される

Scenario: 存在しないコマンド名を打った場合
  Given ユーザーが `shiotsuchi index-files` と誤入力する
  When コマンドが処理される
  Then エラーメッセージと `--help` への誘導が表示される
```

## 受け入れ基準

- [ ] `index` が `chart` のエイリアスとして動作する
- [ ] `prune` が `dredge` のエイリアスとして動作する
- [ ] `stats` が `tide` のエイリアスとして動作する
- [ ] `watch` が `scan` のエイリアスとして動作する
- [ ] `list` が `log` のエイリアスとして動作する
- [ ] 旧コマンド名（`chart`, `dredge`, `tide`, `scan`, `log`）が引き続き動作する（後方互換）
- [ ] `shiotsuchi --help` に新しい標準名が主表示される
- [ ] シェル補完スクリプト生成（`completion` サブコマンド）が新コマンド名にも対応する

## テスト戦略（t_wadaスタイル）

### E2Eテスト（最小限）
- `shiotsuchi index --help` が正常終了する
- `shiotsuchi search "query"` が結果を返す（既存の `dive` のエイリアステストを流用）

### 統合テスト
- 各エイリアスが本コマンドと同一の `Commands` enum バリアントにディスパッチされること
- `clap` の `try_parse_from` で全エイリアスをパースできること

### 単体テスト（`cli/src/main.rs` の既存テスト群に追加）

```rust
// 既存パターン（line 319〜）に倣う
#[test]
fn test_alias_index_for_chart() {
    let cli = parse_cli(&["shiotsuchi", "index"]);
    assert!(matches!(cli.command, Commands::Chart(_)));
}

#[test]
fn test_alias_prune_for_dredge() {
    let cli = parse_cli(&["shiotsuchi", "prune"]);
    assert!(matches!(cli.command, Commands::Dredge(_)));
}

#[test]
fn test_alias_stats_for_tide() {
    let cli = parse_cli(&["shiotsuchi", "stats"]);
    assert!(matches!(cli.command, Commands::Tide(_)));
}

#[test]
fn test_alias_watch_for_scan() {
    let cli = parse_cli(&["shiotsuchi", "watch"]);
    assert!(matches!(cli.command, Commands::Scan(_)));
}

#[test]
fn test_alias_list_for_log() {
    let cli = parse_cli(&["shiotsuchi", "list"]);
    assert!(matches!(cli.command, Commands::Log));
}

#[test]
fn test_backward_compat_chart_still_works() {
    let cli = parse_cli(&["shiotsuchi", "chart"]);
    assert!(matches!(cli.command, Commands::Chart(_)));
}
```

## 実装アプローチ

**Outside-In**: エイリアステスト（失敗）を先に書き、`#[command(alias = ...)]` を追加してグリーンにする。

**Red-Green-Refactor**: `dive` の既存エイリアス実装（line 97）がテンプレート。同じパターンを5コマンドに適用するだけ。

### 具体的な変更箇所

`cli/src/main.rs` の `Commands` enum:

```rust
// Before
#[command(about = crate::messages::CHART_ABOUT)]
Chart(commands::chart::ChartArgs),

// After
#[command(alias = "index", about = crate::messages::CHART_ABOUT)]
Chart(commands::chart::ChartArgs),
```

変更対象:

| 行 (概算) | コマンド | 追加するエイリアス |
|---|---|---|
| 80 | `Chart` | `alias = "index"` |
| 101 | `Dredge` | `alias = "prune"` |
| 105 | `Log` | `alias = "list"` |
| 107 | `Scan` | `alias = "watch"` |
| 117 | `Tide` | `alias = "stats"` |

`dive` の `search` エイリアス（line 97）は**既存のまま変更不要**。

### config migrate サブコマンド化について

`ConfigMigrate` を `Config` のサブコマンドにする変更は影響範囲が広いため、このPBIには含めない。別PBIとして切り出すことを推奨（`config-migrate` は現状でも機能するため優先度低）。

## 落とし穴

- `visible_alias` と `alias` の違い: `alias` は `--help` に表示されない。**標準名を主コマンドとして表示したい**ならば、Enum バリアント名を変更して旧名をエイリアスにする逆転が必要。ただし Enum 名変更は `match` の全ブランチ変更を伴うため工数増。→ まず `alias` で動作確認し、`visible_alias` での表示有無をチームで合意してから決定する。
- シェル補完: `clap_complete` はエイリアスを補完候補に含めるため、追加作業不要。

## 見積もり

**2ポイント**（テスト追加込み。Enum バリアント名変更をしないなら1日以内）

## 技術的考慮事項

- 依存関係: なし（`clap` の機能のみ）
- 後方互換: 旧コマンド名は全てエイリアスとして残るため破壊的変更なし
- ドキュメント: `ref/cli.md` に新コマンド名を主として追記する

## Definition of Done

- [ ] 全BDDシナリオが自動テストとして実装されパスする
- [ ] `cargo test -p shiotsuchi-cli` がグリーン
- [ ] `shiotsuchi --help` で新コマンド名が確認できる
- [ ] `ref/cli.md` のコマンド一覧を新名称で更新済み
- [ ] コードレビュー完了
