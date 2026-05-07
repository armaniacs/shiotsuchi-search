# Plan: `shiotsuchi init` — Enhanced Implementation

## Goal

Provide a single command to bootstrap a user's local configuration file, with automatic detection of common noise directories and interactive exclusion prompts.

## Motivation

Currently, `shiotsuchi init` writes a static default config. Users may later discover that directories like `node_modules`, `dist`, or `templates` are being indexed, leading to noisy search results. Additionally, overwriting an existing config with `--force` is destructive. This plan addresses both issues by:

1. Automatically skipping hidden directories (`.git`, `.obsidian`, `.trash`, etc.) at the indexer level
2. Scanning the vault during `init` to detect exclusion candidates interactively
3. Backing up existing configs before overwrite

---

## Part 1: Auto-exclude hidden directories (indexer)

### Change
In `core/src/indexer.rs`, add `WalkDir::filter_entry` to prune any directory whose name starts with `.` before file matching begins.

```rust
WalkDir::new(notes_dir)
    .follow_links(false)
    .into_iter()
    .filter_entry(|e| {
        if e.file_type().is_dir() {
            !e.file_name().to_string_lossy().starts_with('.')
        } else {
            true
        }
    })
    .filter_map(|e| e.ok())
    // ... existing file filter logic
```

### Impact
- Hidden directories no longer need to be listed in `exclude_patterns`
- Reduces surprise for new users
- `exclude_patterns` remains useful for non-hidden noise dirs (`node_modules`, `dist`, `templates`, `archive`)

### Generated config change
```toml
[indexing]
include_extensions = ["md", "markdown"]
exclude_patterns = ["node_modules"]
```

**Note:** `.git` and `.obsidian` are removed from defaults because hidden dirs are now auto-excluded.

---

## Part 2: Interactive vault scan during `init`

### Flow

```
shiotsuchi init --notes-dir ~/Notes
  → Scan ~/Notes recursively for directories matching known noise patterns
  → Present multi-select prompt:
      "Exclude these directories from indexing?"
      [✓] node_modules (15 files)
      [✓] dist         (3 files)
      [ ] templates    (8 files)
  → Write user choices into exclude_patterns
```

### Known noise patterns
`["node_modules", "dist", "build", "templates", "archive", "archived"]`

### Scan logic
- Recursively walk `notes_dir`
- Skip hidden dirs (already auto-excluded)
- For each directory matching a known pattern, add to candidates
- For directories with `>= 5` matching files (by `include_extensions`), add to candidates
- Store relative paths (from `notes_dir`)

### Interaction
- Use `dialoguer::MultiSelect` for rich arrow-key / spacebar UI
- If stdin is not a TTY (CI, piped), fall back to defaults silently
- Pre-select all detected candidates

---

## Part 3: Backup before overwrite

### Change
When `--force` is used and the config file exists:

```rust
let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
let backup_path = config_path.with_extension(format!("toml.bak.{}", timestamp));
std::fs::copy(config_path, &backup_path)?;
println!("Backed up existing config to {}", backup_path.display());
```

### Behavior
- Single backup per overwrite operation
- Timestamped backups allow rollback without overwriting previous backups
- No prompt; backup is unconditional when `--force` is used

---

## Updated Command Reference

### `shiotsuchi init`

```
shiotsuchi init [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--force` | Overwrite existing config (creates `.bak.YYYYMMDD-HHMMSS.ffffff` backup) |
| `--yes` | Non-interactive mode: auto-accept all detected exclusion candidates (required when stdin is not a TTY) |

Global flags (`--notes-dir`, `--db-path`, `--verbose`) also apply and are passed through from the CLI.

### `shiotsuchi config detect-noise`

```
shiotsuchi config detect-noise [OPTIONS]
```

| Flag | Description |
|------|-------------|
| `--notes-dir <PATH>` | Vault root to scan (defaults to config's `notes_dir`) |

Scans the vault for exclusion candidates and prints them. Does not modify the config file.

---

## Edge Cases

| Scenario | Expected Behavior |
|----------|-------------------|
| Config dir does not exist | Create it automatically. |
| Config file exists, no `--force` | Error message, non-zero exit. |
| Config file exists, with `--force` | Create `.bak.YYYYMMDD-HHMMSS`, then overwrite. |
| Stdin is not a TTY, no `--yes` | Error: interactive mode required. Suggest `--yes` or running in a TTY. |
| Stdin is not a TTY, with `--yes` | Auto-accept all detected exclusion candidates, write config. |
| Stdin is a TTY, with `--yes` | `--yes` auto-accepts all candidates (consistent `--yes` behavior regardless of TTY). |
| `notes_dir` is empty or missing | No candidates detected; write empty `exclude_patterns`. |
| `notes_dir` has no matching dirs | Same as above. |
| User deselects all candidates | Write empty `exclude_patterns`. |

---

## Implementation Checklist

### Phase 1: Indexer
- [x] Add `filter_entry` to `WalkDir` in `core/src/indexer.rs`
- [x] Add test: `test_hidden_dir_auto_excluded`
- [x] Verify existing tests still pass

### Phase 2: Config defaults
- [x] Update `exclude_patterns` default in `cli/src/config.rs`
- [x] Update `test_default_config` assertion

### Phase 3: Dependencies
- [x] `dialoguer = "0.11"` was already in `cli/Cargo.toml`
- [x] `chrono = "0.4"` was already in `cli/Cargo.toml`
- [x] `walkdir = "2"` was already in `cli/Cargo.toml`
- [x] `globset = "0.4"` added to `core/Cargo.toml`

### Phase 4: Init command
- [x] Implement `scan_vault()` in `cli/src/commands/noise.rs` (shared module)
- [x] Implement 2-stage interactive UI with `dialoguer`
- [x] Implement `backup_config()` with timestamped `.bak` (microsecond precision)
- [x] Integrate scan results into generated `ShiotsuchiConfig`
- [x] Handle non-TTY: require `--yes` or error. Auto-accept all candidates when `--yes`.
- [x] Update user-facing messages

### Phase 5: Tests
- [x] `test_init_creates_config` (existing — updated signature)
- [x] `test_init_refuses_overwrite_without_force` (existing)
- [x] `test_init_overwrites_with_force` (existing)
- [x] `test_init_creates_timestamped_backup` (new)
- [x] `test_init_detects_exclusion_candidates` (new)
- [x] `test_init_creates_config_without_candidates` (new, covers non-TTY + `--yes` path)

### Phase 6: Documentation
- [x] Update `plans/plan-h2-init.md` (this file)
- [ ] Update `ref/cli.md` with new `--force` backup behavior
- [ ] Update `docs/CLI-USE.md` with interactive exclusion section
- [ ] Update `docs/CLI-USE.ja.md` with Japanese translation

---

## Files changed (implementation complete)

| File | Change |
|------|--------|
| `core/Cargo.toml` | +`globset = "0.4"` |
| `core/src/models.rs` | Added `auto_exclude_hidden`, `follow_links` to `IndexConfig`. Removed `.git`/`.obsidian` from defaults. |
| `core/src/indexer.rs` | `build_exclude_globset()`, conditional `filter_entry`, `follow_links(config)`, vault boundary on dirs + files, GlobSet matching. |
| `cli/src/config.rs` | Added `auto_exclude_hidden`, `follow_links` to `IndexingConfig`. |
| `cli/src/commands/noise.rs` | **New**: `scan_vault()` shared function, `ExclusionCandidate`, 28 known noise patterns + dynamic detection. |
| `cli/src/commands/init.rs` | **Rewritten**: `--yes`, backup (microsecond `.bak`), 2-stage UI, vault scan, TTY handling, notes_dir validation. |
| `cli/src/commands/config.rs` | **New**: `shiotsuchi config detect-noise` subcommand. |
| `cli/src/commands/chart.rs` | Passes new `IndexConfig` fields from `IndexingConfig`. |
| `cli/src/commands/mod.rs` | Added `config`, `noise` modules. |
| `cli/src/main.rs` | Added `Config` subcommand; raw `--notes-dir`/`--db-path` to init. |

---

## Implementation Notes

### Deviations from initial spec

| Item | Initial Spec | Actual Implementation |
|------|-------------|---------------------|
| `--yes` in TTY mode | Silently ignored; interactive prompt proceeds normally. | Auto-accepts all candidates (consistent `--yes` semantics). |
| `--notes-dir` cwd check | Error if cwd != default notes_dir. | Uses cwd when not specified; prints info message. No hard error. |
| `exclude_patterns` glob matching | Double pattern `**/{pat}` + `**/{pat}/**`. | Single pattern `**/{pat}/**` (sufficient for all file-path cases). |
| Backup timestamp | `%Y%m%d-%H%M%S` (second precision). | `%Y%m%d-%H%M%S.%f` (microsecond precision). |
| `scan_vault()` location | Inside `init.rs`. | Extracted to shared `noise.rs` module for reuse by `config detect-noise`. |
| Dynamic threshold | `>= 5` files (undocumented heuristic). | Documented as `DYNAMIC_THRESHOLD = 5` with rationale comment. |
| `follow_links` vault boundary | Directory-only check. | Check on both directory AND file entries (symlink-to-file escape protection). |

### Test coverage

100 tests pass across the workspace (up from 95):

- **Core crate**: 53 tests (47 unit + 6 integration), including 5 new indexer tests
- **CLI crate**: 29 tests, including 8 new noise module tests, 5 new init tests, 2 new config tests
- **MCP crate**: 18 tests (unchanged)

### Files changed (summary)

| File | Change |
|------|--------|
| `core/Cargo.toml` | +`globset = "0.4"` |
| `core/src/models.rs` | Added `auto_exclude_hidden`, `follow_links` to `IndexConfig`. Removed `.git`/`.obsidian` from defaults. |
| `core/src/indexer.rs` | `build_exclude_globset()`, conditional `filter_entry`, `follow_links(config)`, vault boundary on dirs + files, GlobSet matching. |
| `cli/src/config.rs` | Added `auto_exclude_hidden`, `follow_links` to `IndexingConfig`. |
| `cli/src/commands/noise.rs` | **New**: `scan_vault()`, `ExclusionCandidate`, 8 tests. |
| `cli/src/commands/init.rs` | **Rewritten**: `--yes`, backup, 2-stage UI, vault scan, TTY handling. |
| `cli/src/commands/config.rs` | **New**: `shiotsuchi config detect-noise` subcommand. |
| `cli/src/commands/chart.rs` | Passes new `IndexConfig` fields from `IndexingConfig`. |
| `cli/src/commands/mod.rs` | Added `config`, `noise` modules. |
| `cli/src/main.rs` | Added `Config` subcommand; raw `--notes-dir`/`--db-path` to init. |

---

## 深掘りセッション — 2026-05-07

### 挑戦した仮定

| # | 仮定 | リスク | 発見 | 決定 |
|---|------|--------|------|------|
| A1 | ドット始まりのディレクトリは常に除外して安全 | 高 | 意図的に隠しディレクトリにノートを置くユースケースが存在しうる | `auto_exclude_hidden: bool` を IndexConfig/IndexingConfig に追加。filter_entry レイヤー + exclude_patterns の二重で除外し、フラグで無効化可能にする |
| A2 | 既知のノイズパターン6種で十分カバーできる | 高 | `__pycache__`, `.next`, `target`, `vendor` など多数の出力ディレクトリが存在する | 既知パターンを15〜20種に拡充し、加えて全サブディレクトリから動的に候補を検出する |
| A3 | 5ファイル以上の閾値が適切 | 中 | 動的スキャンにより閾値の意義が変化（全サブディレクトリが対象になる） | 動的スキャンで扱う。閾値は実装時に再検討 |
| A4 | `dialoguer::MultiSelect` が最適なUX | 中 | リストが長大になる可能性がある | 2段階UIを採用：①既知パターンの一括除外(Y/n) ②個別ディレクトリの多段選択 |
| A5 | `init` が排除設定の唯一の機会でよい | 高 | ユーザーは後日ディレクトリ構成を変更する | `shiotsuchi config detect-noise` の独立サブコマンドを追加 |
| A6 | non-TTY時のfallback = デフォルト(空)で問題ない | 中 | 未深掘り（実装時に確認） | — |
| A7 | `cli` に `walkdir` 追加は問題ない | 低 | — | 重複しても許容範囲 |
| A8 | `chrono` の追加がタイムスタンプに正当化される | 低 | — | 許容 |
| — | exclude_patterns は部分文字列マッチで十分 | 中 | gitignore 方式が自然（ユーザー意見） | gitignore 方式（glob/パターンマッチ）をこの計画内で採用。`*` や `**` を含むパターンが使えるようになる。globset 等の導入を検討 |
| — | WalkDir は follow_links(false) でよい | 中 | シンボリックリンク先も vault の一部として認識したい | indexer と scan の両方で `follow_links(true)` に変更。循環リンクはエラーにする。canonicalize チェックを導入して vault 外への逸脱を防止する |
| — | `--notes-dir` 未指定時はカレントディレクトリをスキャン | 中 | ユーザーが意図しないディレクトリをスキャンするリスク | init において、カレントディレクトリが notes_dir（のデフォルト `"."`）と一致しない場合はエラーメッセージを表示して exit 1 |

### 新たに発見したリスク

1. **gitignore 方式への移行が計画全体のスコープに影響する** — exclude_patterns のマッチセマンティクスが変わるため、indexer のフィルタロジック、既存ユーザーの config 互換性、テストアサーションのすべてに影響が及ぶ。globset crate の導入判断が必要。
2. **follow_links(true) の安全性** — walkdir の循環検出に依存するだけでは不十分。canonicalize + starts_with(notes_dir) チェックを indexer と scan 両方に追加する必要があり、search.rs と同様のパターンを一貫させる。
3. **2段階UI + 動的スキャンの実装複雑度** — 既知パターン15〜20種と動的検出結果の統合、重複排除、UI表示のバランスが設計難易度を上げている。
4. **独立サブコマンド `config detect-noise` と `init` の責任分担** — 両者で似た scan ロジックを持つことになり、コード重複のリスクがある。共通の scan_vault() 関数の抽出が必要。
5. **部分文字列マッチから gitignore 方式への移行による既存ユーザーの互換性** — 現状の `exclude_patterns = ["templates"]` が `path/to/templates_old` にマッチしなくなる等の挙動変化がある。

### 未解決の疑問（解消済み）

- A6: non-TTY 環境での挙動 → **`--yes` フラグを追加**。non-TTY で `--yes` があれば全候補を自動採択して続行。`--yes` なしなら「インタラクティブモードが必要です。--yes を使用するか TTY で実行してください」とエラー終了。
- バックアップのローテーション/クリーンアップ戦略 → **管理しない（ユーザー手動削除）**。シンプルを優先。
- 2段階UIの第2段階表示 → **常に全表示**。第1段階が No でも既知パターンは個別候補として第2段階に出現する。

### 決定事項

1. **IndexConfig に `auto_exclude_hidden: bool` を追加**（デフォルト true）。filter_entry で参照し、無効時も exclude_patterns の除外は別途適用される。
2. **IndexConfig に `follow_links: bool` を追加**（デフォルト true）。indexer と scan 両方で使用。canonicalize チェックで vault 外を遮断。
3. **exclude_patterns のマッチ方式を gitignore スタイルに変更**（この計画内で実施）。globset crate の導入を検討。
4. **既知ノイズパターンを 15〜20 種に拡充** + **全サブディレクトリの動的スキャン**。
5. **2段階UI** を採用：第1段階で既知パターンの一括除外確認、第2段階で個別ディレクトリの多段選択。
6. **`shiotsuchi config detect-noise`** 独立サブコマンドを新設し、排除候補の再検出を可能にする。
7. **init でのカレントディレクトリチェック**: `--notes-dir` 未指定時、カレントディレクトリが config の notes_dir デフォルトと一致しなければエラー終了。
8. **バックアップ**: timestamped `.bak` ファイル作成（`chrono` 使用）。ローテーションは行わず、ユーザーが手動削除する。
9. **`--yes` フラグを追加**: non-TTY 環境では `--yes` が必須（全候補を自動採択）。なければエラー終了。TTY 環境では無視される。
10. **2段階UI 第2段階は常に全表示**: 第1段階で No でも既知パターンは個別候補として第2段階に出現する。

### 更新が必要な実装チェックリスト（完了済み）

上記決定に伴い、以下のタスクが実装された：

- [x] `core/src/indexer.rs`: `filter_entry` に `auto_exclude_hidden` フラグを参照させる
- [x] `core/src/indexer.rs`: `follow_links(true)` に変更 + canonicalize チェック追加（ファイルエントリも含む）
- [x] `core/src/indexer.rs`: exclude_patterns のマッチを gitignore 方式に書き換え（globset）
- [x] `core/src/models.rs`: `IndexConfig` に `auto_exclude_hidden`, `follow_links` フィールド追加
- [x] `cli/src/config.rs`: `IndexingConfig` に `auto_exclude_hidden` フィールド追加
- [x] `cli/src/commands/config.rs`: 新規 — `shiotsuchi config detect-noise` の実装
- [x] `cli/src/commands/init.rs`: 2段階UI、scan_vault() 共通化（noise.rs に抽出）
- [x] 依存関係: `globset = "0.4"` を `core/Cargo.toml` に追加
- [x] テスト: gitignore 方式のマッチテスト（5件）、follow_links テスト、config サブコマンドテスト
- [x] 既存テスト: セマンティクス変更によるアサーションの更新
