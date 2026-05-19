# `shiotsuchi clean` Command Design

> **Status:** Implemented
> **Implemented in:** `cli/src/commands/clean.rs`
> **Completed:** 2026-05-18/19
> **Date:** 2026-05-18

## Goal

Add a `shiotsuchi clean` command that backs up the existing SQLite database,
deletes it, then re-indexes all vaults from scratch.

## Behavior

1. Resolve `db_path` from config (or `--db-path` flag / `SHIOTSUCHI_DB_PATH` env).
2. Resolve vaults from config.
3. If DB does not exist, print error and exit.
4. Backup all DB-related files in the same directory:
   - `db.sqlite3` → `db.sqlite3.bak.<unix_timestamp>`
   - `db.sqlite3-wal` → `db.sqlite3-wal.bak.<unix_timestamp>` (if exists)
   - `db.sqlite3-shm` → `db.sqlite3-shm.bak.<unix_timestamp>` (if exists)
5. Delete the original DB files.
6. Re-index all vaults (same logic as `run_chart`).
7. Print backup path and index summary.

## Files Changed

| File | Change |
|---|---|
| `cli/src/commands/clean.rs` | **NEW** — subcommand implementation |
| `cli/src/commands/mod.rs` | Add `pub mod clean;` |
| `cli/src/main.rs` | Add `Clean` variant + dispatch arm |

## Arguments

- None specific to clean. Uses global `--db-path` flag.
- Runs chart in quiet mode internally.
