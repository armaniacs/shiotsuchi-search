# PBI: インデックス統計・ダッシュボード（stats コマンド）

## ユーザーストーリー
大規模 Vault を管理するユーザーとして、ノートの総数・タグ分布・最終更新状況を一覧で見たい、なぜなら自分の知識の偏りや古くなったノートを発見できるから

## ビジネス価値
- 知識ベースの健全性を可視化
- 「どのジャンルのノートが多いか」「最近更新していないノートはどれか」を把握できる

## BDD 受け入れシナリオ

```gherkin
Scenario: インデックス統計が表示される
  When ユーザーが `shiotsuchi stats` を実行する
  Then 総ノート数、タグ TOP10、最終更新日付の分布が表示される

Scenario: JSON 出力に対応する
  When ユーザーが `shiotsuchi stats --json` を実行する
  Then 統計情報が JSON 形式で出力される
```

## 受け入れ基準
- [x] `shiotsuchi stats` サブコマンドを追加する
- [x] 総ノート数・タグ頻度・最終更新日分布を表示する
- [x] `--json` フラグで JSON 出力に対応する

## 見積もり
3 ポイント

## 技術的考慮事項
- 影響ファイル: `cli/src/main.rs`、`core/src/db.rs`（集計クエリ）
- タグ集計は Feat-1（Frontmatter フィルタリング）完了後が前提

---

## ⚠️ 実装者向け注記

### 現状確認（着手前に必ず読むこと）

**`tide` コマンドと `VaultStats` が既に実装されています。**

`cli/src/main.rs` の `Commands::Tide` を確認してください：
```rust
Commands::Tide => {
    let stats = commands::tide::run_tide(&db_path)?;
    commands::tide::print_stats(&stats);
}
```

`core/src/models.rs` の `VaultStats` struct と `core/src/db.rs` の `stats()` メソッドも確認する：
```bash
grep -n "VaultStats\|struct.*Stats\|fn stats" core/src/models.rs core/src/db.rs
cat cli/src/commands/tide.rs
```

### このPBIで実際にやること

1. **`tide` コマンドの現状出力を確認する**  
   `shiotsuchi tide` を実行して何が表示されているか確認する。

2. **不足している統計情報を追加する**  
   `VaultStats` に不足フィールドがあれば追加する。タグ頻度は PBI-04 完了後に追加。

3. **`--json` フラグを追加する**  
   `TideArgs` struct（または新規作成）に `#[arg(long)] json: bool` を追加し、`serde_json::to_string_pretty(&stats)` で出力する。  
   そのために `VaultStats` に `#[derive(Serialize)]` を追加する。

4. **コマンド名を `stats` に変更するか `tide` のエイリアスとするかを決める**  
   現状は `tide` という名前。PBI 要件との整合性を確認する。

### 落とし穴

- `VaultStats` に `Serialize` を追加する場合、`serde` feature が必要か `core/Cargo.toml` を確認する（おそらく既に含まれている）。
- `--json` 出力はパイプや自動化スクリプトでの利用を想定する。数値フィールドは文字列でなく数値型で出力すること。

## Definition of Done
- [x] stats コマンドのテストがパスする
- [x] コードレビュー完了
