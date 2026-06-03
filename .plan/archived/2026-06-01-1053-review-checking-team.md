# Checking Team レビューレポート

> 実施日: 2026-06-01 10:53
> ブランチ: `improve-2026-05-25`
> 比較対象: `origin/main`
> レビュアー: 22名中22名完了

## 総合評価: 84/100 (ランク: A)

**スコア分布:**
| レンジ | エージェント |
|--------|-------------|
| 100 | FinOps Consultant |
| 90-99 | Test Experts, UI Expert, Tuning Expert, i18n Expert, Accessibility Advocate, Compliance & Privacy, Edge & Mobile Strategist, Ethics & Bias Auditor, Refactoring Evangelist, API & Contract Negotiator, DX Advocate, Legacy Bridge Architect |
| 80-89 | Maintainability Guardian |
| 70-79 | Red Team Leader, Blue Team Leader, System Architect, SRE/Ops Specialist, Domain Logic Expert, Data Integrity Expert, Documentation Architect, Supply Chain Sentinel |
| 60-69 | — |
| 0-59 | — |

---

## 重要指摘事項（優先度順）

### [High] Watcher Remove/Rename イベントで tag_counts が減算されない

- **指摘者**: System Architect, SRE/Ops Specialist, Data Integrity Expert（3名一致）
- **場所**: `core/src/watcher.rs:186,223`、`cli/src/commands/delete.rs:55`
- **影響**: ファイルウォッチャーによる削除検知・リネーム・CLI `delete` コマンド実行時、`delete_chunks_for_file()` が呼ばれるが `tag_counts` のデクリメントが行われない。`cleanup_deleted` のみが正しく処理するため、通常運用で削除が発生するたび `tag_counts` にゴミが蓄積し、`tag_stats()` が過大な値を返すようになる。経年劣化でタグ統計が実態と乖離する。
- **対処**: watcher の Remove/Rename ハンドラと `cli/src/commands/delete.rs` で、`delete_chunks_for_file()` の前に `get_tags_for_file()` → `decrement_tag_count()` を追加する。`cleanup_deleted` のパターンに倣う。

### [High] MCP `get_surrounding_context` に vault スコープの認可チェックがない

- **指摘者**: Blue Team Leader
- **場所**: `mcp/src/handler.rs:153-184`
- **影響**: `chunk_id` のみを指定すると、所属 vault に関わらず周辺チャンクを取得可能。異なる vault の chunk_id を推測できれば、本来アクセスすべきでないコンテンツを取得できる。
- **対処**: チャンクの `vault_name` を DB から取得し、リクエストされた `vaults` リストに含まれているか確認する。

### [High] pdfium-render のバージョン重複 (v0.8.37 + v0.9.1)

- **指摘者**: Supply Chain Sentinel
- **場所**: `core/Cargo.toml:40`, `Cargo.lock`
- **影響**: edgequake-pdf2md と core の `pdf` feature が異なる semver-incompatible な pdfium-render バージョンに依存。バイナリサイズ増加・ビルド時間悪化。
- **対処**: バージョンを統一する。edgequake-pdf2md 側の対応を待つか、core 側で 0.8 系に合わせる。

### [Medium] reindex_file のタグ減算に `count > 0` ガードがない（Test Experts が修正済み）

- **指摘者**: Domain Logic Expert
- **場所**: `core/src/db.rs:517`
- **修正**: ✅ Test Experts が `AND count > 0` を追加。`decrement_tag_count` メソッドと一貫。
- **テスト**: `test_tag_count_decrement_with_count_zero_guard` 追加済み。

### [Medium] char_count がバイト数（Unicode 文字数ではない）（Test Experts が修正済み）

- **指摘者**: Domain Logic Expert, Tuning Expert（2名一致）
- **場所**: `core/src/db.rs:598`
- **修正**: ✅ Test Experts が `content.len() as i64` → `content.chars().count() as i64` に変更。
- **テスト**: `test_char_count_is_unicode_chars_not_bytes` 追加済み。

### [Medium] cleanup_deleted のタグデクリメントエラーを黙殺（Test Experts が修正済み）

- **指摘者**: SRE/Ops Specialist, Domain Logic Expert（2名一致）
- **場所**: `core/src/indexer.rs:379`
- **修正**: ✅ Test Experts が `let _ =` → `if let Err(e) = ... { log::warn!(...) }` に変更。

### [Medium] Migration v10 に char_count / tag_counts のバックフィルがない

- **指摘者**: SRE/Ops Specialist
- **場所**: `core/src/db.rs` (migration v10 block)
- **影響**: v10 マイグレーション実行後、既存ファイルの `char_count` が 0、`tag_counts` が空のまま。フル再インデックスまで stats() が誤った値を返す。
- **対処**: マイグレーション末尾に既存 chunks からのバックフィル UPDATE/INSERT を追加する。

### [Medium] タグのカンマ区切りシリアライズによるタグ断片化リスク

- **指摘者**: Red Team Leader
- **場所**: `core/src/chunker.rs:37,77,94,113` → `core/src/db.rs:513,585`
- **影響**: タグ値にカンマが含まれると `split(',')` で断片化。タグ集計が不正確になる。
- **対処**: タグにカンマが含まれる場合は警告ログを出力するか、エスケープ処理を追加する。

### [Medium] MCP エラーメッセージが内部パスを漏洩する

- **指摘者**: Blue Team Leader
- **場所**: `mcp/src/handler.rs:134-136,158`
- **影響**: `canonicalize()` エラー時に絶対パスがエラー文字列に含まれ、MCP クライアントに露出する可能性。
- **対処**: `map_err` でパス情報を除去した一般化エラーに変換する。

### [Medium] edgequake-* クレートのメンテナンス実績不足

- **指摘者**: Supply Chain Sentinel
- **場所**: `Cargo.lock`
- **影響**: edgequake-pdf2md / edgequake-llm は比較的新しいパッケージで監査実績が不足。
- **対処**: パッチバージョン固定、`cargo-deny` / `cargo-audit` の CI 導入。

### [Medium] tag_counts の zero-count 行が削除されず蓄積される

- **指摘者**: Tuning Expert
- **場所**: `core/src/db.rs` (tag_counts maintenance)
- **影響**: `count=0` になった行が残り続ける。長期的にはテーブルに死んだ行が蓄積。
- **対処**: `decrement_tag_count` 内またはバッチ後に `DELETE FROM tag_counts WHERE count = 0` を実行。

### [Medium] 一部のマイグレーションがトランザクションでラップされていない

- **指摘者**: System Architect
- **場所**: `core/src/db.rs:119-337`
- **影響**: 複数操作を含むマイグレーションブロック（v8→v9, v9→v10 等）がトランザクション外。クラッシュ時スキーマ不整合のリスク。
- **対処**: 複数操作を含む全ブロックを `BEGIN TRANSACTION` / `COMMIT` でラップ。

### [Medium] `create_schema` と最終スキーマの不一致

- **指摘者**: System Architect
- **場所**: `core/src/db.rs:340-374`
- **影響**: `create_schema` が古い v2 スキーマを生成。fresh DB が全マイグレーションを通過することで成立しているテクニカルデット。
- **対処**: `create_schema` を最終スキーマを直接生成するよう更新する。

---

## コンフリクト調整結果

- **char_count の単位（バイト vs 文字数）**: Domain Logic Expert → chars() 推奨 / Tuning Expert → 同調。Test Experts が chars() に修正済み。**System Architect の判断**: chars() が正しい。✅
- **cleanup_deleted のエラー処理**: 3名が異なる観点から指摘。SRE/Ops は `log::warn!` への変更、Domain Logic はトランザクション化、Data Integrity は delete_chunks_for_file 内への一元化。Test Experts が `log::warn!` 変更のみ実施。**System Architect の判断**: トランザクション化は将来課題、ログ出力は即時対応で十分。✅
- **tag 区切り文字問題**: Red Team のみが指摘。他エージェントからの確認/反論なし。独立した懸念として Medium で維持。

---

## 未完了エージェント

なし（22名全員完了）

---

## Test Experts による修正内容

Test Experts が以下の 3 件を自動修正した:

1. **`reindex_file` のタグデクリメントに `AND count > 0` ガード追加** (`core/src/db.rs:517`)
2. **`char_count` を UTF-8 バイト数→ Unicode 文字数に修正** (`core/src/db.rs:598`)
3. **`cleanup_deleted` のタグデクリメントエラーをログ出力に変更** (`core/src/indexer.rs:379`)

加えて 9 件のテストを追加・確認済み。
