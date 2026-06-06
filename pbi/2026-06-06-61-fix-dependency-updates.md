# PBI-61: 依存クレートの定期アップデート (DEV-66)

## ユーザーストーリー

開発者として、依存クレートが最新の安定バージョンに更新されていてほしい、なぜなら古いバージョンには未修正のバグや互換性リスクが残っている可能性があるから

## ビジネス価値

- 3つの直接依存を最新互換バージョンに更新する
- `cargo outdated` の出力をクリーンに保つ
- 依存関係の健全性を維持する

## 更新対象

| クレート | 現行 | 最新 | 種別 | 影響 |
|---------|------|------|------|------|
| `rusqlite` | 0.39.0 | 0.40.1 | minor | DB アクセス層 |
| `console` | 0.15.11 | 0.16.3 | minor | プログレス表示 |
| `indicatif` | 0.17.11 | 0.18.4 | minor | プログレスバー |

`criterion` (dev-dep, 0.5.1 → 0.8.2) は開発依存のため更新推奨だが必須ではない。

## BDD 受け入れシナリオ

```gherkin
Scenario: rusqlite を 0.40.1 に更新する
  Given Cargo.toml に rusqlite = "0.39.0" と指定されている
  When  バージョンを最新に更新する
  Then  cargo build が成功する
  And   全テストがグリーン

Scenario: console + indicatif を最新に更新する
  Given Cargo.toml に console = "0.15.11" と indicatif = "0.17.11" と指定されている
  When  バージョンを最新に更新する
  Then  cargo build が成功する
  And   全テストがグリーン
```

## 受け入れ基準

- [ ] `rusqlite` が 0.40.1 に更新されている
- [ ] `console` が 0.16.3 に更新されている
- [ ] `indicatif` が 0.18.4 に更新されている
- [ ] `cargo build`（ワークスペース全体）がエラーなし
- [ ] `cargo test`（ワークスペース全体）が全テストグリーン
- [ ] `cargo outdated --exit-code 1` が（既知の非互換 transitive 依存を除いて）成功する

## テスト戦略（t_wada スタイル）

### 統合テスト
- 全テストがグリーンであることが唯一の検証手段。各クレートの breaking changes は既存テストで検出される。

### 注意点
- `rusqlite` 0.39 → 0.40 の CHANGELOG を確認し、破壊的変更がないか事前に確認する
- `console` / `indicatif` は主にプログレス表示に使用されており、表示の微細な変更は許容する

## 実装アプローチ

- **更新手順**: `cargo upgrade` または手動で Cargo.toml のバージョン番号を書き換える

### 更新コマンド

```bash
cargo upgrade -p shiotsuchi-core -- rusqlite@0.40.1
cargo upgrade -p shiotsuchi-core -- console@0.16.3 indicatif@0.18.4
# または手動で Cargo.toml を編集
```

### 検証コマンド

```bash
cargo build
cargo test
cargo outdated
```

## 見積もり

0.5〜1時間（手動更新 + コンパイル + テスト。破壊的変更があれば追加調査が必要）

## 技術的考慮事項

- **`rusqlite` 0.39 → 0.40**: 互換性のある minor bump であることを確認。0.40 は SQLite 3.47 以降を同梱。`bundled` feature 使用のためシステム SQLite には影響しない
- **`console` 0.15 → 0.16**: 互換性のある minor bump。`console::Style` 等の使用箇所に破壊的変更がないことを確認
- **`indicatif` 0.17 → 0.18**: 互換性のある minor bump。`ProgressBar` / `ProgressStyle` の使用箇所に影響がないことを確認
- `criterion` の更新は dev-dep のため緊急性は低い。余裕があれば行う

## 実装者向け注記

### 現状コードの確認

```bash
grep -n "rusqlite\|console\|indicatif" core/Cargo.toml
cargo outdated -p shiotsuchi-core -p shiotsuchi -p shiotsuchi-mcp
```

### 実装手順

1. `core/Cargo.toml` のバージョン番号を更新
2. `cargo build` でコンパイル確認
3. コンパイルエラーが出た場合は各クレートの CHANGELOG で破壊的変更を確認して修正
4. `cargo test` で全テストグリーン確認
5. `cargo outdated` で更新が反映されたことを確認

### 落とし穴

- `rusqlite` の minor bump でバインディングに破壊的変更がある場合、`bundled` SQLite のバージョン差異が原因の可能性がある
- transitive 依存の更新は `cargo update` でまとめて行う

## Definition of Done

- [ ] `cargo build`（ワークスペース全体）がエラーなし
- [ ] `cargo test`（ワークスペース全体）が全テストグリーン
- [ ] `cargo outdated` で指定した3クレートが最新と表示される
