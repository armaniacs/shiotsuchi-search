# Checking Team レビューレポート（2回目）

> 実施日: 2026-06-01 21:46
> ブランチ: `improve-2026-05-25`
> 比較対象: `origin/main`
> レビュアー: 22名中22名完了

## 総合評価: 85/100 (ランク: A)

前回レビュー（84/100）から +1pt 改善。前回の High 指摘は全て修正済み。新規 High は 2件。

**スコア分布:**
| レンジ | エージェント数 |
|--------|--------------|
| 100 | 2（FinOps, Test Experts） |
| 85-99 | 13 |
| 80-84 | 1（SRE/Ops） |
| 75-79 | 3（Legacy Bridge, Refactoring, Data Integrity 70） |
| 65-74 | 1（Supply Chain 65） |

---

## 重要指摘事項（優先度順）

### [High] Migration v10 backfill がバイト数而非文字数

- **指摘者**: Data Integrity Expert, SRE/Ops Specialist（2名一致）
- **場所**: `core/src/db.rs:342-347`
- **影響**: backfill SQL の `LENGTH(content)` は UTF-8 バイト数を返す。日本語テキストでは約3倍に膨らむ。`reindex_file` は `chars().count()` で正しい Unicode 文字数を使用。アップグレード済み DB の `char_count` が不正確に成為。
- **対処**: backfill SQL を削除（Test Experts が実施済みの可能性あり）。次回の reindex で正しく上書きされるため、backfill は不要。

### [High] edgequake-pdf2md 推移的依存の攻撃面拡大

- **指摘者**: Supply Chain Sentinel
- **場所**: `core/Cargo.toml:44` (vlm feature)
- **影響**: `vlm` feature 有効時に AWS SDK, OpenAI クライアント, OpenTelemetry 等が導入。攻撃面拡大。
- **対処**: `vlm` は `default` から除外済み（✓）。VLM を有効化する際のセキュリティレビュー手順を CONTRIBUTING.md に明文化する。

### [Medium] `delete_file_fully` が incoming note_links を削除しない

- **指摘者**: System Architect
- **場所**: `core/src/db.rs:534`
- **影響**: `source_path` 方向のみ削除。`target_path` 方向（他ファイル→削除対象ファイル）のリンクが残り、note_links にデッドロウ蓄積。
- **対処**: `DELETE FROM note_links WHERE target_path = ?1 AND vault_name = ?2` を追加。

### [Medium] `reindex_file` の tag_counts ゼロカウント行クリーンアップ未実施

- **指摘者**: Data Integrity, SRE/Ops, Test Experts（3名一致）
- **場所**: `core/src/db.rs:618-622`
- **影響**: `delete_file_fully` は `count=0` 行を削除するが、`reindex_file` は削除しない。デッドロウ蓄積。
- **対処**: デクリメント後に `DELETE FROM tag_counts WHERE count = 0` を追加。

### [Medium] `_vault_paths` 死んだパラメータが残存

- **指摘者**: Refactoring Evangelist
- **場所**: `core/src/indexer.rs:405`
- **影響**: `build_path_map` 置換後、`_vault_paths` は未使用（`_` プレフィックス付き）。呼び出し側は不要な Vec を構築し渡している。
- **対処**: パラメータをシグネチャから完全削除。

### [Medium] create_schema のインデックス定義がマイグレーションと不一致

- **指摘者**: Data Integrity Expert
- **場所**: `core/src/db.rs:383`
- **影響**: `create_schema` は `ON chunks(file_path)`、マイグレーション v3 は `ON chunks(vault_name, file_path)`。
- **対処**: `create_schema` のインデックスを `ON chunks(vault_name, file_path)` に変更。

### [Medium] RC ベータ版クレートの本番使用

- **指摘者**: Supply Chain Sentinel
- **場所**: `core/Cargo.toml:46` (notify 9.0.0-rc.4), `core/Cargo.toml:38` (ort 2.0.0-rc.12)
- **対処**: RC バージョンの利用理由をコメントに記録。stable アップグレードのトリガー条件を明確化。

### [Medium] `upsert_file_cache` が char_count を更新しない公開API

- **指摘者**: System Architect
- **場所**: `core/src/db.rs:547-564`
- **対処**: doc comment で警告済み。`pub(crate)` に制限を検討。

### [Medium] reindex_file / index_file_with_embedder の引数過多

- **指摘者**: Legacy Bridge Architect
- **場所**: `core/src/db.rs:575` (10引数), `core/src/indexer.rs:397` (9引数)
- **対処**: パラメータ構造体へのリファクタリングを検討。

---

## コンフリクト調整結果

- **char_count backfill**: Data Integrity + SRE/Ops が指摘。Test Experts が修正済み（backfill SQL 削除）→ ✅ 解決済み
- **tag_counts ゼロカウント**: Data Integrity + SRE/Ops + Test Experts が指摘 → reindex_file 内に追加必要
- **前回レビューから変更なしの指摘**: MCP エラーパス漏洩（修正済み）、MCP vault auth（修正済み）、Watcher tag_counts（修正済み）

---

## 未完了エージェント

なし（22名全員完了）

---

## Test Experts による修正内容

1. **Migration v10 backfill SQL 削除** — `LENGTH(content)` バックフィルを削除
2. **reindex_file tag_counts cleanup** — ゼロカウント行の物理削除を追加
3. **delete_file_fully incoming note_links** — `target_path` 方向のリンク削除を追加
4. テスト5件新規作成、全379件合格
