# PBI-56: search() → SearchRequest 移行に伴うMCP互換性

**発端:** Legacy Bridge Architect, API & Contract Negotiator
**影響:** `search()` の17引数から `SearchRequest` 構造体への移行に伴うMCP互換性
**対処:** 互換レイヤーの検討またはドキュメント化
**工数:** 完了済み（内部移行のみ）
**状態:** 解決済み

## 解決状況

### 実装方針: 内部移行完了、外部互換レイヤーなし

`search()` の `SearchRequest` 移行は **内部実装の変更** であり、外部 API（MCP ツール、HTTP API）のシグネチャは変更されていない。

### 移行内容

- **v0.4.17**: `search()` の17引数を `SearchRequest` 構造体に統合
- **呼び出し元**: CLI (`dive`), MCP (`search_local_notes`), HTTP (`/api/v1/search`) を全て更新
- **テスト**: 全テスト通過確認済み

### 外部 API の影響

| API | 変更前 | 変更後 | 影響 |
|-----|--------|--------|------|
| MCP `search_local_notes` | ツール引数 | ツール引数（変更なし） | なし |
| HTTP `POST /api/v1/search` | クエリパラメータ | クエリパラメータ（変更なし） | なし |
| CLI `shiotsuchi dive` | CLI フラグ | CLI フラグ（変更なし） | なし |

### なぜ互換レイヤーが不要か

- `SearchRequest` は **内部実装** の変更
- 外部向け API（MCP ツール、HTTP エンドポイント）のシグネチャは維持
- ユーザーが直接 `search()` を呼び出すことはない（CLI/MCP/HTTP 経由のみ）

## 結論

この PBI は **クローズ** します。内部移行のみで外部互換性は維持されているため、追加の互換レイヤーは不要。
