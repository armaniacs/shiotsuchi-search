# Checking Team Review Report — `modify-2026-05-09a`

**Date:** 2026-05-09  
**Branch:** `modify-2026-05-09a` vs `main`  
**Agents:** 7/7 completed (Blue Team, Compliance & Privacy, Documentation, Maintainability, Data Integrity, System Architect, Legacy Bridge)

---

## 総合評価: 74/100 (ランク: B)

| エージェント | スコア |
|-----------|------:|
| Blue Team Leader | 72 |
| Compliance & Privacy Guard | 70 |
| Documentation Architect | 85 |
| Maintainability Guardian | 78 |
| Data Integrity Expert | 62 |
| System Architect | 72 |
| Legacy Bridge Architect | 75 |
| **平均** | **74** |

---

## 重要指摘事項（優先度順）

### [High] WAL/SHM コンパニオンファイルが `0o644` のまま
- **指摘者**: Blue Team, Compliance & Privacy, Data Integrity, System Architect（4名）
- **場所**: `core/src/db.rs:27-35`
- **影響**: `PRAGMA journal_mode = WAL` で作成される `<db>-wal` と `<db>-shm` が umask デフォルト (`0o644`) のまま。これらにはインデックスされたノート本文が含まれており、メイン DB の `0o600` 制限を完全に無力化する。
- **対処**: `init_schema()` 後にコンパニオンファイルも `0o600` に変更。または `umask(0o077)` で SQLite によるすべてのファイル作成を包括的に制限。

### [High] DB ファイル作成の TOCTOU 競合
- **指摘者**: Blue Team, Data Integrity, System Architect（3名）
- **場所**: `core/src/db.rs:25`
- **影響**: `!path.exists()` と `Connection::open()` の間で他プロセスがファイルを作成可能。`set_permissions` がスキップされ、デフォルトパーミッションの DB が作成される。
- **対処**: `O_CREAT | O_EXCL` 相当を使えない（rusqlite 未対応）ため、親ディレクトリを `0o700` に制限するか、一時ファイル + rename パターンを採用。

### [Medium] バックアップ失敗時に world-readable ファイルが残存
- **指摘者**: Compliance & Privacy Guard
- **場所**: `cli/src/commands/init.rs:162-167`
- **影響**: `fs::copy` は元ファイルのパーミッションを継承。`set_permissions` 失敗時、既にディスク上に world-readable バックアップが存在する。`?` でエラーは伝播するがファイルは消えない。
- **対処**: `set_permissions` 失敗時にバックアップファイルを削除してからエラーを返す。

### [Medium] 死んだ CLI フラグの即時削除が自動化を破壊
- **指摘者**: Legacy Bridge Architect
- **場所**: `cli/src/commands/chart.rs:14`, `cli/src/commands/scan.rs:13`
- **影響**: `--force` (chart) と `--debounce` (scan) 削除により、これらを使用するスクリプトやエイリアスが "unexpected argument" で強制終了する。
- **対処**: `#[arg(hide = true)]` で隠し、廃止警告を stderr に出力してから次のマイナーリリースで削除する。

### [Medium] DB 親ディレクトリが world-readable (`0o755`)
- **指摘者**: Compliance & Privacy Guard
- **場所**: `cli/src/commands/chart.rs:30-32`, `scan.rs:22-25`
- **影響**: `create_dir_all` で `~/.cache/shiotsuchi/` が `0o755` に作成される。ディレクトリ参照でファイル名・サイズ・タイムスタンプが漏洩。
- **対処**: 親ディレクトリを `0o700` に制限。

### [Medium] `set_permissions` 失敗が警告のみ
- **指摘者**: Blue Team, Data Integrity（2名）
- **場所**: `core/src/db.rs:31-34`
- **影響**: `log::warn!` で黙殺。セキュリティ要件の暗黙的な緩和となる。
- **対処**: `?` でエラーを伝播。または `error!` レベルで記録し、動作は継続（best-effort として文書化）。

### [Low] 文書: global `--verbose` 未記載
- **指摘者**: Documentation Architect
- **場所**: `docs/CLI-USE.md`, `docs/CLI-USE.ja.md`
- **対処**: Quick Start セクションに `--verbose` の言及を追加。

### [Low] 保守性: `WatcherConfig::debounce_ms` が孤立
- **指摘者**: Maintainability Guardian
- **場所**: `cli/src/config.rs:66-78`, `cli/src/commands/scan.rs:19`
- **対処**: `debounce_ms` を `WatcherConfig` から削除、または `VaultWatcher` で実際に使用する。

---

## コンフリクト調整結果

- **DB permissions エラー処理**: Blue Team/Data Integrity は `?` で伝播を推奨 vs Compliance & Privacy は警告で継続を容認。→ **両者を満たす折衷案**: 失敗時に `error!` レベルで記録し、かつ戻り値に `Result` を使う。ただし `warn!` のままでも High 指摘ではないため、Low として扱う。

## 未完了エージェント

なし。

---

## 修正計画

| フェーズ | 内容 | 対応指摘 | 状態 |
|---------|------|---------|------|
| 1 | WAL/SHM コンパニオンファイルの `0o600` 設定 | High #1 | **完了** (`9011e08`) |
| 2 | 親ディレクトリ `0o700` 制限 (chart.rs, scan.rs) | High #2 (代替), Medium #5 | **完了** (`9011e08`, `0c6aff9`) |
| 3 | バックアップ失敗時のクリーンアップ | Medium #3 | **完了** (`9011e08`) |
| 4 | `--force` / `--debounce` を隠しフラグ + 警告に復元 | Medium #4 | **完了** (`9011e08`, `0c6aff9`) |
| 5 | MCP `create_dir_all` エラー処理改善 | Medium (Compliance & Privacy) | **完了** (`0c6aff9`) |
| 6 | 文書更新: `--verbose`, `delete` オプション表 | Low #7 | **完了** (`6aac874`) |
| 7 | 保守性: `debounce_ms` 整理 | Low #8 | **保留** |

### Phase 1-5 実装詳細

#### `core/src/db.rs`
```rust
// init_schema() 後にコンパニオンファイルも制限
#[cfg(unix)]
if is_fresh {
    // main DB
    std::fs::set_permissions(&path, Permissions::from_mode(0o600))...;
    // WAL/SHM
    for suffix in ["-wal", "-shm"] {
        let companion = PathBuf::from(format!("{}{}", base, suffix));
        if companion.exists() { set_permissions(..., 0o600)... }
    }
}
```

#### `cli/src/commands/chart.rs` / `scan.rs`
```rust
#[arg(long, hide = true)]
pub force: bool,  // deprecated, warns on use

#[arg(long, hide = true)]
pub debounce: Option<u64>,  // deprecated, warns on use
```

#### `cli/src/commands/init.rs`
```rust
if let Err(e) = std::fs::set_permissions(&backup_path, Permissions::from_mode(0o600)) {
    let _ = std::fs::remove_file(&backup_path);  // cleanup on failure
    return Err(e.into());
}
```

### 検証結果

```bash
cargo fmt --all --check       # clean
cargo clippy --workspace ...  # clean
cargo test --workspace ...    # 133 passed, 0 failed
```
