# PBI: Frontmatter タグ・日付によるフィルタリングと重み付け

## ユーザーストーリー
Obsidian で YAML Frontmatter を活用しているユーザーとして、タグや日付を条件にしてノートを絞り込み検索したい、なぜなら全文検索だけでは属性による絞り込みができず関係ないノートが大量にヒットするから

## ビジネス価値
- タグ・日付による精密な絞り込みで検索ノイズを削減
- タイトルやタグマッチ時のスコアブーストで関連性の高いノートが上位に来る
- Obsidian のメタデータ運用と連携した実用的な検索体験を提供

## BDD 受け入れシナリオ

```gherkin
Scenario: タグで絞り込んで検索する
  Given Vaultに project タグを持つノートが 20 件、持たないノートが 100 件存在する
  When ユーザーが `shiotsuchi dive --tag project "計画"` を実行する
  Then project タグを持つノートの中からのみ検索結果が返される

Scenario: 日付で絞り込んで検索する
  Given 2026-01-01 以降に作成されたノートが 30 件存在する
  When ユーザーが `shiotsuchi dive --since 2026-01-01 "振り返り"` を実行する
  Then 2026-01-01 以降の frontmatter date を持つノートのみが対象になる

Scenario: タイトルマッチ時にスコアがブーストされる
  Given "プロジェクト計画" というタイトルのノートが存在する
  When ユーザーが `shiotsuchi dive "プロジェクト計画"` を実行する
  Then タイトルにマッチしたノートが本文マッチのみのノートより上位に表示される

Scenario: Frontmatter を持たないノートも検索対象になる
  Given Frontmatter なしのノートが存在する
  When ユーザーが `shiotsuchi dive "検索語"` を実行する（フィルタなし）
  Then Frontmatter なしのノートも通常通り検索結果に含まれる
```

## 受け入れ基準
- [ ] `notes_meta` に `tags TEXT` と `frontmatter_date TEXT` カラムが追加される
- [ ] インデックス時に YAML Frontmatter をパースして `tags`・`frontmatter_date` を格納する
- [ ] `dive` コマンドに `--tag <tag>` フラグが追加される
- [ ] `dive` コマンドに `--since <date>` フラグ（ISO 8601 形式）が追加される
- [ ] タイトル・タグマッチ時に FTS5 スコアブーストが適用される
- [ ] Frontmatter なしノートはフィルタなし検索では引き続き対象になる

## テスト戦略（t_wada スタイル）

### E2E テスト
- `--tag` フィルタが正しく絞り込むことを確認
- `--since` フィルタが日付で絞り込むことを確認

### 統合テスト
- Frontmatter パース → DB 格納 → フィルタ SQL のフロー
- タグ・日付カラムのマイグレーション動作
- スコアブースト適用の検証

### 単体テスト
- YAML Frontmatter パースロジック（tags 配列、date 文字列）
- `--tag`・`--since` フラグの SQL WHERE 条件生成
- Frontmatter なしノートの NULL 安全ハンドリング
- スコアブースト計算ロジック

## 実装アプローチ
- **Outside-In**: E2E → 統合 → 単体の順でテストを先に書く
- **Red-Green-Refactor**: 各レイヤーで TDD サイクルを適用

## 見積もり
5 ポイント（要チームでの見積もり）

## 技術的考慮事項
- 影響ファイル: `core/src/db.rs`、`core/src/indexer.rs`、`core/src/search.rs`、`cli/src/main.rs`
- YAML パースは `serde_yaml` または `gray_matter` クレートで対応
- `tags` は JSON 配列またはカンマ区切り文字列で格納（検索しやすい形式を選定）
- 依存関係: Fix-1（mtime スキャン）完了後が望ましいが独立して進めることも可

---

## ⚠️ 実装者向け注記（ジュニア開発者必読）

### 現状コードの確認

**Frontmatter のパースは一部実装済みです。**

`core/src/indexer.rs` の `test_no_frontmatter`（361行目付近）や  
`test_index_directory_with_progress_collects_tags`（911行目付近）というテスト名が存在します。
これは tags 収集機能が既にある可能性を示します。

```bash
# 現状を確認する
grep -n "frontmatter\|tags\|yaml\|collect_tags" core/src/indexer.rs | head -30
grep -n "tags\|frontmatter" core/src/db.rs | head -20
grep -n "VaultStats" core/src/models.rs
```

`VaultStats` struct（`core/src/models.rs` 70行目付近）に `tag_counts` フィールドがある可能性があります。

### 実装手順（未実装の場合）

1. **`core/src/db.rs` の `notes_meta`（または `file_cache`）テーブルを確認する**  
   tags カラムが存在しなければ追加する：
   ```sql
   ALTER TABLE chunks ADD COLUMN tags TEXT;
   ALTER TABLE chunks ADD COLUMN frontmatter_date TEXT;
   ```
   ただし FTS5 テーブルへの ALTER は制限あり。`chunks_meta` のような別テーブルが必要な場合もある。

2. **Frontmatter パーサーを実装または確認する**  
   既存の実装があれば再利用する。ない場合：
   ```rust
   fn parse_frontmatter(content: &str) -> Option<(HashMap<String, serde_json::Value>, &str)> {
       // "---\n" で始まる場合のみ処理
   }
   ```
   `serde_yaml` は依存に追加が必要。まず `gray_matter` クレートも検討する。

3. **`DiveArgs` に `--tag` と `--since` フラグを追加する**  
   ```rust
   #[arg(long)]
   tag: Vec<String>,  // 複数タグ指定可能に
   
   #[arg(long)]
   since: Option<String>,  // ISO 8601: "2026-01-01"
   ```

4. **`core/src/search.rs` の `search_fts` に WHERE 条件を追加する**

### 落とし穴

- FTS5 仮想テーブルには `ALTER TABLE ADD COLUMN` が使えない。tags はメタテーブルに格納する。
- `---` で始まらないファイルにも安全にフォールバックすること（大半のノートは Frontmatter なし）。
- `tags` フィールドは YAML で配列（`[a, b]`）の場合と文字列（`a, b`）の場合がある。両方に対応すること。
- `--since` の日付フォーマットが多様（`2026-01-01`、`2026/01/01` 等）。パース失敗時のエラーを丁寧に出すこと。

## Definition of Done
- [ ] 全 BDD シナリオが自動テストとして実装されパスする
- [ ] 既存テストがすべてパスする
- [ ] コードレビュー完了
- [ ] リファクタリング完了（グリーン後）
- [ ] `ref/cli.md` にフラグ説明追記済み
