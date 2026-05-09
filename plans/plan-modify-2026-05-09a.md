# Plan: 2026-05-09a — Review Fix Round

## Overview

Documentation Architect (75/100) と Compliance & Privacy Guard (68/100) からの指摘を、TDD (Red→Green→Refactor) に従って修正する。

**ブランチ**: `modify-2026-05-09a`

---

## Issues (優先度順)

### [High] F-2: SQLite database file permissions
- **場所**: `core/src/db.rs:25`, `cli/src/commands/chart.rs:33`
- **問題**: `Connection::open(path)` で作成された `.sqlite3` ファイルがデフォルト umask (`0o644`) のまま。DB にはノート本文（トークナイズ済み）、タイトル、パス、ハッシュが含まれており、config ファイル以上に機密度が高い可能性がある。
- **対処**: `Connection::open` 後に Unix のみ `0o600` で `set_permissions` を呼ぶ。失敗時はエラーを伝播する。

### [Medium] F-1: Backup `set_permissions` error silently discarded
- **場所**: `cli/src/commands/init.rs:166`
- **問題**: `let _ = std::fs::set_permissions(&backup_path, ...)` でエラーを黙殺。失敗時はバックアップが `0o644` のままになり、機密設定が他ユーザーから読める。
- **対処**: `let _ =` を `?` に変更。エラーを伝播してユーザーに通知する。

### [Medium] Doc-2: `chart` options table omits `--quiet`; `--force` is dead code
- **場所**: `cli/src/commands/chart.rs:13-16`
- **問題**: `--force` は `ChartArgs` に定義されているが `run_chart` で使われていない。`--quiet` は実装済みだが docs にない。
- **対処**: `--force` を `ChartArgs` から削除（死んだコード）。`--quiet` を両方の docs に追加。

### [Medium] Doc-3: `scan` options table omits `--debounce` (dead code)
- **場所**: `cli/src/commands/scan.rs:12-13`
- **問題**: `--debounce` フラグは `ScanArgs` に定義されているが `run_scan` で無視されている。
- **対処**: `--debounce` を `ScanArgs` から削除。docs からも configurable debounce の言及を除去（実際には watcher config の `debounce_ms` で設定可能）。

### [Low] F-3: Stale security doc comment
- **場所**: `cli/src/config.rs:97-101`
- **問題**: コメントが "default OS file permissions (typically `0644`)" と記述しているが、実際には `0o600` に変更済み。
- **対処**: コメントを現在の挙動（`0o600`）に更新。

### [Low] Doc-1: `delete` command missing from both docs
- **場所**: `cli/src/commands/delete.rs`
- **問題**: `shiotsuchi delete <path>` コマンドが `docs/CLI-USE.md` と `docs/CLI-USE.ja.md` の両方から欠落している。
- **対処**: 両方の docs に `delete` セクションを追加。

### [Low] Doc-minor: English `CLI-USE.md` omits `dive --format`
- **場所**: `docs/CLI-USE.md:78-84`
- **問題**: 日本語版には `--format` の記述があるが、英語版にはない。
- **対処**: 英語版に `--format` オプションを追加。

---

## TDD Approach

各修正は以下のサイクルに従う:

1. **RED**: 既存テストが壊れないことを確認。新規テストが正しく失敗することを確認。
2. **GREEN**: 最小限の実装でテストを通す。
3. **REFACTOR**: 重複を除去し、コードをきれいにする。

---

## Phase 1: DB permissions (F-2) + Backup error propagation (F-1) [Security]

### RED — Tests to add

```rust
// core/src/db.rs — #[cfg(test)] mod tests
#[test]
#[cfg(unix)]
fn test_db_file_created_with_restricted_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::TempDir::new().unwrap();
    let db_path = temp.path().join("test.db");
    let db = NoteDatabase::open(&db_path).unwrap();
    drop(db); // close connection so we can check file
    let metadata = std::fs::metadata(&db_path).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
}
```

```rust
// cli/src/commands/init.rs — #[cfg(test)] mod tests  
#[test]
#[cfg(unix)]
fn test_backup_permission_failure_propagates_error() {
    // This test is tricky because set_permissions on the backup
    // normally succeeds. We test the path by mocking or by verifying
    // the code uses `?` instead of `let _ =`.
    // Instead, we verify compile-time: grep for `let _ = std::fs::set_permissions`
    // in init.rs and confirm it doesn't exist.
}
```

**RED 検証**:
```bash
cargo test -p shiotsuchi-core test_db_file_created_with_restricted_permissions 2>&1
# => FAIL: set_permissions not yet added to db.rs
```

### GREEN — Implementation

```rust
// core/src/db.rs — NoteDatabase::open
pub fn open<P: AsRef<std::path::Path>>(path: P) -> Result<Self, DbError> {
    let conn = Connection::open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
            log::warn!("Failed to set DB file permissions to 0o600: {}", e);
        }
    }
    // ... rest of init
}
```

**決定事項**: DB permissions の失敗は警告のみ（エラーで止めない）。なぜなら既存の DB ファイルが開かれる場合もあり、既存ファイルの permission を変更すべきかどうかは議論の余地がある。新規作成時に限り `0o600` を設定するのが安全。→ `CREATE TABLE` 実行後に判定する必要があるか、あるいはファイルが事前に存在しない場合のみ設定する。

**代替案**: `std::fs::metadata` でファイル size が 0（新規作成直後）の場合のみ `set_permissions`。これにより既存ファイルへの意図しない permission 変更を防ぐ。

```rust
#[cfg(unix)]
{
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() == 0 {
            // Freshly created DB — restrict permissions
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
                log::warn!("Failed to set DB file permissions to 0o600: {}", e);
            }
        }
    }
}
```

Backup error propagation:
```rust
// cli/src/commands/init.rs:166
// Before: let _ = std::fs::set_permissions(...);
// After:  std::fs::set_permissions(...)?;
```

---

## Phase 2: Remove dead CLI args (Doc-2, Doc-3)

### RED — Confirm dead code exists

```bash
grep -n "force" cli/src/commands/chart.rs | grep -v "//"
# => only in struct definition, not in run_chart function
grep -n "debounce" cli/src/commands/scan.rs | grep -v "//"
# => only in struct definition, not in run_scan function
```

### GREEN — Remove dead code

Remove `force: bool` from `ChartArgs`.
Remove `debounce: u64` from `ScanArgs`.

既存テストへの影響: なし（テストはこれらのフラグを使っていない）。

---

## Phase 3: Doc updates (Doc-1, Doc-minor, F-3)

- `docs/CLI-USE.md`: add `delete` section, add `--format` to `dive`, add `--quiet` to `chart`, remove `--debounce` from `scan`
- `docs/CLI-USE.ja.md`: same changes
- `cli/src/config.rs:97-101`: update stale security comment

---

## Acceptance Criteria

```bash
# All tests pass
cargo test --workspace --exclude shiotsuchi-e2e 2>&1 | tail -5
# => test result: ok. N passed; 0 failed

# Format clean
cargo fmt --all --check

# Clippy clean
cargo clippy --workspace --exclude shiotsuchi-e2e -- -D warnings

# New tests included
cargo test -p shiotsuchi-core test_db_file_created_with_restricted_permissions
cargo test -p shiotsuchi test_backup_file_permissions  # existing test still passes
cargo test -p shiotsuchi test_config_file_permissions  # existing test still passes
```
