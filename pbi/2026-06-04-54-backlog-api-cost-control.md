# PBI-54: Embedding API コスト上限とフォールバック機構

**発端:** FinOps Consultant (スコア70)
**影響:**
1. Embedding API にコスト上限がない。大量のテキスト埋め込みで予期せぬAPIコストが発生する可能性
2. vec_chunks ストレージ増加に対する圧縮・間引き戦略がない
**対処:**
1. Embedding API に月間/日次のコスト上限設定を追加
2. API障害時にFTS5にフォールバックする機構
3. vec_chunks の定期的な圧縮・削除オプション
**工数:** 3-5日
