# PBI-56: search() → SearchRequest 移行に伴うMCP互換性

**発端:** Legacy Bridge Architect, API & Contract Negotiator
**影響:** `search()` の17引数から `SearchRequest` 構造体への移行に伴い、MCP `search_local_notes` ツールの内部シグネチャが破壊的に変更された。外部依存がある場合、deprecation期間なしの変更となる
**対処:** 
- 外部向けにはSearchRequestの旧形式引数を受け付ける互換レイヤーを検討
- または非互換であることをドキュメント化し、バージョンアップ手順を明記
**工数:** 1日
