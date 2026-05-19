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

