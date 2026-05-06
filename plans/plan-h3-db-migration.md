# Plan: Database Schema Migration Strategy

**Issue**: H3 — SRE/Ops Specialist (Checking Team)
**Severity**: High
**Status**: Plan only (not implemented)

## Problem

The current codebase uses `PRAGMA user_version` to track schema version but has **no migration path for future schema changes**.

### Current State

- `core/src/db.rs:68-76`: Schema initialization checks `PRAGMA user_version`, sets to `1` if `0`
- Schema version is hard-coded to `1` (set on first run)
- No version tracking of individual schema components (indexes, tables, constraints)
- Adding new FTS5 columns or indexes would require manual DB deletion and rebuild (data loss)

### Risk

Users who upgrade to a new version with schema changes cannot migrate cleanly. They must:
1. Manually delete their DB file (`~/.cache/shiotsuchi/db.sqlite3`)
2. Re-run `shiotsuchi chart` to rebuild from scratch
3. Lose all indexing metadata (indexed_at timestamps, etc.)

## Design

### Approach: Incremental Migration Runner

Introduce a migration framework that applies schema changes sequentially by version number.

### Key Components

1. **`core/src/migration.rs`** — New module containing:
   - `Migration` struct: version number + up/down SQL
   - `MigrationRunner`: loads current version, applies pending migrations
   - Migration list: `fn migrations() -> Vec<Migration>`

2. **`core/src/db.rs` changes**:
   - Call `MigrationRunner::run(&conn)` after `PRAGMA journal_mode=wal` and before table creation
   - Remove hard-coded DDL from `open()` — let migrations handle initial schema (version 0→1)

3. **Minimal schema**:
   ```sql
   -- v1: Initial schema (exactly what `initialize_schema` does now)
   CREATE TABLE IF NOT EXISTS notes_meta (...)
   CREATE VIRTUAL TABLE IF NOT EXISTS notes_fts USING fts5(body, content=notes_meta, content_rowid=rowid)
   CREATE TRIGGER ...
   ```

### Migration Example

```rust
pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub up: &'static str,
    pub down: Option<&'static str>,
}

fn migrations() -> Vec<Migration> {
    vec![
        Migration {
            version: 1,
            description: "Initial schema: notes_meta + notes_fts",
            up: include_str!("../sql/v1-up.sql"),
            down: Some("DROP TABLE IF EXISTS notes_meta; DROP TABLE IF EXISTS notes_fts;"),
        },
        // Future: add v2, v3, etc.
    ]
}
```

### File Changes

| File | Change |
|------|--------|
| `core/src/migration.rs` | New file — migration framework |
| `core/src/lib.rs` | Add `pub mod migration;` |
| `core/src/db.rs` | Replace `initialize_schema` with `MigrationRunner::run()` |
| `core/src/db.rs` | Remove hard-coded DDL, reference migration v1 |
| `core/tests/migration.rs` | Add tests for migration up/down |
| `CHANGELOG.md` | Document migration support |

### Testing

- Test fresh DB initializes at latest version
- Test upgrade from v1→v2 (when adding future migrations)
- Test downgrade (rollback)
- Test no-op when already at latest version

## Trade-offs

- **Pros**: No data loss on upgrade, clean separation of schema concerns, testable
- **Cons**: Additional complexity for a tool that currently has only one schema version
- **Mitigation**: Keep migration framework lightweight (~50 lines of Rust), defer actual multi-version migrations until needed

## Not In Scope

- Automatic data migration (moving data between columns/tables)
- Online migration (schema changes while DB is in use)
- Remote/networked DB backup before migration
