# PBI: MCP 検索結果へのメタデータ自動付与

## ユーザーストーリー
AI に検索結果を渡すユーザーとして、作成日・更新日・リンク数などのメタデータも一緒に渡したい、なぜなら AI が「最新の情報を優先する」といった判断をするためにはメタデータが必要だから

## ビジネス価値
- AI が時系列・重要度を考慮した回答ができる
- ノートの信頼性・鮮度判断をAIに委ねられる

## BDD 受け入れシナリオ

```gherkin
Scenario: 検索結果にメタデータが含まれる
  Given MCP 経由で `search_notes` を呼ぶ
  When 検索結果が返される
  Then 各ノートに created_at, updated_at, backlink_count, tags が含まれる

Scenario: AI がメタデータを使って最新情報を優先できる
  Given 同じトピックの古いノートと新しいノートが存在する
  When AI が updated_at を参照する
  Then 新しいノートを優先して回答できる
```

## 受け入れ基準
- [x] `search_notes` レスポンスの各アイテムに `created_at`・`updated_at`・`tags`・`backlink_count` が含まれる
- [x] メタデータは `notes_meta` テーブルから取得する

## 見積もり
2 ポイント

## 技術的考慮事項
- 影響ファイル: `mcp/src/handler.rs`、`core/src/search.rs`
- `backlink_count` は別途リンク解析機能（PBI-12 依存）があれば活用

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
# ChunkSearchResult の現状フィールドを確認する
grep -n "struct ChunkSearchResult\|pub.*chunk\|pub.*score\|pub.*vault\|pub.*file" core/src/models.rs
# MCP レスポンスの現状を確認する
grep -n "SearchResult\|json!\|to_json\|mtime\|created_at" mcp/src/handler.rs | head -20
```

### 実装手順

1. **`ChunkSearchResult` に `mtime: Option<i64>` と `tags: Vec<String>` を追加する**（`core/src/models.rs`）

2. **`core/src/db.rs` の `get_chunks_by_ids` に JOIN を追加して mtime を取得する**  
   `file_cache` テーブルの `mtime` を JOIN で引いてくる：
   ```sql
   SELECT c.*, fc.mtime
   FROM chunks c
   LEFT JOIN file_cache fc ON fc.vault_name = c.vault_name AND fc.path = c.file_path
   WHERE c.rowid IN (...)
   ```

3. **MCP の JSON レスポンスに `updated_at`（mtime を ISO 8601 変換）と `tags` を追加する**

### 落とし穴

- `mtime` は Unix タイムスタンプ（ミリ秒）。AI に渡す際は ISO 8601 文字列（`"2026-01-15T10:30:00Z"`）に変換する。
- `ChunkSearchResult` のフィールド追加は `core/src/search.rs` の `build_results` 関数も修正が必要。
- tags は PBI-04（Frontmatter フィルタリング）が完了していない場合は空配列を返すフォールバックを実装する。

## Definition of Done
- [x] メタデータ付与のテストがパスする
- [x] コードレビュー完了
