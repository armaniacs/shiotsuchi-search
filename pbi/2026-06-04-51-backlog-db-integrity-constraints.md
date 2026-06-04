# PBI-51: FTS/vec 参照整合性制約と vec_chunks トランザクション

**発端:** Data Integrity Expert (スコア85)
**影響:**
1. v3→v4 マイグレーションで vec_chunks の DROP/CREATE がトランザクション外
2. FTS/vec 仮想テーブルに参照整合性制約がない (chunk削除時に残骸が残る可能性)
**対処:**
1. vec_chunks DROP/CREATE をトランザクション内に移動
2. 仮想テーブル運用の制約をドキュメント化 (sqlite-vec/ FTS5 は外部キー制約未対応のため)
**工数:** 1-2日
