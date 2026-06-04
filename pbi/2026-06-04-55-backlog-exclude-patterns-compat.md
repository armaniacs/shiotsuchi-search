# PBI-55: exclude_patterns → exclude_dirs 後方互換性

**発端:** Legacy Bridge Architect (スコア90)
**影響:** `exclude_patterns` → `exclude_dirs` のリネームに後方互換性がない。古い設定ファイルで `exclude_patterns` を使用している場合、設定が無視される
**対処:** `IndexingConfig` のデシリアライズ時に `exclude_patterns` を `exclude_dirs` にフォールバックするロジックを追加
**工数:** 0.5-1日
