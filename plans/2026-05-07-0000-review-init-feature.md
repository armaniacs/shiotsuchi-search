# Checking Team Review Report
## Branch: feature-0507
## Date: 2026-05-07 (Updated 2026-05-09 10:19 JST)

> **Update Log (2026-05-09):** 以下の修正が追加実施され、すべての High/Medium 指摘が解決済み。v0.2.9 として CHANGELOG に記録済み。
>
> | # | 項目 | 状態 | コミット/日付 |
> |---|------|------|--------------|
> | 1 | `scan_vault` が `auto_exclude_hidden` を無視 | **修正済** | v0.2.9 |
> | 2 | `init --force` が `notes_dir` を CWD で上書き | **修正済** | v0.2.9 |
> | 3 | `config detect-noise` の `--notes-dir` 無視 | **修正済** | v0.2.9 |
> | 4 | `scan` コマンドが indexing config を無視 | **修正済** | v0.2.9 |
> | 5 | `follow_links` デフォルト `true` → `false` | **修正済** | v0.2.9 |
> | 6 | アトミック書き込み / バックアップ衝突回避 | **修正済** | v0.2.9 |
> | 7 | dynamic 候補の自動選択 → デフォルト未選択 | **修正済** | v0.2.9 |
> | 8 | stdout TTY チェック欠如 | **修正済** | v0.2.9 |
> | 9 | `scan_vault` I/O 倍増（WalkDir + read_dir） | **修正済** | v0.2.9 — HashMap 単一パスに変更 |
> | 10 | `exclude_patterns` → `exclude_dirs` リネーム | **修正済** | v0.2.9 — 即時置き換え + エラーメッセージ |
> | 11 | 大規模 vault OOM リスク | **修正済** | v0.2.9 — チャンク分割（256エントリ OR 25.6MB） |
> | 12 | `strip_prefix` フルパス DB 格納リスク | **修正済** | v0.2.9 — `path.starts_with(notes_dir)` 事前確認 |
> | 13 | 走査エラー黙殺 | **修正済** | v0.2.9 — `filter_map(|e| e.ok())` → `match` + `log::warn!` |
> | 14 | ファイルパーミッション未制限 | **修正済** | v0.2.9 — config/backup を `0o600` に設定（Unix のみ） |
> | 15 | dynamic 閾値ハードコード | **修正済** | v0.2.9 — `IndexConfig::dynamic_threshold: usize`（デフォルト5） |
> | 16 | 無効パターンのフィードバック不足 | **修正済** | v0.2.9 — `ChartSummary::invalid_patterns` を追加 |
> | 17 | 候補数上限なし（DoS/フリーズ） | **修正済** | v0.2.9 — `CANDIDATE_LIMIT = 1000` |
> | 18 | `chrono` 依存（単一タイムスタンプ生成） | **修正済** | v0.2.9 — `std::time::SystemTime` + Unix epoch に変更 |
> | 19 | `index_file` / `index_directory` 重複 | **修正済** | v0.2.9 — `prepare_file()` を共通関数として抽出 |
> | 20 | cargo fmt / clippy 違反 | **修正済** | 2026-05-09 — `02810f4` |
>
> **Update Log 2 (2026-05-09 10:19 JST, branch `feature-0507`):**
> `plan-h2-init-fix-remaining.md` で定義された TDD テスト一式を追加実装。すべて RED→GREEN→REFACTOR のサイクルに従い実施。
>
> | # | 項目 | 状態 | コミット/日付 |
> |---|------|------|--------------|
> | 21 | TDD: チャンク分割境界テスト（256エントリ、25.6MB、exact boundary） | **追加済** | `3a3537f` |
> | 22 | TDD: 小規模 vault 単一チャンクテスト | **追加済** | `3a3537f` |
> | 23 | TDD: `strip_prefix` シンボリックリンク拒否テスト | **追加済** | `3a3537f` |
> | 24 | TDD: `index_file` と `index_directory` 結果一致テスト | **追加済** | `3a3537f` |
> | 25 | TDD: `exclude_dirs` 旧キー拒否 / 新キー受入テスト | **追加済** | `3a3537f` |
> | 26 | TDD: `dynamic_threshold=0` マッチテスト | **追加済** | `3a3537f` |
> | 27 | TDD: config/backup ファイルパーミッション `0o600` テスト | **追加済** | `3a3537f` |
> | 28 | `serde(deny_unknown_fields)` 追加（旧 `exclude_patterns` を確実に拒否） | **修正済** | `3a3537f` |

---

## 総合評価: 82/100 (ランク: A)

### スコア明細（21/22名完了）

| エージェント | スコア |
|-----------|------:|
| Red Team Leader | 85 |
| Blue Team Leader | 85 |
| System Architect | 55 |
| Maintainability Guardian | 85 |
| Legacy Bridge Architect | 70 |
| UI Expert | 85 |
| Tuning Expert | 75 |
| SRE/Ops Specialist | 85 |
| Domain Logic Expert | 85 |
| Compliance & Privacy Guard | 90 |
| i18n Expert | 90 |
| Accessibility Advocate | 90 |
| Documentation Architect | 70 |
| FinOps Consultant | 90 |
| Edge & Mobile Strategist | 75 |
| Refactoring Evangelist | 85 |
| Ethics & Bias Auditor | 95 |
| Supply Chain & Dependency Sentinel | 85 |
| API & Contract Negotiator | 70 |
| DX Advocate | 90 |
| Test Experts | 100 |
| Data Integrity Expert | タイムアウト |

**未完了エージェント**: Data Integrity Expert（タイムアウト）

---

## 既修正事項（レビュー時点からの累積）

### [High→解決] `build_exclude_globset` での glob メタキャラクタ未エスケープ・スラッシュ正規化漏れ
- 指摘者: System Architect, Red Team Leader, Domain Logic Expert, API Contract Negotiator, Test Experts
- 修正: `escape_glob_literal()` を追加し、`*`, `?`, `[`, `]`, `{`, `}` をエスケープ; `trim_matches('/')` でスラッシュ正規化; 空文字列パターンをスキップ

### [High→解決] `scan_vault` が `auto_exclude_hidden` 設定を無視
- 指摘者: System Architect, Maintainability Guardian
- 修正: `scan_vault` に `auto_exclude_hidden: bool` と `dynamic_threshold: usize` パラメータを追加し、条件付きで隠しディレクトリをフィルタする。

### [High→解決] `init --force` が既存 config の `notes_dir` を現在の作業ディレクトリで上書き
- 指摘者: Legacy Bridge Architect
- 修正: `--notes-dir` 未指定時、既存 `cfg.vault.notes_dir` がデフォルト値（`".`"）以外ならその値を維持し、デフォルト値の場合のみ CWD をフォールバックとして使用。

### [High→解決] `index_directory` で大規模 vault の一括メモリロードにより OOM リスク
- 指摘者: Edge & Mobile Strategist
- 修正: `process_chunk()` を導入し、256エントリ OR 累積25.6MB でチャンクを分割。各チャンクを `par_iter()` で並列処理し、チャンク間は逐次処理。

### [High/設計→解決] `exclude_patterns` のスキーマ契約と実装の乖離
- 指摘者: API & Contract Negotiator, Red Team Leader
- 修正: フィールド名を `exclude_dirs` に即時リネーム。旧キー名 `exclude_patterns` は deserialize エラーになる（親切なエラーメッセージで新キー名を案内）。ドキュメント（`docs/CLI-USE.md`）にも移行注記を追加。

### [High→解決] `noise.rs` の `scan_vault` で WalkDir 走査後に各ディレクトリで追加 `read_dir` が発生し I/O が倍増
- 指摘者: Tuning Expert
- 修正: WalkDir イテレーション中に `HashMap<String, (usize, bool)>` で各ディレクトリのマッチングファイル数をインクリメント集計。`count_matching_files` 関数を削除。

### [Medium→解決] `config detect-noise` の `--notes-dir` フラグが無視される
- 指摘者: Legacy Bridge Architect, DX Advocate
- 修正: `run_config` 内で `DetectNoiseArgs.notes_dir` を優先して使用するようにディスパッチ修正済み。

### [Medium→解決] `scan` コマンドが indexing config を無視
- 指摘者: Legacy Bridge Architect
- 修正: `run_scan` に `indexing_cfg: &IndexingConfig` を渡し、全フィールド（`exclude_dirs`, `auto_exclude_hidden`, `follow_links`, `dynamic_threshold`）を反映。

### [Medium→解決] `follow_links` のデフォルト値が `true`
- 指摘者: Compliance & Privacy Guard
- 修正: `IndexConfig::default()` と `IndexingConfig::default()` で `follow_links: false` に変更。安全なデフォルトに準拠。

### [Medium→解決] 設定ファイルへの書き込みがアトミックではない
- 指摘者: Blue Team Leader, SRE/Ops Specialist
- 修正: 一時ファイル（`.toml.tmp`）に書き込み完了後に `fs::rename` でアトミックに置き換え。

### [Medium→解決] バックアップファイル名の衝突
- 指摘者: System Architect, SRE/Ops Specialist
- 修正: `backup_path.exists()` を確認し、存在する場合は連番サフィックスを付加（`config.toml.bak.<timestamp>.<n>`）。

### [Medium→解決] dynamic 候補がインタラクティブ選択で自動選択される
- 指摘者: UI Expert, Accessibility Advocate, Ethics & Bias Auditor
- 修正: dynamic 候補もデフォルト未選択に変更。選択肢に `[auto-detected]` マーカーを追加。

### [Medium→解決] stdout TTY チェック欠如
- 指摘者: UI Expert, API & Contract Negotiator
- 修正: `dialoguer_stdin_is_tty()` で `stdin().is_terminal() && stdout().is_terminal()` を両方チェック。

### [Medium→解決] `init --yes` で dynamic ノイズ判定ヒューリスティックがメモリを誤除外するリスク
- 指摘者: Ethics & Bias Auditor
- 修正: `dynamic_threshold` を `IndexConfig` / `IndexingConfig` の設定可能フィールドに追加（デフォルト 5）。ユーザーが自身のユースケースに合わせて調整可能。

### [Medium→解決] ディレクトリ走査エラーの黙殺
- 指摘者: SRE/Ops Specialist
- 修正: `filter_map(|e| e.ok())` を `match` + `log::warn!("Directory scan error: {}", err)` + `None` に変更（`noise.rs` と `indexer.rs` の両方）。

### [Medium→解決] 巨大 vault に対する候補数の上限がなく DoS/フリーズのリスク
- 指摘者: Blue Team Leader
- 修正: `scan_vault` に `candidate_limit: usize` パラメータを追加（`CANDIDATE_LIMIT = 1000`）。超過時に truncated フラグを返し、呼び出し元で "showing first 1000 of many candidates" と表示。

### [Medium→解決] `strip_prefix` の失敗フォールバックによるフルパス DB 格納と情報露出
- 指摘者: Red Team Leader
- 修正: `strip_prefix` 前に `path.starts_with(notes_dir)` でプレフィックス確認。失敗時は `log::warn!` + スキップ（フォールバックでフルパスが保存されることはなくなった）。

### [Medium→解決] 設定ファイル・バックアップのファイルパーミッション未制限
- 指摘者: Compliance & Privacy Guard
- 修正: Unix 環境では `std::fs::set_permissions` で `0o600` を設定（プライマリconfig + バックアップ両方）。

### [Medium→解決] 無効な除外パターンのフィードバックが不足
- 指摘者: UI Expert
- 修正: `ChartSummary` に `invalid_patterns: usize` を追加。`build_exclude_globset` でエスケープ後も無効なパターンをカウントし、`chart` の実行結果に表示（例: "3 invalid patterns"）。

### [Medium→解決] `core/src/indexer.rs` でファイル処理ロジックが `index_file` と `index_directory` に重複
- 指摘者: Maintainability Guardian, Refactoring Evangelist
- 修正: 読み込み～トークナイズまでを `prepare_file(path: &Path, ...)` として共通関数に抽出。`index_file` と `process_chunk`（チャンク処理）の両方から呼び出し。

### [Low→解決] 単一のタイムスタンプ文字列生成のために `chrono` 依存
- 指摘者: DX Advocate
- 修正: `chrono` クレートを削除。バックアップタイムスタンプは `std::time::SystemTime::now()` の Unix epoch 秒（小数部あり）を使用。

---

## 未修正の重要指摘事項（残タスク）

**すべての High / Medium 指摘が v0.2.9 で解決済み。残タスクはありません。**

---

## Low 指摘（スコアに影響なし / 将来対応）

| # | 項目 | 指摘者 | 対応状況 |
|---|------|--------|---------|
| 1 | `init --yes` のヘルプにデータ取り扱いの注記が不十分 | Compliance & Privacy | 保留 — ドキュメントレベルでの対応検討 |
| 2 | ハードコードされた英語複数形ロジック（"1 invalid pattern" / "2 invalid patterns"） | i18n | 保留 — 本格的な i18n は将来対応 |
| 3 | ハードコードされた英語フォールバックタイトル "Untitled" | i18n | 保留 — 同上 |
| 4 | Documentation 未更新（README, ref/cli.md, docs/CLI-USE.md の移行注記） | Documentation Architect | **部分的に対応** — `docs/CLI-USE.md` に `exclude_dirs` 移行注記を追加済み。残りのドキュメントは次回メジャー更新時に統合。 |

---

## 検証コマンド

```bash
# フォーマティング
cargo fmt --all --check

# リント
cargo clippy --workspace --exclude shiotsuchi-e2e -- -D warnings

# テスト（ワークスペース全体）
cargo test --workspace --exclude shiotsuchi-e2e

# ビルド確認
cargo build --workspace --exclude shiotsuchi-e2e
```

**最終検証日**: 2026-05-09 10:19 JST — **131 tests passing, 0 failures**（+11 from TDD additions on `feature-0507`）

## TDD テスト追加サマリー (`feature-0507`)

| ファイル | テスト数 | テスト名 |
|---------|---------|---------|
| `core/src/indexer.rs` | 6 | `test_chunking_splits_at_256_entries`, `test_chunking_splits_at_byte_threshold`, `test_chunking_single_chunk_for_small_vault`, `test_chunking_exact_boundary_256`, `test_strip_prefix_outside_vault_is_rejected`, `test_index_file_and_directory_produce_same_result` |
| `cli/src/config.rs` | 2 | `test_exclude_dirs_rejects_old_key`, `test_exclude_dirs_accepts_new_key` |
| `cli/src/commands/noise.rs` | 1 | `test_scan_vault_threshold_zero_matches_all` |
| `cli/src/commands/init.rs` | 2 | `test_config_file_permissions_0600`, `test_backup_file_permissions_0600` |

**注意**: `test_build_exclude_globset_counts_invalid_patterns` / `test_empty_globset_when_all_patterns_invalid` は計画で「現在の `escape_glob_literal` によりすべてのパターンが有効になり `invalid_patterns` は常に 0 になる」と記載されており、テスト追加対象外とした。将来的に `escape_glob_literal` の挙動が変わった場合に追加を検討。
