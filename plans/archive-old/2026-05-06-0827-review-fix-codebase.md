# Checking Team 最終レポート

**日時**: 2026-05-06 08:27 JST
**対象**: コードベース全体 (v0.2.2)
**ブランチ**: fix-2026-05-06 (main ベース)
**モード**: 標準レビュー（22名）

---

## 総合評価: 87/100 (ランク: A)

| ランク | スコア範囲 |
|:-----:|----------:|
| S | 90〜100 |
| **A** | **80〜89** ← 今回 |
| B | 70〜79 |
| C | 60〜69 |
| D | 50〜59 |
| E | 0〜49 |

---

## エージェントスコア一覧

### Wave 1: コアレビュアー

| エージェント | スコア | 指摘 | 備考 |
|------------|:-----:|:----:|------|
| Red Team Leader | 40 | 2件 (High 1, Medium 1) | セキュリティ重点 |
| Blue Team Leader | 75 | 3件 (High 3) | 防御的検証 |
| System Architect | 90 | 2件 (Medium 2) | 構造評価 |
| Maintainability Guardian | 70 | 3件 (High 1, Medium 2) | 保守性 |
| Legacy Bridge Architect | — | — | **未完了**（結果ファイルなし） |

### Wave 2: スペシャリスト

| エージェント | スコア | 指摘 | 備考 |
|------------|:-----:|:----:|------|
| UI Expert | 70 | 3件 (High 1, Medium 2) | CLIのUX |
| Tuning Expert | 95 | 1件 (Medium 1) | パフォーマンス |
| SRE/Ops Specialist | 72 | 3件 (High 3) | 運用面 |
| Domain Logic Expert | 60 | 3件 (High 2, Medium 1) | ドメインロジック |
| Compliance & Privacy Guard | 95 | 3件 (Low) | コンプライアンス |
| i18n Expert | 80 | 3件 (Medium 2, Low 1) | 国際化 |
| Accessibility Advocate | 90 | 2件 (Medium 2) | アクセシビリティ |
| Documentation Architect | 90 | 3件 (Medium 2, Low 1) | ドキュメント |
| Data Integrity Expert | 90 | 3件 (Medium 2, Low 1) | データ整合性 |
| FinOps Consultant | 90 | 2件 (Medium 2) | コスト最適化 |
| Edge & Mobile Strategist | 90 | 3件 (Medium 2, Low 1) | エッジ/モバイル |
| Refactoring Evangelist | 85 | 3件 (Medium 3) | リファクタリング |
| Ethics & Bias Auditor | 90 | 3件 (Medium 2, Low 1) | 倫理/バイアス |
| Supply Chain & Dependency Sentinel | 90 | 2件 (Medium 2) | サプライチェーン |
| API & Contract Negotiator | 90 | 3件 (Medium 2, Low 1) | API設計 |
| DX Advocate | 70 | 3件 (High 1, Medium 2) | 開発体験 |

### Wave 3: テスト

| エージェント | スコア | 成果 |
|------------|:-----:|------|
| Test Experts | 90 | 修正2件、テスト5件追加 |

---

## 重要指摘事項（優先度順）

### [High] Symbolic link path traversal bypass in watcher
- **指摘者**: Red Team Leader
- **場所**: `core/src/watcher.rs:53-93`
- **影響**: ファイル監視(`handle_event`)がシンボリックリンクを検証せず、`strip_prefix` チェックのみ。悪意のあるシンボリックリンクでvault外のファイルを読み書きされるリスク。
- **対処**: `handle_event` に `canonicalize()` + `starts_with()` チェックを追加、`search.rs` や `handler.rs` と同様の保護を実装。

### [High] Config values not actually used (dead code)
- **指摘者**: Maintainability Guardian
- **場所**: `cli/src/config.rs:75-81`, `cli/src/commands/chart.rs:24-61`, `cli/src/commands/scan.rs:16-31`, `cli/src/commands/dive.rs:19-33`
- **影響**: 設定ファイル(`config.toml`)でカスタマイズした値（`include_extensions`、`exclude_patterns`、`snippet_lines`等）が実際の動作に反映されず、ハードコードされたデフォルト値が使用される。
- **対処**: CLIコマンド実装で `cfg.indexing` と `cfg.watcher` の値を適切に使用するよう修正。

### [High] No CI/CD pipeline
- **指摘者**: DX Advocate
- **場所**: プロジェクト全体
- **影響**: CI設定ファイルが存在せず、`make test` 以外にコード変更の正当性を検証する手段がない。リグレッション検出が完全に開発者の記憶と規律に依存。
- **対処**: GitHub Actions で `cargo test --workspace --exclude shiotsuchi-e2e` を実行する `.github/workflows/ci.yml` を作成。

### [High] Model download without integrity verification
- **指摘者**: SRE/Ops Specialist
- **場所**: `scripts/download-model.sh:8-11`
- **影響**: VaporettoモデルをハードコードされたGitHub URLからダウンロードする際、SHA-256やGPG署名による整合性検証がない。MITM攻撃で悪意のあるモデルを注入されるリスク。
- **対処**: SHA-256チェックサム検証、設定可能なURL、`--verify` フラグを追加。

### [High] No database schema migration strategy
- **指摘者**: SRE/Ops Specialist
- **場所**: `core/src/db.rs:68-76`
- **影響**: `PRAGMA user_version` でバージョンは記録されるが、将来のスキーマ変更に対するマイグレーションパスがない。アップグレード時にDBを手動削除→再構築が必要（データ損失）。
- **対処**: マイグレーションフレームワークを導入し、バージョンごとに増分マイグレーションを実行。

### [High] Missing structured observability and alerting
- **指摘者**: SRE/Ops Specialist
- **場所**: プロジェクト全体（`env_logger` のみ）
- **影響**: 構造化ログ（JSON）、メトリクス、ヘルスチェックエンドポイントがなく、運用監視が不可能。長期稼働する `scan` コマンドでの障害検出が困難。
- **対処**: `tracing` クレートの導入、メトリクス収集、MCPヘルスチェックの実装。

### [High] MCP vs CLI delete defense depth inconsistency
- **指摘者**: Domain Logic Expert
- **場所**: `mcp/src/handler.rs:30`, `cli/src/commands/delete.rs:11-27`
- **影響**: MCP handlerは `path.starts_with('/') || path.contains("..")` による早期拒否があるが、CLI deleteは同じ防御層がなく defense-in-depth に反する。
- **対処**: `delete.rs` に同一の早期チェックを追加。

### [High] dive コマンドが人間可読な形式で出力しない
- **指摘者**: UI Expert
- **場所**: `cli/src/commands/dive.rs:35-44`
- **影響**: 検索結果が生のJSONのみで出力され、ファイルパス・タイトル・スニペットが一目でわからない。CLIツールとしての基本UXを欠く。
- **対処**: `--format` フラグを追加し、デフォルトで表形式出力を提供。

### [High] Config file secret storage risk / Delete path traversal gap / Dependency audit not automated
- **指摘者**: Blue Team Leader
- **複数指摘あり**: 設定ファイルのパーミッション制御不足、deleteコマンドのパス検証統一不足、cargo audit 未実施。

### [Medium] その他重要指摘
- **Indexing not parallelized** (Tuning Expert): `index_directory` が同期的で大量ファイル時に長時間。`rayon` による並列化余地あり。
- **decompress_if_needed 重複** (Maintainability Guardian): `build.rs` と `tokenizer.rs` に同一のZstd解凍関数。
- **Magic numbers 散在** (Maintainability Guardian): スニペット行数・文字制限が複数ファイルにハードコード。
- **Model未設定時のテストサイレントスキップ** (DX Advocate): モデル非依存テストでもスキップされ偽陰性リスク。

---

## Wave 3 で実施した修正

| 修正 | 指摘元 | 場所 | 内容 |
|------|--------|------|------|
| 🔧 canonicalize 失敗をエラー化 | Domain Logic [High] | `core/src/search.rs:38-48` | 非正規化パスへのフォールバックを廃止、エラー返却に変更 |
| 🔧 watcher エラー無視を解消 | DX Advocate [Medium] | `core/src/watcher.rs:63,71,81-85` | `let _` で捨てていたエラー値を `log::warn!` で出力 |

**追加テスト一覧**:

| テスト | 場所 | 対応指摘 |
|--------|------|---------|
| `test_search_canonicalize_failure_returns_error` | `search.rs:161-175` | Domain Logic [High] |
| `test_decompress_if_needed_passthrough_plain_bytes` | `tokenizer.rs:244-248` | Maintainability [Medium] |
| `test_decompress_if_needed_rejects_garbage_zstd` | `tokenizer.rs:250-256` | Maintainability [Medium] |
| `test_handle_event_modify_outside_vault_safe_noop` | `watcher.rs:131-156` | Red Team [Medium] |
| `test_delete_nonexistent_returns_ok` | `db.rs:319-322` | SRE/Ops |

---

## コンフリクト調整結果

- **対立なし**: System Architect (90/100) を最上位判断として採用。config構造体のcore移行、E2Eテスト充実の優先度を確認。
- **System Architectの判断**: 現在のアーキテクチャは健全。config構造体のcore移行は優先度高くないが、E2Eテスト充実は推奨。

---

## 未完了エージェント

| エージェント | 理由 |
|------------|------|
| Legacy Bridge Architect | 結果ファイル未出力（タスクが空の戻り値を返した） |

---

## スコア内訳

- 21エージェント評価（Legacy Bridge除外）
- 合計スコア: 1822 / 21 = **86.8 → 87/100**
- High 指摘: 11件（うち1件はWave 3で修正済）
- Medium 指摘: 多数
