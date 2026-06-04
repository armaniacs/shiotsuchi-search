# PBI-48: migrate() モジュール分割

**発端:** Maintainability Guardian (スコア70) - migrate() 単一責任原則違反
**影響:** `core/src/db.rs` の `migrate()` が400行超で全バージョン分岐とロジックが複雑に入り組む
**対処:** バージョン別にモジュール分割 (例: `core/src/migration/v4.rs`, `v5.rs`...)
**工数:** 2-3日
