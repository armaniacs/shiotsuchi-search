# FinOps Consultant レビュー結果

## スコア: 95/100

## 指摘事項

### [Medium] ファイル監視（watcher）でのリネーム処理の網羅性不足によるDB肥大化リスク
- 場所: core/src/watcher.rs:75-88
- 影響: `RenameMode::Both` のみをハンドリングしており、Linux（inotify）などで発生する `RenameMode::From` / `RenameMode::To` の個別イベントが処理されない。これによりファイルリネーム時に旧パスのインデックスが削除されず、新パスのみが追加される。ユーザーが頻繁にファイル名を変更する運用では、SQLite DB に幽霊エントリが蓄積し、ストレージ容量が非限定的に増大する。また、削除済み/リネーム済みの古いパスが検索結果に表示される可能性がある。
- 対処: `EventKind::Modify(ModifyKind::Name(RenameMode::From))` で旧パスを `delete_note` し、`RenameMode::To` で新パスを `index_file` する分岐を追加する。cookie による From/To のペアリングが可能な場合は活用して整合性を高める。

## 確認済みの良好点
- **外部APIへの依存なし**: 全オペレーションがローカル完結（SQLite + vaporetto ローカルトークナイザ）。AI API、Obsidian REST API、クラウドストレージ等への課金コストが一切発生しない。
- **トークナイザーのキャッシュ化**: `core/src/tokenizer.rs` で `OnceLock` を用いたグローバルキャッシュ `get_tokenizer()` を実装しており、モデル初期化コストを同一プロセス内で1回のみに抑制している。
- **MCP 検索の上限制御**: `mcp/src/handler.rs` で `search` の `limit` を 20 に固定しており、無制限なリソース消費を防止している。
- **変更検知によるスキップ**: `db.rs` の `upsert_note` でハッシュ比較を行い、未変更ファイルの無駄な再インデックスを防止している。
