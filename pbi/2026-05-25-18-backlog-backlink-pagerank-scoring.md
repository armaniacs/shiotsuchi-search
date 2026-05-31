# PBI: Backlink / PageRank によるノート重要度スコアリング

## ユーザーストーリー
Obsidian で双方向リンクを活用するユーザーとして、他のノートから多く参照されている「ハブノート」が検索上位に来てほしい、なぜなら重要なノートは多くのノートからリンクされているはずだから

## ビジネス価値
- 知識ベース内の「重要なハブ」を検索で発見しやすくする
- Google の PageRank 的アプローチで検索品質を向上

## BDD 受け入れシナリオ

```gherkin
Scenario: 多くのノートからリンクされているノートが上位に来る
  Given 同一 Vault 内でノート A が 20 件のノートからリンクされている
  And 同一 Vault 内でノート B が 1 件のノートからリンクされている
  When 両ノートが同じキーワードでヒットする
  Then ノート A がノート B より上位に表示される

Scenario: リンク解析なしでも従来通り動作する
  Given config.toml で backlink_scoring = false に設定している
  When 検索を実行する
  Then backlink_count によるスコア補正は適用されず従来のスコア順になる

Scenario: Vault をまたいだリンクはカウントしない
  Given Vault X のノート A が Vault Y のノート B をリンクしている
  When Vault X で検索を実行する
  Then Vault Y のノート B の backlink_count は増加しない

Scenario: インクリメンタルインデックスでもバックリンクが更新される
  Given ノート C が既にインデックス済みで backlink_count = 5 である
  When ノート C にリンクしている新規ノート D を追加してインデックスする
  Then ノート C の backlink_count が 6 に更新される
```

## 受け入れ基準
- [ ] インデックス時に `[[ノート名]]` / `[[ノート名|表示テキスト]]` 形式の Obsidian リンクを解析し `note_links` テーブルに格納する
- [ ] DB マイグレーション v9: `note_links` テーブルを追加し `file_cache` に `backlink_count INTEGER NOT NULL DEFAULT 0` カラムを追加する
- [ ] 検索スコアに backlink_count を重み付けして補正する（FTS/Vec/Hybrid の各モードで補正方向が正しい）
- [ ] `IndexingConfig` に `backlink_scoring: bool`（デフォルト `true`）を追加し config.toml で制御できる
- [ ] Vault をまたいだリンクは別 Vault の backlink_count に影響しない
- [ ] `watcher` によるインクリメンタルインデックス時にも `note_links` と `backlink_count` が更新される

## 見積もり
13 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/indexer.rs`、`core/src/db.rs`、`core/src/search.rs`、`core/src/config.rs`、`core/src/watcher.rs`
- `[[ノート名]]` のリンク解析はファイルのチャンク挿入と同一トランザクションで実行する（インデックス後の別パスではなく）
- 現在の DB バージョンは v8（`emphasized_text` カラム追加）。次は v9

---

## ⚠️ 実装者向け注記

### 実際のスキーマ構成

このコードベースに `notes_meta` テーブルは**存在しない**。実際のテーブルは:
- `chunks` — ノートチャンク（id, file_path, vault_name, content, tokenized_content, tags, title, emphasized_text, ...）
- `file_cache` — ファイルキャッシュ（vault_name, path, hash, mtime, model_id, file_size）
- `fts_chunks` — FTS5 仮想テーブル
- `vec_chunks` — ベクトルテーブル
- `tasks` — チェックボックスタスク

`backlink_count` は **`file_cache`** に追加する（ファイル単位のメタデータであるため）。

### 実装手順

1. **DB マイグレーション v9 を追加する**（`core/src/db.rs` の `migrate()`）：
   ```sql
   CREATE TABLE IF NOT EXISTS note_links (
       source_path TEXT NOT NULL,
       target_path TEXT NOT NULL,   -- 解決済み相対パス
       vault_name  TEXT NOT NULL,
       PRIMARY KEY (source_path, target_path, vault_name)
   );

   ALTER TABLE file_cache ADD COLUMN backlink_count INTEGER NOT NULL DEFAULT 0;
   ```
   `target_name`（ファイル名のみ）ではなく解決済みの `target_path` を格納する。

2. **リンク抽出関数を実装する**（`core/src/indexer.rs`）：
   ```rust
   fn extract_wikilinks(content: &str) -> Vec<String> {
       // [[ノート名]] と [[ノート名|表示テキスト]] を抽出、ノート名のみ返す
       let re = Regex::new(r"\[\[([^\]|]+)(?:\|[^\]]+)?\]\]").unwrap();
       re.captures_iter(content)
           .map(|c| c[1].trim().to_string())
           .collect()
   }
   ```

3. **Wikilink 解決関数を実装する**（同ファイル）：  
   Obsidian のルール: 同名ファイルが複数ある場合は最短パス優先。
   ```rust
   fn resolve_wikilink<'a>(target_name: &str, vault_paths: &[&'a str]) -> Option<&'a str> {
       // 完全一致（パス末尾が "{target_name}.md"）を候補として収集
       // 複数候補があれば最短パスを採用
       vault_paths.iter()
           .filter(|p| p.ends_with(&format!("{}.md", target_name)))
           .min_by_key(|p| p.len())
           .copied()
   }
   ```

4. **インデックス時にリンクを `note_links` へ挿入し、`backlink_count` を集計更新する**  
   `index_file()` のチャンク挿入と同一トランザクション内で実行する。  
   ファイル再インデックス時は `DELETE FROM note_links WHERE source_path=? AND vault_name=?` で古いリンクを削除してから再挿入する。  
   集計更新:
   ```sql
   UPDATE file_cache
   SET backlink_count = (
       SELECT COUNT(*) FROM note_links
       WHERE target_path = file_cache.path AND vault_name = file_cache.vault_name
   )
   WHERE vault_name = ?;
   ```

5. **検索スコアへのバックリンクボーナスを実装する**（`core/src/search.rs` の `search()` 関数）  
   `ChunkSearchResult` を組み立てる際に `file_cache.backlink_count` を JOIN して取得し、モード別に補正する:
   - **FTS モード**: `bm25()` は負値で小さいほど良い → `score -= backlink_count as f64 * 0.05`
   - **Vec/Hybrid モード**: コサイン類似度・RRF スコアは正値で大きいほど良い → `score += backlink_count as f64 * 0.05`
   - `config.backlink_scoring == false` の場合はこの補正をスキップする

6. **`IndexingConfig` に設定フラグを追加する**（`core/src/config.rs`）：
   ```rust
   #[serde(default = "default_backlink_scoring")]
   pub backlink_scoring: bool,

   fn default_backlink_scoring() -> bool { true }
   ```

### 落とし穴

- `build_results` という関数は**存在しない**。スコア補正は `search()` 関数内（`core/src/search.rs:129`）に直接実装する。
- FTS/Vec/Hybrid でスコアの符号・スケールが異なる。補正方向を間違えると FTS 検索が劣化する。
- `vault_name` スコープを必ず付けること。付けないと Vault 間でバックリンクが汚染される。
- Watcher（`core/src/watcher.rs`）経由のインクリメンタルインデックスでも同じ `index_file()` パスを通るため、正しく実装すれば自動的に対応できる。確認すること。
- バックリンク集計の `UPDATE file_cache` は Vault 全体を更新するため、大規模 Vault では時間がかかる可能性がある。進捗ログを入れること。

## Definition of Done
- [ ] バックリンク解析とスコアリングのテストがパスする（FTS/Vec/Hybrid 各モード、Vault スコープ分離、オン/オフ切り替え）
- [ ] インクリメンタルインデックス（watcher）でバックリンクが更新されることをテストで確認する
- [ ] コードレビュー完了
