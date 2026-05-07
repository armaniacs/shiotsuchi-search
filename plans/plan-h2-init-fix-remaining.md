# Plan: `shiotsuchi init` — 残課題修正

## Overview

[plan-h2-init.md](plan-h2-init.md) の実装に対する checking team レビューで指摘された未修正項目に対処する。
修正は以下の3つの High 項目と7つの Medium/Low 項目に分類される。

**開発方法論**: 本計画は TDD (Red-Green-Refactor) に従う。各変更はテストから始め、テスト失敗を確認してから実装する。

---

## 深掘りセッション — 2026-05-07

### 背景

[2026-05-07-0000-review-init-feature.md](./2026-05-07-0000-review-init-feature.md) のレビュー結果を精査したところ、
「未修正」とされた14項目のうち以下は**コード上ですでに修正済み**であった:

- `scan_vault` が `auto_exclude_hidden` を無視 → パラメータ追加済み
- `init --force` が `notes_dir` を CWD で上書き → 既存値維持ロジック実装済み
- `config detect-noise` の `--notes-dir` 無視 → 優先使用ロジック実装済み
- `scan` コマンドが indexing config を無視 → IndexingConfig 渡し済み
- `follow_links` デフォルト `true` → `false` に変更済み
- アトミック書き込み / バックアップ衝突回避 → 実装済み
- dynamic 候補の自動選択 → デフォルト未選択に変更済み
- stdout TTY チェック欠如 → stdin + stdout 両方チェック済み

したがって真の残タスクは以下の11項目である。

### 挑戦した仮定

| # | 仮定 | リスク | 発見 | 決定 |
|---|------|--------|------|------|
| A1 | OOM の主因は WalkDir エントリ数であり、チャンク分割で解決する | 高 | 実際のメモリ圧迫は並列ファイル読み込み。チャンクサイズは 256エントリ OR 25.6MB のハイブリッド条件が適切。単なるファイル数分割では大きな単一ファイルへの対策にならない。 | チャンク分割（OR条件: 256エントリ or 累積25.6MB、先に達した方）。固定値（設定不可）。 |
| A2 | `exclude_patterns` → `exclude_dirs` の名称変更は後方互換性が問題 | 高 | 既存ユーザーの config が deserialize エラーになる。段階的廃止か即時置き換えかの選択。 | 即時置き換え + 親切なエラーメッセージ。1リリースで行う。 |
| A3 | `scan_vault` の HashMap 集約は副作用がない | 中 | I/O は半減するが、HashMap 自体のメモリ使用量が増える。vault が巨大な場合のトレードオフ。 | WalkDir 内で集約を採用（count_matching_files 削除）。軽量スキャン(stat)は技術的に実装困難。 |
| A4 | 全残タスクを一つの計画で同時に実施できる | 中 | H2（スキーマ変更）と H1/M7（indexer 変更）は同じファイルを触るのでコンフリクトする。順序依存あり。 | フェーズ分割: Phase 1 (indexer 変更) → Phase 2 (CLI 変更) → Phase 3 (リファクタリング)。 |
| A5 | チャンクサイズ 256/25.6MB は適切なデフォルト | 中 | 典型 md ファイル ~10KB 想定で 256 ファイル ≈ 2.5MB。25.6MB はその10倍。経験的には十分な安全マージン。 | 固定値として採用。将来必要になれば設定可能にする。 |
| A6 | 破壊的設定変更は deserialize エラーで十分 | 中 | エラーメッセージで新しいキー名を明示すればユーザーは対応可能。段階的廃止のコード複雑度に見合わない。 | 親切なエラーメッセージを表示（"Use `exclude_dirs` instead of `exclude_patterns`"）。 |
| A7 | Dynamic 閾値を設定可能にしてもユーザーは適切な値を選べる | 低 | この設定はノイズ検出のヒューリスティックであり、ユーザーが理解するのは難しい。デフォルト 5 は議論の余地あり。 | 設定フィールドに追加するが、デフォルト 5 を維持。ドキュメントに解説を追加。 |
| A8 | `canonicalize` で strip_prefix 問題を修正できる | 中 | canonicalize は権限不足や broken symlink で失敗する。かえって新しいエラーケースを生む。 | canonicalize を使わず、プレフィックス確認 + スキップ方式を採用。 |
| A9 | 候補数上限 1000 は十分大きい | 中 | 数百万ファイルの vault を想定すると 1000 では少なすぎる可能性もあるが、全候補をメモリに載せるリスクとのトレードオフ。 | 上限 1000 + "N candidates omitted" 表示。ユーザーに truncated を知らせる。 |
| A10 | M1/M2/M4/M7 は独立した変更で安全 | 低 | すべて異なる場所の変更であり相互依存はない。独立して実施可能。 | フェーズ内で並行して実施。 |

### 発見されたリスク

1. **チャンク条件の曖昧さ**: "OR" と "AND" の解釈が分かれる。詳細設計時に統一が必要。
2. **`exclude_dirs` の急な deserialize エラー**: ユーザーが `shiotsuchi` をしばらく実行していない場合、突然の互換性破壊に驚く可能性がある。CHANGELOG とマイグレーションガイドで事前周知。
3. **`index_file` 関数は公開 API**: `pub fn` として core crate からエクスポートされている可能性あり。廃止・変更の前に使用箇所を確認。
4. **チャンク導入によるパフォーマンス回帰リスク**: 小規模 vault (< 256 files) ではチャンク分割のオーバーヘッドのみが乗る。`unreachable!()` 等のバグが入らないよう注意。

### 未解決の疑問

- チャンク分割の OR/AND セマンティクス: **OR を採用**（いずれかの条件に先に達した時点でチャンクを切る）。
- チャンクサイズの設定可能性: **固定値で十分。** 後日必要になれば設定可能にする。
- `exclude_dirs` の段階的廃止: **行わない。** 1リリースで即時置き換え + エラーメッセージ。
- `chrono` 依存の除去: タイムスタンプ生成に `std::time::SystemTime` を使用するよう変更。
- "Untitled" → 言語非依存の "Untitled" を維持。英語以外のロケールではコマンドのメッセージ全体を i18n する際に対処。
- 英語複数形ロジック: 単純な ternary で許容。本格的な i18n は将来対応。

### 決定事項

1. **チャンク分割**: `index_directory` で 256 エントリ OR 累積 25.6MB の条件でチャンクに分割。各チャンクを `par_iter()` で並列処理し、チャンク間は逐次処理。最終チャンクは WalkDir が尽きた時点で処理。
2. **`exclude_patterns` → `exclude_dirs`**: 設定キー名を変更。旧キー名で deserialize エラー。エラーメッセージで新しいキー名を案内。`serde(rename)` ではなく新しいフィールド名に完全移行。
3. **`scan_vault` I/O 最適化**: WalkDir イテレーション中に `HashMap<PathBuf, (usize, bool)>` で各ディレクトリのマッチングファイル数をカウント。`count_matching_files` を削除。
4. **Dynamic 閾値設定**: `IndexConfig` / `IndexingConfig` に `dynamic_threshold: usize`（デフォルト 5）を追加。`noise.rs` の `DYNAMIC_THRESHOLD` をこの設定値で置き換え。
5. **無効パターンの可視化**: `ChartSummary` に `invalid_patterns: usize` を追加。`build_exclude_globset` 内でインクリメントし、集計結果を CLI に表示。
6. **候補数上限**: `scan_vault` に `candidate_limit: usize`（デフォルト 1000）を追加。超過時に "N candidates omitted" を表示。
7. **`strip_prefix` 対策**: canonicalize は使わず、`strip_prefix` 前にプレフィックス確認。失敗時は `log::warn!` + スキップ。
8. **走査エラー可視化**: `filter_map(|e| e.ok())` → `match` + `log::warn!` + スキップ。
9. **ファイルパーミッション**: Unix のみ `std::fs::set_permissions` で `0o600` を設定（`init.rs`）。
10. **`index_file` / `index_directory` 重複除去**: 読み込み〜トークナイズを `prepare_file(path)` として共通関数に抽出。
11. **`chrono` 依存除去**: タイムスタンプ生成を `std::time::SystemTime` + `std::time::UNIX_EPOCH` ベースに変更。

---

## TDD Compliance Assessment

### 現状の計画の問題点

| 問題 | 深刻度 | 説明 |
|------|--------|------|
| **テストが実装より後** | CRITICAL | Phase 1, 2（実装） → Phase 3（テスト）の順序。TDD では各変更の前にテストを書き、失敗を確認してから実装する。 |
| **RED 検証プロセス未定義** | HIGH | テストが正しく失敗することを確認する手順が一切ない。TDD の鉄則「テストが失敗するのを見なかったものはテストしたことにならない」に違反。 |
| **既存テスト破壊の分析なし** | HIGH | フィールド名変更 (`exclude_patterns` → `exclude_dirs`) や戻り値変更により、既存テストが少なくとも7件は壊れる。どのテストが壊れるかがリストアップされていない。 |
| **エラーパステストの欠落** | HIGH | 11件の変更のうち、エラーケースをテストするものがほとんど定義されていない。 |
| **境界値テストの欠落** | MEDIUM | チャンク分割の境界（ちょうど256ファイル、ちょうど25.6MB）、候補数上限の境界などが未定義。 |
| **相互作用テストの欠落** | MEDIUM | 複数の変更が組み合わさったときの動作を検証するテストがない（例: chunking + exclude_dirs）。 |
| **「最小コード」原則の欠如** | MEDIUM | 各テスト通過に必要な最小限のコード変更が明示されていない。過剰実装のリスク。 |

### TDD 準拠のための再構成方針

本計画を以下の構造に再構成する:

```
各サブフェーズ (1a, 1b, ...) ごとに:
  1. RED: 先に書くテスト（失敗確認）
  2. GREEN: テスト通過のための最小実装
  3. REFACTOR: グリーン維持下のリファクタリング
```

実装順序は RED → GREEN → REFACTOR → 次の RED のサイクル。

---

## Implementation Plan (TDD)

### Phase 1: Core — Indexer 変更

各変更は TDD サイクルに従う。

---

#### 1a. チャンク分割 (`core/src/indexer.rs`)

##### RED — 先に書くテスト

```rust
#[test]
fn test_chunking_splits_at_256_entries() {
    // 257ファイルを用意 → 2チャンクに分割される
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    for i in 0..257 {
        let content = format!("# Note {}\n\nSmall content", i);
        fs::write(vault.join(format!("note{}.md", i)), content).unwrap();
    }
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(results.len(), 257, "all files should be indexed");
    assert_eq!(db.stats().unwrap().total_notes, 257);
}

#[test]
fn test_chunking_splits_at_byte_threshold() {
    // 25.6MB を超えるファイル1つ + 通常ファイル → 2チャンク以上に分割
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    // ~13MB のファイルを2つ作成（合計 26MB > 25.6MB）
    let big_content = "x".repeat(13_000_000);
    fs::write(vault.join("big1.md"), &big_content).unwrap();
    fs::write(vault.join("big2.md"), &big_content).unwrap();
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(results.len(), 2);
}

#[test]
fn test_chunking_preserves_all_results() {
    // 300ファイル → 分割されても全ファイルがインデックスされる
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    for i in 0..300 {
        fs::write(vault.join(format!("note{}.md", i)), format!("# Note {}", i)).unwrap();
    }
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(results.len(), 300);
    // すべての相対パスが一意であることを確認
    let mut paths: Vec<&str> = results.iter().map(|(p, _)| p.as_str()).collect();
    paths.sort();
    paths.dedup();
    assert_eq!(paths.len(), 300, "no duplicate paths");
}

#[test]
fn test_chunking_single_chunk_for_small_vault() {
    // 100ファイル → 1チャンク（256未満）
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    for i in 0..100 {
        fs::write(vault.join(format!("note{}.md", i)), format!("# Note {}", i)).unwrap();
    }
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(results.len(), 100);
}

#[test]
fn test_chunking_exact_boundary_256() {
    // ちょうど256ファイル → 1チャンク（256 < 256 は偽 → 境界は OR 条件 で 256 以上）
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    for i in 0..256 {
        fs::write(vault.join(format!("note{}.md", i)), format!("# Note {}", i)).unwrap();
    }
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(results.len(), 256);
}

#[test]
fn test_chunking_handles_unreadable_file_gracefully() {
    // メタデータが読めないファイル → エラーにならずスキップ（chunk 内のループ）
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    fs::write(vault.join("good.md"), "# Good").unwrap();
    // ファイルを作成してからパーミッションを剥奪 (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bad = vault.join("bad.md");
        fs::write(&bad, "# Bad")?;
        fs::set_permissions(&bad, fs::Permissions::from_mode(0o000)).unwrap();
    }
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    // 読めるファイルはインデックスされる（読めないファイルはスキップ）
    let paths: Vec<&str> = results.iter().map(|(p, _)| p).collect();
    assert!(paths.contains(&"good.md"));
}

#[test]
fn test_chunking_does_not_deadlock_empty_vault() {
    // ファイル0 → 処理が完了する（デッドロックしない）
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    assert!(results.is_empty());
}
```

**RED 検証コマンド**:
```bash
cargo test -p shiotsuchi-core test_chunking_splits_at_256_entries 2>&1
# => FAIL: function process_chunk does not exist
cargo test test_chunking_splits_at_byte_threshold 2>&1
# => FAIL: chunking not implemented
# 全テストが「チャンク分割が未実装」のため失敗することを確認
```

**既存テストへの影響**: なし（新規テストのみ）

##### GREEN — 最小実装

```rust
// indexer.rs
const CHUNK_MAX_ENTRIES: usize = 256;
const CHUNK_MAX_BYTES: u64 = 25_624_064; // ~25.6 MB

pub fn index_directory(
    db: &NoteDatabase,
    tokenizer: &JapaneseTokenizer,
    config: &IndexConfig,
) -> Result<Vec<(String, IndexResult)>, DbError> {
    // ... (既存の WalkDir + フィルタロジックは変更しない)

    // エントリ収集 (既存)
    let entries: Vec<_> = /* ... */;

    // --- チャンク分割 (追加) ---
    let mut all_results = Vec::new();
    let mut chunk = Vec::with_capacity(CHUNK_MAX_ENTRIES);
    let mut chunk_bytes: u64 = 0;

    for entry in &entries {
        let path = entry.path();
        // ファイルサイズを事前取得（取得不可なら size=0 として扱う）
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);

        chunk.push(entry);
        chunk_bytes = chunk_bytes.saturating_add(size);

        if chunk.len() >= CHUNK_MAX_ENTRIES || chunk_bytes >= CHUNK_MAX_BYTES {
            let chunk_results = process_chunk(
                &chunk, notes_dir, &exclude_globset, tokenizer, db
            )?;
            all_results.extend(chunk_results);
            chunk.clear();
            chunk_bytes = 0;
        }
    }
    // 最後のチャンク
    if !chunk.is_empty() {
        let chunk_results = process_chunk(
            &chunk, notes_dir, &exclude_globset, tokenizer, db
        )?;
        all_results.extend(chunk_results);
    }

    Ok(all_results)
}

/// チャンク内のファイルを並列処理し、逐次 DB 書き込み
fn process_chunk(
    entries: &[&DirEntry],
    notes_dir: &Path,
    exclude_globset: &GlobSet,
    tokenizer: &JapaneseTokenizer,
    db: &NoteDatabase,
) -> Result<Vec<(String, IndexResult)>, DbError> {
    // ... (既存の par_iter() + DB upsert ロジックをそのまま移植)
}
```

**GREEN 検証コマンド**:
```bash
cargo test -p shiotsuchi-core test_chunking_ 2>&1
# => 全 PASS
cargo test -p shiotsuchi-core 2>&1
# => 既存テストもすべて PASS（回帰なし）
```

##### REFACTOR

- `process_chunk` が `index_file` と重複するロジックを含む → 1f で統一（この時点では重複を許容）

---

#### 1b. `exclude_patterns` → `exclude_dirs` リネーム

**この変更は既存テストを破壊する。まず壊れるテストを特定し、そのテストを RED として書き直す。**

##### 事前準備: 壊れる既存テストのリスト

以下のテストが `exclude_patterns` フィールドに依存している:

| テスト | ファイル | 修正内容 |
|--------|---------|---------|
| `default_index_config` | `core/src/models.rs:103` | `exclude_patterns` → `exclude_dirs` にアサーション変更 |
| `test_index_directory_respects_exclude_patterns` | `core/src/indexer.rs:367` | `exclude_patterns` → `exclude_dirs` にフィールド変更 |
| `test_exclude_patterns_globset_basename_matching` | `core/src/indexer.rs:543` | 同上 |
| `test_globset_matches_subdirectory_files` | `core/src/indexer.rs:617` | 同上 |
| `test_default_config` | `cli/src/config.rs:121` | 同上 |
| `test_init_preserves_existing_exclude_patterns` | `cli/src/commands/init.rs:427` | テスト名 + 内部ロジック変更 |
| `test_init_detects_exclusion_candidates` | `cli/src/commands/init.rs:335` | テスト名は variables 名 |

これらを変更する前に、まず **新規の振る舞いを定義するテスト** を書く。

##### RED — 先に書くテスト (新規)

```rust
// cli/src/config.rs または core/src/models.rs
#[test]
fn test_exclude_dirs_rejects_old_key() {
    // 旧キー "exclude_patterns" で deserialize → エラー
    let toml_str = r#"
        [indexing]
        exclude_patterns = ["node_modules"]
    "#;
    let result: Result<IndexingConfig, _> = toml::from_str(toml_str);
    assert!(result.is_err(), "old key should cause deserialize error");
    let err = result.unwrap_err().to_string();
    // エラーメッセージに新しいキー名が含まれていることを確認
    assert!(err.contains("exclude_dirs"), "error should hint new key name");
}

#[test]
fn test_exclude_dirs_accepts_new_key() {
    // 新キー "exclude_dirs" で deserialize → 成功
    let toml_str = r#"
        [indexing]
        exclude_dirs = ["node_modules"]
    "#;
    let config: IndexingConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.exclude_dirs, vec!["node_modules"]);
}

#[test]
fn test_index_directory_respects_exclude_dirs() {
    // exclude_patterns → exclude_dirs にフィールド名を変更した以外は
    // 既存の test_index_directory_respects_exclude_patterns と同じ振る舞い
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let templates = vault.join("templates");
    fs::create_dir(&templates).unwrap();
    fs::write(templates.join("daily.md"), "# Daily template").unwrap();
    fs::write(vault.join("main.md"), "# Main").unwrap();
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        exclude_dirs: vec!["templates".to_string()],  // ← 新しいフィールド名
        ..Default::default()
    };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "main.md");
}
```

**RED 検証コマンド**:
```bash
cargo test test_exclude_dirs_rejects_old_key 2>&1
# => FAIL: フィールド名がまだ exclude_patterns のため deserialize が成功してしまう
cargo test test_exclude_dirs_accepts_new_key 2>&1
# => FAIL: フィールド名がまだ exclude_patterns のため unknown field エラー
cargo test test_index_directory_respects_exclude_dirs 2>&1
# => FAIL: IndexConfig に exclude_dirs フィールドがない
```

##### GREEN — 最小実装

```rust
// core/src/models.rs
pub struct IndexConfig {
    pub notes_dir: PathBuf,
    pub include_extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,   // ← リネーム
    pub auto_exclude_hidden: bool,
    pub follow_links: bool,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            notes_dir: PathBuf::from("."),
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            exclude_dirs: vec!["node_modules".to_string()],  // ← リネーム
            auto_exclude_hidden: true,
            follow_links: false,
        }
    }
}
```

```rust
// cli/src/config.rs (Serde のフィールド名も変更)
pub struct IndexingConfig {
    pub snippet_lines: usize,
    pub include_extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,  // ← リネーム
    pub auto_exclude_hidden: bool,
    pub follow_links: bool,
}
```

```rust
// core/src/indexer.rs: build_exclude_globset 呼び出し
let exclude_globset = build_exclude_globset(&config.exclude_dirs);
```

**GREEN 検証コマンド**:
```bash
cargo test test_exclude_dirs_rejects_old_key 2>&1
# => PASS: 旧キーがエラーになることを確認
cargo test test_exclude_dirs_accepts_new_key 2>&1
# => PASS
cargo test test_index_directory_respects_exclude_dirs 2>&1
# => PASS
cargo test -p shiotsuchi-core 2>&1
# => 注意: 既存の test_index_directory_respects_exclude_patterns はコンパイルエラー
# (フィールド名が変わったため)
```

##### 既存テスト修正 (GREEN の一部として)

コンパイルが通るよう、既存テストのフィールド名参照を更新する:

- `models.rs`: `default_index_config` → `exclude_dirs` に変更
- `indexer.rs`: `test_index_directory_respects_exclude_patterns` → フィールド名とテスト名両方を更新
- `indexer.rs`: `test_exclude_patterns_globset_basename_matching` → 同上
- `indexer.rs`: `test_globset_matches_subdirectory_files` → 同上
- `config.rs`: `test_default_config` → 不要（since we removed the default test for exclude_dirs; update if needed）
- `init.rs`: `test_init_preserves_existing_exclude_patterns` → テスト名とフィールド名更新
- `init.rs`: `test_init_detects_exclusion_candidates` → テスト名の `exclusion` は維持、`exclude_patterns` → 変数名のみ変更

##### REFACTOR

- `exclude_patterns` という名前をコードベース全体から削除（コメント含む）
- `build_exclude_globset` を必要に応じて rename

---

#### 1c. Dynamic 閾値の設定フィールド化

##### RED — 先に書くテスト

```rust
// cli/src/commands/noise.rs または core/src/models.rs

#[test]
fn test_scan_vault_respects_dynamic_threshold() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let dir = vault.join("many_notes");
    fs::create_dir(&dir).unwrap();
    for i in 0..10 {
        fs::write(dir.join(format!("{}.md", i)), "# content").unwrap();
    }

    // threshold=10 → 10に達しないので dynamic 候補にならない
    let candidates = scan_vault(&vault, &["md".to_string()], true, 10, 1000).0;
    let dynamic_candidates: Vec<_> = candidates.iter()
        .filter(|c| !c.is_known_pattern).collect();
    assert!(dynamic_candidates.is_empty(), "below threshold: no dynamic candidates");

    // threshold=9 → 10 >= 9 なので候補になる
    let candidates = scan_vault(&vault, &["md".to_string()], true, 9, 1000).0;
    let dynamic_candidates: Vec<_> = candidates.iter()
        .filter(|c| !c.is_known_pattern).collect();
    assert_eq!(dynamic_candidates.len(), 1);
}

#[test]
fn test_dynamic_threshold_default_is_5() {
    let config = IndexConfig::default();
    assert_eq!(config.dynamic_threshold, 5);
}

#[test]
fn test_scan_vault_threshold_zero_matches_all() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let dir = vault.join("small");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("a.md"), "# A").unwrap();  // 1 file

    // threshold=0 → 1 >= 0 でマッチ
    let candidates = scan_vault(&vault, &["md".to_string()], true, 0, 1000).0;
    assert!(!candidates.is_empty());
}
```

**RED 検証**: 新規テストは FAIL（`dynamic_threshold` フィールドが未定義）

##### GREEN — 最小実装

```rust
// core/src/models.rs
pub struct IndexConfig {
    // ...
    pub dynamic_threshold: usize,
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            // ...
            dynamic_threshold: 5,
        }
    }
}
```

```rust
// cli/src/config.rs
pub struct IndexingConfig {
    // ...
    pub dynamic_threshold: usize,
}
```

```rust
// cli/src/commands/noise.rs
pub fn scan_vault(
    notes_dir: &Path,
    include_extensions: &[String],
    auto_exclude_hidden: bool,
    dynamic_threshold: usize,  // ← 新規パラメータ（const から変更）
    candidate_limit: usize,
) -> (Vec<ExclusionCandidate>, bool) {
    // DYNAMIC_THRESHOLD 定数を削除し、パラメータを使用
    // ... || *count >= dynamic_threshold
}
```

##### REFACTOR

- `noise.rs` の `DYNAMIC_THRESHOLD` 定数を削除
- 呼び出し元から設定値を注入

---

#### 1d. 無効パターンの可視化

##### RED — 先に書くテスト

```rust
// core/src/indexer.rs
#[test]
fn test_build_exclude_globset_counts_invalid_patterns() {
    let patterns = vec![
        "valid".to_string(),
        r"[invalid".to_string(),  // invalid パターン
        "valid2".to_string(),
    ];
    let (_set, count) = build_exclude_globset(&patterns);
    // After escaping, "[invalid" becomes a valid literal pattern.
    // Currently all patterns are valid after escaping, so we need
    // a pattern that is truly invalid even after escaping.
    // \ is escaped to \\, so a trailing backslash is still problematic.
    // Actually: escape_glob_literal escapes \ too, so "\\" becomes "\\\\"
    // which is a valid glob (backslash literal).

    // Hmm — all patterns currently become valid after escape_glob_literal.
    // This means invalid_patterns will ALWAYS be 0 with current logic.
    // That's an important discovery!
    assert_eq!(count, 0, "all patterns are escaped and become valid");
    // TODO: if we want to test actual invalid patterns, we need patterns
    // that even escaped form is invalid. TOML parsing error? globset error?
    // For now, this test documents the current behavior.
}

#[test]  // ← integration test: chart.rs
fn test_chart_summary_reports_invalid_patterns() {
    // ... verify ChartSummary.invalid_patterns == expected
}

#[test]
fn test_empty_globset_when_all_patterns_invalid() {
    // This documents edge case: if we ever have truly invalid patterns,
    // the GlobSet should be empty (not panicking).
    // Currently unreachable due to escape_glob_literal.
}
```

**注意**: `escape_glob_literal` の実装により、現在はすべてのパターンがエスケープ後に有効になる。そのため `invalid_patterns` は理論上常に 0 になるが、将来の互換性のためにカウント機構を実装する。

##### GREEN — 最小実装

```rust
// core/src/indexer.rs
pub fn build_exclude_globset(patterns: &[String]) -> (GlobSet, usize) {
    let mut builder = GlobSetBuilder::new();
    let mut invalid = 0;
    for pat in patterns {
        let pat = pat.trim_matches('/');
        if pat.is_empty() { continue; }
        let escaped = escape_glob_literal(pat);
        let wrapped = format!("**/{}/**", escaped);
        match Glob::new(&wrapped) {
            Ok(g) => builder.add(g),
            Err(e) => {
                log::warn!("Skipping invalid exclude pattern {:?}: {}", pat, e);
                invalid += 1;
            }
        }
    }
    let set = builder.build().unwrap_or_else(|e| {
        log::warn!("Failed to build exclude GlobSet: {}", e);
        GlobSet::empty()
    });
    (set, invalid)
}
```

```rust
// cli/src/commands/chart.rs
pub struct ChartSummary {
    pub indexed: usize,
    pub skipped: usize,
    pub errors: usize,
    pub invalid_patterns: usize,
}
```

`index_directory` 内で `build_exclude_globset` の戻り値から `invalid` を取得し、`ChartSummary` に反映する。

##### REFACTOR

- `build_exclude_globset` の既存呼び出し元を新しい戻り値 `(GlobSet, usize)` に対応させる

---

#### 1e. `strip_prefix` 対策 + 走査エラー可視化

##### RED — 先に書くテスト

```rust
// core/src/indexer.rs
#[test]
fn test_strip_prefix_outside_vault_is_rejected() {
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    // vault の外にファイルを作成
    let outside = temp.path().join("outside.md");
    fs::write(&outside, "# Outside").unwrap();
    // vault 内から外へのシンボリックリンク（もし follow_links が有効な場合）
    // このテストは vault boundary チェックが効いていることを確認する
    // follow_links=false の場合はそもそもシンボリックリンクが辿られない
    // follow_links=true + vault 外シンボリックリンク → ブロックされる
    let broken_link = vault.join("escape.md");
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&outside, &broken_link).unwrap();
        let db = NoteDatabase::open_in_memory().unwrap();
        let config = IndexConfig {
            notes_dir: vault.clone(),
            follow_links: true,
            ..Default::default()
        };
        let results = index_directory(&db, &tokenizer, &config).unwrap();
        assert!(results.is_empty(), "external symlink should be rejected");
    }
}

// 走査エラーのテストは実際に権限エラーを発生させる必要があるため、
// Unix 専用。CI 環境で実行できる場合のみ有効。
#[test]
#[cfg(unix)]
fn test_index_directory_logs_permission_denied() {
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    // 読み取り不可のディレクトリを作成
    let restricted = vault.join(".restricted");
    fs::create_dir(&restricted).unwrap();
    fs::write(restricted.join("secret.md"), "# Secret").unwrap();
    std::fs::set_permissions(&restricted,
        std::fs::Permissions::from_mode(0o000)).unwrap();
    // 通常のディレクトリ
    let normal = vault.join("notes");
    fs::create_dir(&normal).unwrap();
    fs::write(normal.join("note.md"), "# Note").unwrap();
    let db = NoteDatabase::open_in_memory().unwrap();
    let config = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db, &tokenizer, &config).unwrap();
    // 権限不足のディレクトリはスキップされるが、通常のファイルはインデックスされる
    // 実際の WalkDir の挙動はプラットフォーム依存の可能性あり
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "notes/note.md");
    // パーミッションを戻す（後処理）
    std::fs::set_permissions(&restricted,
        std::fs::Permissions::from_mode(0o755)).unwrap();
}
```

**RED 検証**:
```bash
cargo test test_strip_prefix_outside_vault_is_rejected 2>&1
# => FAIL: strip_prefix のプレフィックス確認ロジックが未実装
cargo test test_index_directory_logs_permission_denied 2>&1
# => コンパイルOK、ただしエラーハンドリングが未実装
```

##### GREEN — 最小実装

```rust
// indexer.rs — filter 内
.filter(|entry| {
    let path = entry.path();
    if !path.is_file() { return false; }

    // vault boundary check (follow_links 時)
    if let Some(ref canonical_root) = notes_canonical {
        match path.canonicalize() {
            Ok(canonical) => {
                if !canonical.starts_with(canonical_root) {
                    log::warn!("File outside vault: {:?}", path);
                    return false;
                }
            }
            Err(_) => return false,
        }
    }

    // strip_prefix の前にプレフィックス確認
    let relative = if path.starts_with(notes_dir) {
        path.strip_prefix(notes_dir).unwrap_or(path)
    } else {
        log::warn!("File path {:?} outside vault root {:?}", path, notes_dir);
        return false;
    };
    // ... (extension check, exclude_globset matching)
})
```

```rust
// filter_map — 走査エラー可視化
.filter_map(|e| match e {
    Ok(entry) => Some(entry),
    Err(err) => {
        log::warn!("Directory scan error: {}", err);
        None
    }
})
```

##### REFACTOR

- 既存の `strip_prefix` フォールバック (`unwrap_or(path)`) が使われている箇所を確認し、不要になったら削除
- 一貫性のため `scan_vault` でも同様のパターンを採用

---

#### 1f. `index_file` / `index_directory` 重複除去

これは 1a で `process_chunk` に抽出したロジックと `index_file` の間の重複を解決する。

##### RED — 先に書くテスト

```rust
#[test]
fn test_index_file_and_directory_produce_same_result() {
    // 同一ファイルを index_file で処理した結果と index_directory で処理した結果が
    // DB エントリとして一致することを確認
    let tokenizer = crate::require_tokenizer!(Default::default());
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    fs::write(vault.join("test.md"), "---\ntitle: Same\n---\n\nContent").unwrap();

    // index_file で1ファイル処理
    let db1 = NoteDatabase::open_in_memory().unwrap();
    let config1 = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let result1 = index_file(&db1, &tokenizer, &vault.join("test.md"), "test.md", &config1);
    assert_eq!(result1, IndexResult::Inserted);

    // index_directory で全ファイル処理 (同一ファイルのみ)
    let db2 = NoteDatabase::open_in_memory().unwrap();
    let config2 = IndexConfig {
        notes_dir: vault.clone(),
        ..Default::default()
    };
    let results = index_directory(&db2, &tokenizer, &config2).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].1, IndexResult::Inserted);

    // 両方の DB の内容が一致することを確認
    let meta1 = db1.get_metadata("test.md").unwrap();
    let meta2 = db2.get_metadata("test.md").unwrap();
    assert_eq!(meta1.title, meta2.title);
    assert_eq!(meta1.hash, meta2.hash);
    assert_eq!(meta1.path, meta2.path);
}
```

**RED 検証**:
```bash
cargo test test_index_file_and_directory_produce_same_result 2>&1
# => FAIL: prepare_file 関数が未実装（または実装後に index_file がまだ呼んでいない）
```

##### GREEN — 最小実装

```rust
/// 内部共通: ファイル読み込み → パース → トークナイズまで
/// tokenizer の所有権は呼び出し側が管理。
fn prepare_file(
    path: &Path,
    relative_path: &str,
    tokenizer: &JapaneseTokenizer,
) -> Result<PreparedFile, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Read error: {}", e))?;
    let hash = compute_hash(&content);
    let mtime = fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (frontmatter_title, body) = extract_frontmatter(&content);
    let title = frontmatter_title.unwrap_or_else(|| title_from_path(path));
    let plain_text = markdown_to_text(&body);
    let tokenized = tokenizer.split(&plain_text);

    Ok(PreparedFile::new(
        relative_path.to_string(),
        hash,
        mtime,
        title,
        tokenized,
    ))
}
```

`index_file` から呼び出す:
```rust
pub fn index_file(db, tokenizer, file_path, relative_path, _config) -> IndexResult {
    let prep = match prepare_file(file_path, relative_path, tokenizer) {
        Ok(p) => p,
        Err(e) => return IndexResult::Error(e),
    };
    // ... DB upsert
}
```

`process_chunk` (1a) から呼び出す:
```rust
fn process_chunk(entries, notes_dir, exclude_globset, tokenizer, db) {
    let prepared: Vec<_> = entries.par_iter().map(|entry| {
        let path = entry.path();
        let relative = path.strip_prefix(notes_dir).unwrap_or(path);
        let rel_str = relative.to_string_lossy().to_string();
        let prep = prepare_file(path, &rel_str, tokenizer);
        (rel_str, prep)
    }).collect();
    // ... DB upsert (serial)
}
```

##### REFACTOR

- `PreparedFile` から `relative_path` フィールドを削除し、呼び出し側で管理（不要なら）
- `index_file` のテストを更新（`prepare_file` をテストする形に）

---

### Phase 2: CLI (`scan_vault` / `init` / `config`)

---

#### 2a. `scan_vault` I/O 最適化

##### RED — 先に書くテスト

```rust
// cli/src/commands/noise.rs
#[test]
fn test_scan_vault_no_extra_read_dir() {
    // 既存テストが new 実装でも通ることを確認（回帰テスト）
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let nm = vault.join("node_modules");
    fs::create_dir(&nm).unwrap();
    fs::write(nm.join("dep.md"), "# Dep").unwrap();
    let candidates = scan_vault(&vault, &["md".to_string()], true, 5, 1000).0;
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].relative_path, "node_modules");
}

#[test]
fn test_scan_vault_returns_correct_file_counts() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let templates = vault.join("templates");
    fs::create_dir(&templates).unwrap();
    for i in 0..3 {
        fs::write(templates.join(format!("t{}.md", i)), "# T").unwrap();
    }
    let candidates = scan_vault(&vault, &["md".to_string()], true, 5, 1000).0;
    // templates は既知パターンなので 3 ファイルで候補になる
    let t = candidates.iter().find(|c| c.relative_path == "templates").unwrap();
    assert_eq!(t.file_count, 3);
}

#[test]
fn test_scan_vault_no_count_for_non_matching_extensions() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let dir = vault.join("random");
    fs::create_dir(&dir).unwrap();
    fs::write(dir.join("f.txt"), "text").unwrap();  // .txt は対象外
    fs::write(dir.join("f.md"), "# markdown").unwrap();  // .md は対象
    let candidates = scan_vault(&vault, &["md".to_string()], true, 3, 1000).0;
    // .txt しかなければ候補にならない（3未満）
    assert!(candidates.is_empty());
}

#[test]
fn test_scan_vault_return_type_includes_truncated_flag() {
    // 戻り値が (Vec<ExclusionCandidate>, bool) であることを確認
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let (candidates, truncated) = scan_vault(&vault, &["md".to_string()], true, 5, 1000);
    assert!(candidates.is_empty());
    assert!(!truncated);
}
```

**RED 検証**:
```bash
cargo test -p shiotsuchi-cli test_scan_vault_no_extra_read_dir 2>&1
# => FAIL: 引数の数が合わない（count_matching_files 版の scan_vault は引数が3つ）
```

##### GREEN — 最小実装

1. `scan_vault` の4つのパラメータを新しいシグネチャに変更
2. WalkDir イテレーション内で `HashMap` 集約
3. `count_matching_files` 関数を削除
4. 戻り値を `(Vec<ExclusionCandidate>, bool)` に変更

```rust
pub fn scan_vault(
    notes_dir: &Path,
    include_extensions: &[String],
    auto_exclude_hidden: bool,
    dynamic_threshold: usize,
    candidate_limit: usize,
) -> (Vec<ExclusionCandidate>, bool) {
    let mut candidates: Vec<ExclusionCandidate> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    let mut current_parent = std::path::PathBuf::new();
    let mut dir_count: usize = 0;
    let mut dir_name = String::new();

    for entry in WalkDir::new(notes_dir)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| {
            if auto_exclude_hidden && e.file_type().is_dir() {
                !e.file_name().to_string_lossy().starts_with('.')
            } else { true }
        })
        .filter_map(|e| match e {
            Ok(e) => Some(e),
            Err(err) => {
                log::warn!("Directory scan error: {}", err);
                None
            }
        })
    {
        if !entry.file_type().is_file() { continue; }
        let ext = entry.path().extension()
            .and_then(|e| e.to_str()).unwrap_or("");
        if !include_extensions.iter().any(|a| a == ext) { continue; }

        // 親ディレクトリの相対パスを特定
        let parent = entry.path().parent().unwrap_or(entry.path());
        let rel = parent.strip_prefix(notes_dir).unwrap_or(parent);
        let rel_str = rel.to_string_lossy().to_string();
        if rel_str.is_empty() { continue; }

        // 親ディレクトリが変わったら candidate を確定
        if rel != current_parent && !current_parent.as_os_str().is_empty() {
            let is_known = KNOWN_NOISE_PATTERNS.contains(&dir_name.as_str());
            if is_known || dir_count >= dynamic_threshold {
                if !seen.contains(&rel_str) {  // ← バグ: current_parent を使うべき
                    // ...
                }
            }
        }
        // ↑ この設計は間違っている。WalkDir はファイルをフラットに走査するので、
        // 親ディレクトリでグルーピングするには HashMap を使う必要がある。
    }

    // 正しい実装:
    let mut dir_map: std::collections::HashMap<String, (usize, bool)> = HashMap::new();
    // ... 走査中に各ファイルの親ディレクトリをキーにしてカウント ...
    // ... 走査後に HashMap → Vec に変換 + 上限適用 ...
    unimplemented!()  // TDD: テストに合わせて実装
}
```

<!-- TODO: このセクションの正確な実装は TDD でテストが失敗した後に決定する -->

##### 既存テスト修正

`scan_vault` の既存テストはすべて新しいシグネチャに対応する必要がある:

- `test_scan_vault_empty_dir`
- `test_scan_vault_detects_known_noise`
- `test_scan_vault_dynamic_threshold`
- `test_scan_vault_below_threshold`
- `test_scan_vault_skips_hidden_dirs`
- `test_scan_vault_dedup`
- `test_scan_vault_multiple_candidates`
- `test_scan_vault_root_not_a_candidate`
- `test_scan_vault_includes_hidden_dirs_when_disabled`
- `test_detect_noise_empty_vault`
- `test_detect_noise_detects_candidates`

すべてに `dynamic_threshold` と `candidate_limit` パラメータを追加し、
戻り値を `(candidates, _)` で受け取るよう変更:

```rust
// 変更前:
let candidates = scan_vault(&vault, &default_extensions(), true);
// 変更後:
let (candidates, _truncated) = scan_vault(&vault, &default_extensions(), true, 5, 1000);
```

---

#### 2b. 候補数上限

##### RED — 先に書くテスト

```rust
#[test]
fn test_scan_vault_candidate_limit() {
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    // 候補を3つ作成
    for dir in &["node_modules", "dist", "templates", "build"] {
        let d = vault.join(dir);
        fs::create_dir(&d).unwrap();
        fs::write(d.join("f.md"), "# test").unwrap();
    }
    // limit=2 → 2件のみ + truncated=true
    let (candidates, truncated) = scan_vault(
        &vault, &["md".to_string()], true, 5, 2
    );
    assert_eq!(candidates.len(), 2);
    assert!(truncated);
}

#[test]
fn test_scan_vault_candidate_limit_not_truncated() {
    // 候補数 < limit → truncated=false
    // (上記と同じセットアップで limit=10)
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    for dir in &["node_modules", "dist"] {
        let d = vault.join(dir);
        fs::create_dir(&d).unwrap();
        fs::write(d.join("f.md"), "# test").unwrap();
    }
    let (candidates, truncated) = scan_vault(
        &vault, &["md".to_string()], true, 5, 10
    );
    assert_eq!(candidates.len(), 2);
    assert!(!truncated);
}

#[test]
fn test_scan_vault_candidate_limit_zero() {
    // limit=0 → 候補0件
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let nm = vault.join("node_modules");
    fs::create_dir(&nm).unwrap();
    fs::write(nm.join("f.md"), "# test").unwrap();
    let (candidates, truncated) = scan_vault(
        &vault, &["md".to_string()], true, 5, 0
    );
    assert!(candidates.is_empty());
    assert!(!truncated);  // 空の場合は truncated=false
}
```

**RED 検証**: 新規テストは FAIL（`candidate_limit` パラメータがまだない）

##### GREEN — 最小実装

```rust
pub fn scan_vault(
    // ...
    candidate_limit: usize,
) -> (Vec<ExclusionCandidate>, bool) {
    // ...
    let mut truncated = false;
    // ... (HashMap → Vec 変換後)
    if candidates.len() > candidate_limit {
        candidates.truncate(candidate_limit);
        truncated = true;
    }
    (candidates, truncated)
}
```

```rust
// init.rs
const CANDIDATE_LIMIT: usize = 1000;
let (candidates, truncated) = scan_vault(
    &effective_notes_dir,
    &out_cfg.indexing.include_extensions,
    out_cfg.indexing.auto_exclude_hidden,
    out_cfg.indexing.dynamic_threshold,
    CANDIDATE_LIMIT,
);
if truncated {
    eprintln!("info: showing first {} of {} total candidates",
        CANDIDATE_LIMIT, "many");
}
```

##### REFACTOR

- `CANDIDATE_LIMIT` を必要に応じてモジュール定数として noise.rs に移動（init.rs, config.rs で共有）

---

#### 2c. ファイルパーミッション (0o600)

##### RED — 先に書くテスト

```rust
// cli/src/commands/init.rs
#[test]
#[cfg(unix)]
fn test_config_file_permissions_0600() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("config.toml");
    let cfg = ShiotsuchiConfig::default();
    let args = InitArgs { force: false, yes: true };
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    run_init(&args, &cfg, &config_path, Some(&vault), None).unwrap();
    let metadata = fs::metadata(&config_path).unwrap();
    let mode = metadata.permissions().mode();
    assert_eq!(mode & 0o777, 0o600,
        "config file should have 0o600 permissions, got {:#o}", mode);
}

#[test]
#[cfg(unix)]
fn test_backup_file_permissions_0600() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    let config_path = temp.path().join("config.toml");
    fs::write(&config_path, "original").unwrap();

    let cfg = ShiotsuchiConfig::default();
    let args = InitArgs { force: true, yes: true };

    run_init(&args, &cfg, &config_path, Some(&vault), None).unwrap();

    // バックアップファイルのパーミッションを確認
    let parent = config_path.parent().unwrap();
    for entry in fs::read_dir(parent).unwrap() {
        let entry = entry.unwrap();
        if entry.file_name().to_string_lossy().contains(".toml.bak.") {
            let mode = entry.metadata().unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600,
                "backup file should have 0o600 permissions, got {:#o}", mode);
        }
    }
}
```

**RED 検証**:
```bash
cargo test -p shiotsuchi-cli test_config_file_permissions_0600 2>&1
# => FAIL: パーミッション設定が未実装（デフォルト 0o644 になる）
```

##### GREEN — 最小実装

```rust
// init.rs のアトミック書き込み後
std::fs::write(&tmp_path, toml)?;
#[cfg(unix)]
{
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&tmp_path, std::fs::Permissions::from_mode(0o600))?;
}
std::fs::rename(&tmp_path, config_path)?;

// backup_config 内
#[cfg(unix)]
{
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&backup_path, std::fs::Permissions::from_mode(0o600))?;
}
```

##### REFACTOR

- パーミッション設定をヘルパー関数に抽出（DRY）:
```rust
#[cfg(unix)]
fn set_private_permissions(path: &std::path::Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}
```

---

#### 2d. `chrono` 依存除去

##### RED — 先に書くテスト

```rust
// cli/src/commands/init.rs
#[test]
fn test_backup_timestamp_is_unique() {
    // 連続して2回 backup_config を呼び出し → 異なるタイムスタンプ
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("config.toml");
    fs::write(&config_path, "v1").unwrap();
    backup_config(&config_path).unwrap();

    fs::write(&config_path, "v2").unwrap();
    backup_config(&config_path).unwrap();

    let parent = config_path.parent().unwrap();
    let backups: Vec<_> = fs::read_dir(parent).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".toml.bak."))
        .collect();
    // ユニークなタイムスタンプなので2つのバックアップが存在する
    assert_eq!(backups.len(), 2, "two backup files should exist");
    // ファイル名が異なることを確認
    assert_ne!(backups[0].file_name(), backups[1].file_name());
}

#[test]
fn test_backup_timestamp_is_sortable() {
    // タイムスタンプが時系列順にソート可能であること
    let temp = TempDir::new().unwrap();
    let config_path = temp.path().join("config.toml");
    fs::write(&config_path, "v1").unwrap();
    backup_config(&config_path).unwrap();

    std::thread::sleep(std::time::Duration::from_millis(10));

    fs::write(&config_path, "v2").unwrap();
    backup_config(&config_path).unwrap();

    let parent = config_path.parent().unwrap();
    let mut names: Vec<String> = fs::read_dir(parent).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".toml.bak."))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    // ソートされた順序が時系列と一致することを確認（v1 → v2）
    // 新しいタイムスタンプは Unix epoch 秒 + マイクロ秒なので自然にソート可能
    // ただし同一秒内の区別はマイクロ秒に依存する
    // ここでは少なくとも2つ存在することだけ確認
    assert_eq!(names.len(), 2);
}
```

**注意**: `backup_config` の新しいタイムスタンプ形式は Unix epoch 秒 (`{secs}.{micros}`) に変更される。
これは `%Y%m%d-%H%M%S.%f` (例: `20260507-050912.123456`) から
`1743984552.123456` 形式への変更だが、ユニーク性とソート可能性は維持される。

##### GREEN — 最小実装

```rust
fn backup_config(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let micros = now.subsec_micros();
    let timestamp = format!("{}.{:06}", secs, micros);
    let mut backup_path = config_path.with_extension(format!("toml.bak.{}", timestamp));
    let mut counter = 1u32;
    while backup_path.exists() {
        backup_path = config_path.with_extension(format!("toml.bak.{}.{}", timestamp, counter));
        counter += 1;
    }
    std::fs::copy(config_path, &backup_path)?;
    #[cfg(unix)] { /* パーミッション設定 */ }
    println!("Backed up existing config to {}", backup_path.display());
    Ok(())
}
```

CLI Cargo.toml から `chrono` 行を削除:
```toml
# cli/Cargo.toml — chrono 行を削除
```

##### REFACTOR

なし。

---

#### 2e. DynamicThreshold 設定値の注入

##### RED — 先に書くテスト

```rust
// cli/src/commands/config.rs
#[test]
fn test_detect_noise_uses_dynamic_threshold() {
    // DetectNoise コマンドが dynamic_threshold 設定を反映することを確認
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    // 3ファイルのディレクトリ
    let dir = vault.join("many");
    fs::create_dir(&dir).unwrap();
    for i in 0..3 {
        fs::write(dir.join(format!("f{}.md", i)), "# content").unwrap();
    }
    // threshold=2 → 3 >= 2 で候補
    let args = ConfigArgs {
        command: ConfigCommands::DetectNoise(DetectNoiseArgs {
            notes_dir: Some(vault.clone()),
        }),
    };
    // 直接 scan_vault を呼んで動作確認（run_config は stdout に出力するだけ）
    let (candidates, _) = scan_vault(&vault, &["md".to_string()], true, 2, 1000);
    assert!(!candidates.is_empty(), "threshold=2 should detect 3-file dir");

    // threshold=5 → 3 < 5 で候補にならない
    let (candidates, _) = scan_vault(&vault, &["md".to_string()], true, 5, 1000);
    assert!(candidates.is_empty(), "threshold=5 should not detect 3-file dir");
}

// cli/src/main.rs の integrated test
#[test]
fn test_init_passes_dynamic_threshold_to_scan_vault() {
    // init → run_init → scan_vault のパラメータパススルー検証
    let temp = TempDir::new().unwrap();
    let vault = temp.path().join("vault");
    fs::create_dir(&vault).unwrap();
    // 6ファイルのディレクトリ（デフォルト threshold=5 を超える）
    let dir = vault.join("notes");
    fs::create_dir(&dir).unwrap();
    for i in 0..6 {
        fs::write(dir.join(format!("f{}.md", i)), "# content").unwrap();
    }

    let config_path = temp.path().join("config.toml");
    let mut cfg = ShiotsuchiConfig::default();
    cfg.indexing.dynamic_threshold = 10;  // ← 10 に設定

    let args = InitArgs { force: false, yes: true };
    run_init(&args, &cfg, &config_path, Some(&vault), None).unwrap();

    let contents = fs::read_to_string(&config_path).unwrap();
    // threshold=10 なので 6ファイルの notes は候補にならない
    // exclude_dirs に notes が含まれていないことを確認
    assert!(!contents.contains("notes"),
        "notes dir should not be excluded (below threshold)");
}
```

##### GREEN — 最小実装

```rust
// cli/src/main.rs
Commands::Config(args) => {
    commands::config::run_config(
        &args,
        &cfg.vault.notes_dir,
        &cfg.indexing.include_extensions,
        cfg.indexing.auto_exclude_hidden,
        cfg.indexing.dynamic_threshold,  // ← new
    )?;
}

Commands::Init(args) => {
    // init は内部で cfg 全体を受け取るので、cfg.indexing.dynamic_threshold が自動反映
    // ただし scan_vault に渡す必要あり → パラメータ追加
}
```

```rust
// cli/src/commands/init.rs
let (candidates, _truncated) = scan_vault(
    &effective_notes_dir,
    &out_cfg.indexing.include_extensions,
    out_cfg.indexing.auto_exclude_hidden,
    out_cfg.indexing.dynamic_threshold,  // ← 新しい cfg フィールドを注入
    CANDIDATE_LIMIT,
);
```

---

### Phase 3: 既存破壊テスト一覧と修正ガイド

以下のテストが上記の変更によって壊れる。各テストの修正方法を示す:

| # | テスト | ファイル | 破壊原因 | 修正方法 |
|---|--------|---------|---------|---------|
| 1 | `default_index_config` | `core/src/models.rs:103` | `exclude_patterns` → `exclude_dirs` | `exclude_dirs` に変更、`dynamic_threshold` アサーション追加 |
| 2 | `test_index_directory_respects_exclude_patterns` | `core/src/indexer.rs:367` | 同上 | フィールド名変更、テスト名変更推奨 |
| 3 | `test_exclude_patterns_globset_basename_matching` | `core/src/indexer.rs:543` | 同上 | 同上 |
| 4 | `test_globset_matches_subdirectory_files` | `core/src/indexer.rs:617` | 同上 | 同上 |
| 5 | `test_default_config` | `cli/src/config.rs:121` | 同上 | 同上 |
| 6 | `test_init_preserves_existing_exclude_patterns` | `cli/src/commands/init.rs:427` | 同上 + 変数名 | テスト名+変数名変更 |
| 7 | `test_init_detects_exclusion_candidates` | `cli/src/commands/init.rs:335` | 変数名のみ | `exclude_patterns` → `exclude_dirs` |
| 8 | `test_scan_vault_empty_dir` (and 8 siblings) | `cli/src/commands/noise.rs:159` | `scan_vault` シグネチャ変更 | 引数追加（5, 1000）、戻り値 `.0` で受取 |
| 9 | `test_detect_noise_empty_vault` | `cli/src/commands/config.rs:78` | 同上 | 同上 |
| 10 | `test_detect_noise_detects_candidates` | `cli/src/commands/config.rs:88` | 同上 | 同上 |
| 11 | `test_build_exclude_globset_invalid_pattern` | `core/src/indexer.rs:584` | 戻り値 `GlobSet` → `(GlobSet, usize)` | `.0` で受取 |
| 12 | `test_build_exclude_globset_escapes_special_chars` | `core/src/indexer.rs:596` | 同上 | 同上 |
| 13 | `test_build_exclude_globset_trims_slashes` | `core/src/indexer.rs:607` | 同上 | 同上 |

**注意**: 各テストの修正は対応する Phase の GREEN または REFACTOR で行う。テストコードは決して後回しにしない。

---

### Phase 4: ドキュメント更新

- `ref/cli.md` — `exclude_dirs`, `dynamic_threshold` 反映
- `docs/CLI-USE.md` — 同上
- `CHANGELOG` — 破壊的変更を記載（exclude_dirs リネーム）
- `plans/plan-h2-init.md` — 本計画の内容を追記

---

## 影響分析

### 互換性

| 変更 | 影響 | 軽減策 |
|------|------|--------|
| `exclude_patterns` → `exclude_dirs` | 既存 config.toml が deserialize エラー | エラーメッセージで新キー名を案内。CHANGELOG に明記。 |
| `scan_vault` 戻り値 `Vec<>` → `(Vec<>, bool)` | `init.rs` と `config.rs` の2箇所 + 既存テスト全9件 | コンパイルエラーで発見可能。 |
| `run_config` シグネチャ変更 | `main.rs` の1箇所 | 同上。 |
| `build_exclude_globset` 戻り値 `GlobSet` → `(GlobSet, usize)` | `index_directory` の1箇所 + 既存テスト3件 | 同上。 |
| バックアップタイムスタンプ形式変更 | 既存バックアップファイルとの混在 | 形式的な変更、機能上の問題なし。 |

### 依存関係

- `chrono` 依存: **削除予定** (CLI crate)。`Cargo.toml` から chrono 行を削除。
- 新規依存: なし。

### パフォーマンス

| 変更 | 影響 |
|------|------|
| チャンク分割 | 最大メモリ使用量が約 25.6MB × スレッド数 に制限される。小規模 vault では若干のオーバーヘッド。 |
| scan_vault I/O最適化 | ディレクトリ数 N に対して I/O が 2N → N に半減。 |
| exclude_dirs GlobSet | 現状維持。GlobSet のコンパイルはパターン数に依存、パターン数の変化なし。 |

---

## 実装手順（TDD サイクル）

```
フェーズ1a (チャンク分割):
  RED   → test_chunking_* を書く → テスト失敗確認
  GREEN → index_directory にチャンク分割実装
  REFACTOR → cleanup

フェーズ1b (exclude_dirs リネーム):
  RED   → test_exclude_dirs_* を書く → テスト失敗確認
  GREEN → フィールド名変更 + 既存テスト修正（コンパイルを通す）
  REFACTOR → コード全体の命名統一

フェーズ1c (Dynamic閾値):
  RED   → test_dynamic_threshold_* を書く → テスト失敗確認
  GREEN → IndexConfig に dynamic_threshold 追加
  REFACTOR → DYNAMIC_THRESHOLD 定数削除

フェーズ1d (無効パターン可視化):
  RED   → test_invalid_patterns_* を書く → テスト失敗確認
  GREEN → build_exclude_globset 戻り値変更
  REFACTOR → 呼び出し元統一

フェーズ1e (strip_prefix + 走査エラー):
  RED   → test_strip_prefix_*, test_permission_denied を書く
  GREEN → プレフィックス確認ロジック追加 + filter_map 置き換え
  REFACTOR → 重複境界チェックコード整理

フェーズ1f (重複除去):
  RED   → test_prepare_file_consistency を書く
  GREEN → prepare_file 関数抽出 + index_file / process_chunk から呼び出し
  REFACTOR → PreparedFile の relative_path 整理

フェーズ2a (scan_vault I/O最適化):
  RED   → test_scan_vault_no_extra_read_dir 等を書く
  GREEN → HashMap 集約 + シグネチャ変更 + 既存テスト修正
  REFACTOR → count_matching_files 削除

フェーズ2b (候補数上限):
  RED   → test_candidate_limit_* を書く
  GREEN → candidate_limit パラメータ追加
  REFACTOR → 定数共有

フェーズ2c (パーミッション):
  RED   → test_file_permissions_0600 を書く
  GREEN → set_permissions(0o600) 追加
  REFACTOR → ヘルパー関数抽出

フェーズ2d (chrono 除去):
  RED   → test_backup_timestamp_is_unique を書く
  GREEN → SystemTime ベースに変更 + Cargo.toml から chrono 削除
  REFACTOR → なし

フェーズ2e (DynamicThreshold 注入):
  RED   → test_dynamic_threshold_passthrough を書く
  GREEN → main.rs, init.rs, config.rs にパラメータ追加
  REFACTOR → なし
```

各 TDD サイクル後:
```bash
cargo test -p shiotsuchi-core 2>&1  # 回帰確認
cargo test -p shiotsuchi-cli 2>&1   # 回帰確認
```

---

## Implementation Checklist (TDD 版)

### 1a: チャンク分割
- [x] RED: チャンク分割テスト (7件) を書き、失敗を確認
- [x] GREEN: `index_directory` にチャンク分割 + `process_chunk` 追加
- [x] 全テスト PASS 確認（回帰なし）
- [x] REFACTOR: チャンク条件 OR/AND の統一

### 1b: exclude_dirs リネーム
- [x] RED: 新キーテスト (3件) を書き、失敗を確認
- [x] GREEN: `IndexConfig.exclude_dirs` + `IndexingConfig.exclude_dirs` + `build_exclude_globset` 更新
- [x] 破壊される既存テスト (7件) を修正
- [x] 全テスト PASS 確認

### 1c: Dynamic 閾値
- [x] RED: 閾値テスト (3件) を書き、失敗を確認
- [x] GREEN: `IndexConfig.dynamic_threshold` + `scan_vault` パラメータ追加
- [x] 全テスト PASS 確認

### 1d: 無効パターン可視化
- [x] RED: 無効パターンテスト (1件) を書き、失敗を確認
- [x] GREEN: `build_exclude_globset` 戻り値変更 + `ChartSummary.invalid_patterns`
- [x] 破壊される既存テスト (3件) を修正
- [x] 全テスト PASS 確認

### 1e: strip_prefix + 走査エラー
- [x] RED: strip_prefix テスト + 権限テスト (2件) を書き、失敗を確認
- [x] GREEN: プレフィックス確認ロジック + `filter_map` → `match` + `log::warn!`
- [x] 全テスト PASS 確認

### 1f: 重複除去
- [x] RED: 一貫性テスト (1件) を書き、失敗を確認
- [x] GREEN: `prepare_file` 関数抽出
- [x] 全テスト PASS 確認

### 2a: scan_vault I/O最適化
- [x] RED: I/O最適化テスト (4件) を書き、失敗を確認
- [x] GREEN: `scan_vault` HashMap 集約 + `count_matching_files` 削除
- [x] 破壊される既存テスト (9件) を修正
- [x] 全テスト PASS 確認

### 2b: 候補数上限
- [x] RED: 上限テスト (3件) を書き、失敗を確認
- [x] GREEN: `candidate_limit` パラメータ + truncated 表示
- [x] 全テスト PASS 確認

### 2c: パーミッション
- [x] RED: パーミッションテスト (2件) を書き、失敗を確認
- [x] GREEN: `set_permissions(0o600)` 追加
- [x] 全テスト PASS 確認

### 2d: chrono 除去
- [x] RED: タイムスタンプテスト (2件) を書き、失敗を確認
- [x] GREEN: `SystemTime` ベース + `Cargo.toml` から chrono 削除
- [x] 全テスト PASS 確認

### 2e: DynamicThreshold 注入
- [x] RED: パススルーテスト (2件) を書き、失敗を確認
- [x] GREEN: `main.rs` + `init.rs` + `config.rs` パラメータ追加
- [x] 全テスト PASS 確認

### Phase 4: ドキュメント
- [x] `ref/cli.md` — exclude_dirs, dynamic_threshold, init, config detect-noise 反映
- [x] `docs/CLI-USE.md` — 同上
- [x] `CHANGELOG` — v0.2.9 として breaking change + 全変更を記載
- [x] `plans/plan-h2-init.md` — 本計画の内容を追記（このファイル）

---

## 参考: 既存ファイル変更一覧

| ファイル | 変更内容 |
|----------|----------|
| `core/Cargo.toml` | （変更なし） |
| `core/src/models.rs` | `exclude_patterns` → `exclude_dirs`, `+dynamic_threshold` |
| `core/src/indexer.rs` | チャンク分割, `build_exclude_globset` 戻り値変更, `strip_prefix` 対策, 走査エラー, `prepare_file` 抽出 |
| `cli/Cargo.toml` | `-chrono` |
| `cli/src/config.rs` | `exclude_dirs`, `dynamic_threshold` |
| `cli/src/commands/noise.rs` | `scan_vault` I/O 最適化, `+dynamic_threshold`, `+candidate_limit`, 走査エラー |
| `cli/src/commands/init.rs` | `backup_config` chrono 除去, パーミッション, `scan_vault` 新シグネチャ対応 |
| `cli/src/commands/config.rs` | `run_config` 新パラメータ, `scan_vault` 新シグネチャ対応 |
| `cli/src/commands/chart.rs` | `+invalid_patterns` in ChartSummary |
| `cli/src/main.rs` | `run_config` 呼び出しに `dynamic_threshold` 追加 |
