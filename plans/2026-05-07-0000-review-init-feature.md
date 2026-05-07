# Checking Team Review Report
## Branch: main (HEAD~2...HEAD)
## Date: 2026-05-07

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

## 既修正事項（Test Experts により修正済み）

### [High→解決] `build_exclude_globset` での glob メタキャラクタ未エスケープ・スラッシュ正規化漏れ
- 指摘者: System Architect, Red Team Leader, Domain Logic Expert, API Contract Negotiator, Test Experts
- 修正: `escape_glob_literal()` を追加し、`*`, `?`, `[`, `]`, `{`, `}` をエスケープ; `trim_matches('/')` でスラッシュ正規化; 空文字列パターンをスキップ

### [Medium→解決] `scan_vault` で vault ルート自身が空文字列パターンとして書き込まれる
- 指摘者: Domain Logic Expert, Test Experts
- 修正: `rel_str.is_empty()` でスキップ、テスト追加

### [Medium→解決] `init --force` で既存の手動除外設定が完全に失われる
- 指摘者: Domain Logic Expert, API Contract Negotiator, Test Experts
- 修正: `selected_patterns` と既存 `exclude_patterns` をマージ（dedup）するように変更

---

## 修正適用済み事項（Review後に修正実行）

### Test Experts による修正
1. `build_exclude_globset` での glob メタキャラクタ未エスケープ・スラッシュ正規化漏れ（High）
2. `scan_vault` で vault ルート自身が空文字列パターンとして書き込まれる（Medium）
3. `init --force` で既存の手動除外設定が完全に失われる（Medium）

### オーケストレーターによる追加修正
4. `scan_vault` が `auto_exclude_hidden` 設定を無視（High）→ `auto_exclude_hidden: bool` パラメータを追加、呼び出し元から設定値を渡すように変更
5. `init --force` が既存 config の `notes_dir` を CWD で上書き（High）→ `--notes-dir` 未指定時は既存 `cfg.vault.notes_dir` を維持するように変更
6. `config detect-noise` の `--notes-dir` フラグが無視される（Medium）→ `DetectNoiseArgs.notes_dir` を優先して使用するようにディスパッチ修正
7. `scan` コマンドが indexing config を無視（Medium）→ `run_scan` に `indexing_cfg` を渡し、全フィールドを反映
8. `follow_links` のデフォルトが `true`（Medium）→ `false` に変更（安全なデフォルト）
9. 設定ファイルへの書き込みがアトミックではない（Medium）→ 一時ファイル + `fs::rename` でアトミック書き込み
10. バックアップファイル名の衝突（Medium）→ 連番サフィックスで重複回避
11. dynamic 候補がインタラクティブ選択で自動選択される（Medium）→ デフォルト未選択に変更
12. stdout TTY チェック欠如（Medium）→ `stdout().is_terminal()` も併せてチェック

---

## 未修正の重要指摘事項（残タスク）

### [High] `scan_vault` が `auto_exclude_hidden` 設定を無視して隠しディレクトリを常にスキップ
- 指摘者: System Architect, Maintainability Guardian
- 場所: `cli/src/commands/noise.rs:76-89`
- 影響: `auto_exclude_hidden = false` を設定しても、`noise` / `init` で隠しディレクトリ内の候補を検出できない。`.env`, `.ssh`, `.obsidian` などがインデックス対象になっていることに気づかないままセキュリティリスクが生じる。
- 対処: `scan_vault` に `auto_exclude_hidden: bool` パラメータを追加し、条件付きでフィルタする。呼び出し元から設定値を渡す。

### [High] `init --force` が既存 config の `notes_dir` を現在の作業ディレクトリで上書きする
- 指摘者: Legacy Bridge Architect
- 場所: `cli/src/commands/init.rs:35-45`
- 影響: `--notes-dir` を省略して `init --force` を実行すると、`std::env::current_dir()` が無条件で採用され、設定済みの vault ルートが失われる。
- 対処: `--notes-dir` が未指定の場合、既存 `cfg.vault.notes_dir` がデフォルト値以外ならその値を維持し、デフォルト値の場合のみ CWD をフォールバックとして使用する。

### [High] `index_directory` で大規模 vault の一括メモリロードにより OOM リスク
- 指摘者: Edge & Mobile Strategist
- 場所: `core/src/indexer.rs:186-279`
- 影響: WalkDir の全エントリを `Vec` に collect した後 `par_iter()` で全ファイルを同時に処理し、低スペックデバイスで OOM する可能性がある。
- 対処: バッチ単位で処理するか、`par_bridge()` を使用するか、ファイルサイズ上限を設ける。

### [High/設計] `exclude_patterns` のスキーマ契約と実装の乖離
- 指摘者: API & Contract Negotiator, Red Team Leader
- 場所: `core/src/indexer.rs:22-46`
- 影響: `exclude_patterns` は強制的に `**/{pat}/**` でディレクトリ名のみに限定しているが、CHANGELOG や設定ファイルでは「gitignore-style glob matching」と説明しており、ユーザーはファイル名パターンも除外できると期待する。
- 対処: フィールド名を `exclude_dirs` に変更するか、ファイル名マッチングも可能な実装にする。またはドキュメントで「ディレクトリ名のみ有効」と明記する。

### [High] `noise.rs` の `scan_vault` で WalkDir 走査後に各ディレクトリで追加 `read_dir` が発生し I/O が倍増
- 指摘者: Tuning Expert
- 場所: `cli/src/commands/noise.rs:76-138`
- 影響: ディレクトリ数が多い vault でシステムコールと I/O がほぼ2倍になり、スキャン時間が著しく長くなる。
- 対処: WalkDir のイテレーション中に `HashMap<PathBuf, usize>` でディレクトリごとのマッチングファイル数をインクリメント集計し、`count_matching_files` を削除する。

---

### [Medium] 設定ファイルへの書き込みがアトミックではない
- 指摘者: Blue Team Leader, SRE/Ops Specialist
- 場所: `cli/src/commands/init.rs:96-97`
- 影響: 書き込み途中にプロセスが終了すると設定ファイルが破損する。
- 対処: 一時ファイルに書き込み、完了後に `std::fs::rename` でアトミックに置き換える。

### [Medium] バックアップファイル名の衝突
- 指摘者: System Architect, SRE/Ops Specialist
- 場所: `cli/src/commands/init.rs:120-122`
- 影響: 高速連続実行・並列実行で過去のバックアップが上書きされる。
- 対処: `backup_path.exists()` を確認し、存在する場合は連番やランダムサフィックスを付加する。

### [Medium] `config detect-noise` の `--notes-dir` フラグが無視される
- 指摘者: Legacy Bridge Architect, DX Advocate
- 場所: `cli/src/commands/config.rs:24-30`, `cli/src/main.rs:109-111`
- 影響: CLI インターフェースが実装と不一致になる。
- 対処: `run_config` 内で `args.command` を展開し、`DetectNoiseArgs.notes_dir` があればそれを優先して使用する。

### [Medium] `scan` コマンドが indexing config を無視
- 指摘者: Legacy Bridge Architect
- 場所: `cli/src/commands/scan.rs:29-32`
- 影響: 同一 vault に対して `chart` と `scan` の挙動が不一致になる。
- 対処: `run_scan` の引数に `indexing_cfg: &IndexingConfig` を追加し、各フィールドを引き継ぐ。

### [Medium] `follow_links` のデフォルト値が `true`
- 指摘者: Compliance & Privacy Guard
- 場所: `core/src/models.rs:79`, `cli/src/config.rs:53`
- 影響: デフォルトでシンボリックリンクをフォローする。安全なデフォルトの観点から `false` に変更すべき。
- 対処: `follow_links` のデフォルト値を `false` に変更する。

### [Medium] 設定ファイル・バックアップのファイルパーミッション未制限
- 指摘者: Compliance & Privacy Guard
- 場所: `cli/src/commands/init.rs:97`, `:122`
- 影響: 同一ホストの他ユーザーから読み取り可能になる。
- 対処: Unix 環境では `0o600` に設定する。

### [Medium] 無効な除外パターンのフィードバックが不足
- 指摘者: UI Expert
- 場所: `core/src/indexer.rs:31-38`
- 影響: 無効な glob パターンがあっても `log::warn!` のみでユーザーが原因を把握しにくい。
- 対処: `ChartSummary` に `invalid_patterns: usize` を追加し、実行結果に表示する。

### [Medium] dynamic 候補がインタラクティブ除外選択で自動選択される
- 指摘者: UI Expert, Accessibility Advocate, Ethics & Bias Auditor
- 場所: `cli/src/commands/init.rs:169-177`
- 影響: ユーザーが存在を知らないディレクトリが勝手に選択され、メモリ等がインデックス対象から漏れる。
- 対処: dynamic 候補もデフォルト未選択とし、選択肢に `[auto-detected]` マーカーを表示する。

### [Medium] stdout TTY チェック欠如
- 指摘者: UI Expert, API & Contract Negotiator
- 場所: `cli/src/commands/init.rs:207-210`
- 影響: stdout がリダイレクトされている環境でプロンプトが表示されずにフリーズしたように見える。
- 対処: `std::io::stdout().is_terminal()` も併せてチェックする。

### [Medium] `init --yes` で dynamic ノイズ判定ヒューリスティックがメモリを誤除外するリスク
- 指摘者: Ethics & Bias Auditor
- 場所: `cli/src/commands/noise.rs:67`, `cli/src/commands/init.rs:86-91`
- 影響: 閾値（5ファイルで「ノイズ」判定）が、日記・講義ノートなどをまとめるユーザーを不利にする。
- 対処: `DYNAMIC_THRESHOLD` を設定可能にするか、閾値を引き上げる。または `--yes` 時に dynamic 候補のみを分離する。

### [Medium] ディレクトリ走査エラーの黙殺
- 指摘者: SRE/Ops Specialist
- 場所: `cli/src/commands/noise.rs:90`, `core/src/indexer.rs:212`
- 影響: 権限不足でディレクトリが読めない場合、単純にスキップされるだけで警告が出ない。
- 対処: `filter_map(|e| e.ok())` を `match` で置き換え、エラーを `log::warn!` で可視化する。

### [Medium] 巨大 vault に対する候補数の上限がなく DoS/フリーズのリスク
- 指摘者: Blue Team Leader
- 場所: `cli/src/commands/noise.rs:76-121`
- 影響: 数百万ファイルの vault でメモリ圧迫や UI フリーズの可能性がある。
- 対処: `candidates` の数に上限を設けるか、走査深度の上限を追加する。

### [Medium] `strip_prefix` の失敗フォールバックによるフルパス DB 格納と情報露出
- 指摘者: Red Team Leader
- 場所: `core/src/indexer.rs:233`, `:246`
- 影響: `strip_prefix` が失敗すると絶対パスが DB に保存され、ファイルシステム構造が露出する。
- 対処: `notes_dir` を事前に `canonicalize` し、`strip_prefix` の失敗を厳密にエラーとして処理する。

### [Medium] `core/src/indexer.rs` でファイル処理ロジックが `index_file` と `index_directory` に重複
- 指摘者: Maintainability Guardian, Refactoring Evangelist
- 場所: `core/src/indexer.rs:110-143`, `242-279`
- 影響: 両方を更新し忘れるリスクがある。
- 対処: 読み込み〜トークナイズまでの処理を共通関数に抽出する。

---

## コンフリクト調整結果

特に相反する指摘は確認されませんでした。

System Architect（55/100）が最も低いスコアを示しており、以下3点に絞って指摘しています：
1. `build_exclude_globset` の glob メタキャラクタ未エスケープ → **Test Experts により修正済み**
2. `scan_vault` が `auto_exclude_hidden` を無視 → **未修正（優先度 High）**
3. `backup_config` のタイムスタンプ衝突 → **未修正（優先度 Medium）**

---

## Low 指摘（スコアに影響なし）

- `init --yes` のヘルプにデータ取り扱いの注記が不十分（Compliance & Privacy）
- `chrono` 依存が単一のタイムスタンプ文字列生成のために追加されている（DX Advocate）
- ハードコードされた英語複数形ロジック（i18n）
- ハードコードされた英語フォールバックタイトル "Untitled"（i18n）
- Documentation 未更新（README, ref/cli.md, docs/CLI-USE.md）

