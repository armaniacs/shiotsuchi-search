# 00-INDEX.md

このファイルは、`.plan/` 以下に配置されたファイルがどのようなものなのかを記述する。

## ルール

1. **完了したファイルは `.plan/archived/` に移動する。**
   移動元にはファイルを残さない。移動元のディレクトリが空になった場合は削除してよい。
2. **移動は `git mv` を使う。**
   `.gitignore` で `.plan/` が除外されているが、`git mv` による rename は git が追跡する。
3. **一度アーカイブしたファイルは編集しない。**
   設計・計画・レビューはスナップショットとして保持する。修正が必要な場合は上書きせず新しいファイルを作る。
4. **INDEX にアーカイブ内容を記載する。**
   この `00-INDEX.md` に一覧を追加する。INDEX 自体は `.gitignore` のため git 追跡外だが、ローカル参照用として維持する。

## `.plan/archived/*.md` について

完了した実装計画・PBI・レビューレポート・デザインスペックは `.plan/archived/` に移動される。


## アーカイブ一覧

| ファイル | 内容 |
|---------|------|
| `2026-05-19-0400-review-main-v0.4.0.md` | v0.4.0 リリースレビュー（22指摘、19件修正・3件deferred） |
| `2026-05-19-1901-review-v0.4.x.md` | v0.4.x リリースレビュー（全指摘対応済み、v0.4.1 リリース済み） |
| `dig-findings-2026-05-16-dependency-upgrade.md` | 依存関係アップグレード計画の深掘り評価（8仮定検証・リスク評価） |
| `2026-04-29-shiotsuchi-search-design.md` | コア設計仕様書（DB/Tokenizer/Indexer/Search/Watcher 設計） |
| `2026-05-19-post-v0.4.0-improvements.md` | v0.4.0 リリース後改善提案（clean command/進捗最適化等） |
| `2026-05-19-v0.4.x-improvements.md` | v0.4.x リリース後改善提案（clean/rebuild/multi-vault 設計） |
| `2026-05-31-29-feat-intuitive-command-aliases.md` | PBI-29: CLI コマンドに標準名エイリアス追加（v0.4.13） |
| `2026-05-31-30-feat-interactive-welcome.md` | PBI-30: Interactive welcome screen + onboarding wizard（v0.4.14） |
| `2026-05-31-31-fix-onboarding-config-exists.md` | PBI-31: Search→onboarding の config_exists ハードコード修正 |
| `2026-05-31-32-fix-welcome-no-color-support.md` | PBI-32: ウェルカムメニュー NO_COLOR 対応 |
| `2026-05-31-33-fix-non-tty-command-list.md` | PBI-33: 非TTY時にコマンド一覧表示 |
| `2026-05-31-34-fix-search-query-max-length.md` | PBI-34: 検索クエリ200文字バリデーション |
| `2026-05-31-35-fix-onboarding-completion-box-width.md` | PBI-35: 完了画面ボックス幅動的計算 |
| `2026-05-31-36-refactor-messages-to-constants.md` | PBI-36: 文字列を messages.rs に定数抽出 |
| `2026-05-31-0923-review-interactive-welcome.md` | Checking Team v1 レビューレポート（PBI-30） |
| `2026-05-31-1125-review-pbi31-36.md` | Checking Team v2 レビューレポート（PBI-31〜36） |
| `2026-04-29-shiotsuchi-search-phase1-core.md` | Phase1 コアライブラリ実装計画（Tasks 1-9完了・TDD全遵守） |
| `2026-04-29-shiotsuchi-search-phase2-cli.md` | Phase2 CLI実装計画（Tasks 1-7完了・TDD全遵守） |
| `2026-04-29-shiotsuchi-search-phase4-mcp.md` | Phase4 MCPサーバー実装計画（Tasks 1-5完了） |
| `2026-04-29-shiotsuchi-search-phase5-polish.md` | Phase5 ポリッシュ実装計画（Tasks 1-5完了） |
| `2026-05-04-post-review-phase2-fixes.md` | Phase2レビュー後包括的修正計画（全8フェーズ完了・v0.2.0リリース） |
| `2026-05-12-rag-core.md` | RAGコア実装計画（全10タスク完了・チャンク/埋め込み/検索） |
| `2026-05-12-rag-mcp.md` | RAG MCP更新実装計画（全4タスク完了・ツール4本リネーム） |
| `2026-05-16-dependency-upgrade-deferred.md` | 依存関係アップグレードdeferred計画（全タスク完了） |
| `2026-05-17-close-coverage-gaps.md` | カバレッジギャップ閉鎖計画（全9タスク完了・214 tests pass） |
| `2026-05-17-coverage-improvement-phase2.md` | カバレッジ改善Phase2計画（全9タスク完了・268 tests pass） |
| `2026-05-12-rag-implementation-pre-analysis.md` | RAG実装前分析（設計判断・リスク一覧・実装依存順序） |
| `2026-05-18-clean-command.md` | clean コマンド実装計画（全5ステップ完了・アトミックリネーム版で実装） |
| `2026-05-18-clean-command-design.md` | clean コマンド設計仕様（バックアップ→再インデックス、Draft→Implemented） |
| `2026-05-18-multi-vault-support.md` | マルチボールト実装計画（全10タスク完了・core/CLI/MCP全層実装済み） |
| `2026-05-18-multi-vault-support-design.md` | マルチボールト設計仕様（config/DB/検索/監視/移行、Draft→Implemented） |
| `2026-05-16-cli-build-info.md` | CLI ビルド時情報表示 実装計画（TDD・全6タスク完了） |
| `2026-05-16-cli-build-info-design.md` | CLI ビルド時情報表示 設計仕様（help/version/support --json） |
| `2026-04-29-shiotsuchi-search-phase3-skill.md` | Skillサーバー実装計画（廃棄: skill/ クレート削除済み） |
| `2026-05-16-dependency-upgrade.md` | 依存関係アップグレード計画（全5タスク完了・rusqlite 0.31→0.39） |
| `2026-05-17-coverage-improvement-phase3.md` | カバレッジ改善Phase3（大半は先行作業で完了、1 test追加） |
| `2026-05-01-shiotsuchi-search-phase6-vitest-integration.md` | Vitest MCP統合テスト計画（全タスク完了・自動セットアップ付き） |
| `2026-05-01-shiotsuchi-search-phase7-mcp-inspector.md` | MCP Inspector 手動テスト（アーカイブ: ツール名古い・Phase6で代替済み・意味なし） |
| `2026-05-03-0855-review-phase2.md` | Phase2 Checking Team レビューレポート (76/100) — 全指摘対応済み |
| `2026-05-06-0827-review-fix-codebase.md` | コードベース全体レビュー (87/100, v0.2.2) — 全指摘対応済み |
| `2026-05-07-0000-review-init-feature.md` | `init` 機能レビューレポート — v0.2.9 で全指摘修正済み |
| `plan-checking-team-2026-05-09a.md` | modify-2026-05-09a レビュー (74/100) — 全指摘対応済み |
| `plan-h2-init-fix-remaining.md` | `init` 残課題修正計画 — 全タスク完了 (v0.2.9 TDD) |
| `plan-h2-init-future.md` | `init` Future Work 計画 — 全機能実装済み (v0.2.8~v0.2.9) |
| `plan-h2-init.md` | `init` 拡張実装計画 — auto-exclude/scan/backup 全て v0.2.8 で実装完了 |
| `plan-h5-dive-format.md` | `dive`  Human-Readable 出力計画 — 2026-05-08 実装済み |
| `plan-modify-2026-05-09a.md` | modify-2026-05-09a 修正計画 — 全タスク完了 |
| `plan-next-actions-2026-05-10.md` | レビュー後アクション計画 — 全アイテム完了 |
| `plan-h3-db-migration.md` | DB スキーママイグレーション戦略 — アドホック実装で必要十分。YAGNI 判断正解 |
| `plan-h4-observability.md` | 構造化オブザーバビリティ計画 — `doctor` コマンドのみ実装。tracing/metrics は過剰と判断し deferred |
| `plan-h6-RAG.md` | RAG 拡張設計仕様書 — Level 1+2 チャンク/ベクトル検索/RRF/Embedding/差分更新 全て実装済み (~85%) |
| `plan-h7-MCP.md` | MCP サーバー拡張設計仕様書 — 4 ツール/セキュリティ対策/出力フォーマット 全て実装済み (~90%) |
| `2026-05-20-1726-review-feat-min-size.md` | feat-min-size Checking Team レビュー (83/100, 15指摘中12修正) — feat-min-size は main にマージ済み |
| `2026-05-21-0528-dig-findings.md` | feat-min-size レビューの深掘りセッション (7決定事項、全て実行済み) |
| `2026-05-25-01-fix-mtime-size-two-stage-scan.md` | PBI-01: mtime + size 二段階スキャン — 既存実装確認済み |
| `2026-05-25-02-fix-semantic-optional-feature-flag.md` | PBI-02: Semantic 検索を Cargo feature flag でオプション化 |
| `2026-05-25-03-fix-multi-vault-native-support.md` | PBI-03: マルチ Vault ネイティブ対応 — 既存実装確認＋vault_default 追加 |
| `2026-05-25-04-feat-frontmatter-tag-date-filter.md` | PBI-04: Frontmatter タグ・日付フィルタリング — 既存実装確認済み |
| `2026-05-25-05-backlog-i18n-japanese-cli-messages.md` | PBI-05: CLI メッセージ日本語 i18n — 全コマンド日本語化完了 |
| `2026-05-25-06-backlog-db-secure-app-data-dir.md` | PBI-06: DB セキュアアプリデータ領域 — 既存実装確認済み |
| `2026-05-25-07-backlog-custom-user-dictionary.md` | PBI-07: カスタムユーザー辞書 — 既存実装確認済み |
| `2026-05-25-08-backlog-synonym-thesaurus-mapping.md` | PBI-08: 同義語シソーラスマッピング — 既存実装確認済み |
| `2026-05-25-09-backlog-fuzzy-search.md` | PBI-09: あいまい検索 — 既存実装確認済み |
| `2026-05-25-10-backlog-hybrid-search-alpha-tuning.md` | PBI-10: ハイブリッド検索 Alpha 値 — 既存実装確認済み |
| `2026-05-25-11-backlog-mmr-diversity-reranking.md` | PBI-11: MMR 多様化リランキング — 実装＋レビュー修正完了 |
| `2026-05-25-13-backlog-pluggable-embedding-model.md` | PBI-13: 埋め込みモデル差し替え（API 方式）— ✅ v0.4.12 完了 |
| `2026-05-25-28-backlog-synonym-cli-manager.md` | PBI-28: 同義語管理 CLI と専用ファイル対応 — 実装完了 |
| `2026-05-25-21-backlog-ocr-pdf-image-search.md` | PBI-21: PDF テキスト抽出検索 Phase A — pdfium-auto + XY-Cut 完了、画像 OCR は PBI-28 へ移動 |
| `2026-05-25-18-backlog-backlink-pagerank-scoring.md` | PBI-18: Backlink / PageRank スコアリング — v0.4.15 完了 |
| `2026-06-04-49-backlog-mcp-calltool-split.md` | PBI-49: MCP call_tool ツール別分割 — コード上で既に実装完了 |
| `2026-06-04-55-backlog-exclude-patterns-compat.md` | PBI-55: exclude_patterns → exclude_dirs 後方互換性 — 明示的拒否方式で解決済み |
| `2026-06-04-56-backlog-mcp-search-compat.md` | PBI-56: search() → SearchRequest 移行に伴うMCP互換性 — 内部移行のみ、外部互換レイヤー不要 |
| `2026-06-06-57-backlog-mcp-rate-limiter-all-endpoints.md` | PBI-57: MCP 全エンドポイントにレート制限追加 — GENERAL 50 req/s + REBUILD 1 req/s |
| `2026-06-06-58-backlog-mcp-sensitive-config-safe-default.md` | PBI-58: MCP 機密データマスキングのデフォルト有効化 — detection: true, Option 除去 |
| `2026-06-06-53a-backlog-tracing-mcp.md` | PBI-53a: MCP サーバーの構造化ログ導入（tracing-subscriber + stderr 固定） |
| `2026-06-06-53b-backlog-tracing-http.md` | PBI-53b: HTTP サーバーへの TraceLayer + リクエストID 導入 |
| `2026-06-06-53c-backlog-tracing-core.md` | PBI-53c: core ライブラリの log:: → tracing:: 移行 + #[instrument] |
| `2026-06-06-53d-backlog-tracing-cli.md` | PBI-53d: CLI の env_logger → tracing-subscriber 移行 |

