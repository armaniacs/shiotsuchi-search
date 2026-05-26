# PBI: .gitignore 風インデックス除外ルール

## ユーザーストーリー
特定のフォルダや一時ファイルをインデックスから除外したいユーザーとして、ワイルドカードや正規表現で柔軟に除外設定をしたい、なぜなら `node_modules`・テンプレート・プライベートフォルダを AI に見せたくないから

## ビジネス価値
- 不要ファイルのインデックス除外でパフォーマンス向上
- プライバシー保護（AI に見せたくないフォルダを確実に除外）

## BDD 受け入れシナリオ

```gherkin
Scenario: 設定した除外パターンのファイルがインデックスされない
  Given config.toml に exclude = ["private/**", "*.tmp"] を設定している
  When `shiotsuchi chart` を実行する
  Then private/ フォルダ内のファイルと .tmp ファイルはインデックスされない

Scenario: 除外設定なしでは全 .md ファイルをインデックスする
  Given exclude 設定がない
  When `shiotsuchi chart` を実行する
  Then 全 .md ファイルがインデックスされる
```

## 受け入れ基準
- [x] config.toml に `exclude_dirs` 配列でパターンを設定できる（既存）
- [x] glob パターン（`*`・`**`・`?`）をサポートする
- [x] `.shiotsuchiignore` ファイルで除外ルールを設定できる（vault ルート直下のみ）
- [x] `shiotsuchi check-ignore <path>` 診断コマンドで除外理由を確認できる
- [x] 除外されたファイル数が chart のサマリに表示される
- [x] `--verbose` 時に除外理由がログ出力される

## 見積もり
3 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/indexer.rs`、`cli/src/config.rs`
- `glob` または `ignore` クレートで除外判定

---

## ⚠️ 実装者向け注記

### 現状確認（着手前に必ず読むこと）

**この機能は既に実装されています。**

`core/src/indexer.rs` の `build_exclude_globset` 関数（30行目付近）を見ると、  
`globset` クレートを使った除外パターンが既に動作しています。

`cli/src/config.rs` の `IndexingConfig` に `exclude_dirs: Vec<String>` が存在し、  
`config.toml` の `[indexing] exclude_dirs = [...]` で設定できます。

### このPBIで実際にやること

```bash
# 現状の exclude 実装を確認する
grep -n "exclude_dirs\|exclude_patterns\|build_exclude" core/src/indexer.rs | head -20
grep -n "exclude_dirs\|exclude" core/src/config.rs | head -10
```

1. **現状実装でワイルドカードが動くか確認する**  
   `["*.tmp", "private/**"]` などのパターンでテストを書いて動作確認する。

2. **`.shiotsuchiignore` ファイルのサポート（オプション）**  
   `.gitignore` と同様にVault ルートの `.shiotsuchiignore` を読み込む機能を追加する：
   ```rust
   fn load_shiotsuchiignore(vault_dir: &Path) -> Vec<String> {
       let ignore_file = vault_dir.join(".shiotsuchiignore");
       // ...
   }
   ```

3. **ドキュメント更新**  
   `ref/cli.md` と README に `exclude_dirs` の使用例を追記する。

### 落とし穴

- `build_exclude_globset` は `**/パターン/**` でラップしているため、ファイルへの直接パターン（`*.tmp`）が効かない可能性がある。テストで動作確認すること。
- `exclude_dirs` という名前はディレクトリのみ除外するように見えるが、ファイルパターンも除外できるか確認が必要。

## Definition of Done
- [ ] 除外ルールのテストがパスする
- [ ] コードレビュー完了
