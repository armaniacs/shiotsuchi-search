# Checking Team レビューレポート

- 日時: 2026-05-27 05:30
- ブランチ: `improve-2026-05-25`
- 比較: `main`
- 変更: 80 files, +6973/-648 lines
- 22/22 エージェント完了

## 総合評価: 73.4/100 (ランク: B)

| Wave | エージェント | スコア | 指摘数 |
|------|------------|:------:|:------:|
| W1 | Red Team Leader | 90 | 3 |
| W1 | Blue Team Leader | 95 | 2 |
| W1 | System Architect | 90 | 4 |
| W1 | Maintainability Guardian | 80 | 8 |
| W1 | Legacy Bridge Architect | 33 | 4 |
| W2 | UI Expert | 90 | 2 |
| W2 | Tuning Expert | 85 | 3 |
| W2 | SRE/Ops Specialist | 40 | 6 |
| W2 | Domain Logic Expert | 75 | 4 |
| W2 | Compliance & Privacy Guard | 50 | 4 |
| W2 | i18n Expert | 75 | 5 |
| W2 | Accessibility Advocate | 70 | 3 |
| W2 | Documentation Architect | 60 | 7 |
| W2 | Data Integrity Expert | 70 | 4 |
| W2 | FinOps Consultant | 55 | 3 |
| W2 | Edge & Mobile Strategist | 85 | 3 |
| W2 | Refactoring Evangelist | 85 | 6 |
| W2 | Ethics & Bias Auditor | 85 | 8 |
| W2 | Supply Chain & Dependency Sentinel | 70 | 3 |
| W2 | API & Contract Negotiator | 90 | 2 |
| W2 | DX Advocate | 50 | 5 |
| W3 | Test Experts | 92 | 8 |

## 重要指摘事項（優先度順）

### [High] Hybrid モードでスコアブーストが逆方向に働く
- 指摘者: Domain Logic Expert
- 場所: `core/src/search.rs:261,274`
- 影響: Hybrid (RRF) は higher=better だが、score *= 0.3/0.5 のブーストが適用されると降格される
- 対処: Hybrid モードでは score /= 0.3（1.0超の乗数）で boost、または apply_filters_and_boost がモードを認識して分岐

### [High] reindex_file がタスクを削除せず古いタスクが残存
- 指摘者: Data Integrity Expert
- 場所: `core/src/db.rs:404-484`
- 影響: ファイル更新時に古い `- [ ]` タスクが DB に残り続ける
- 対処: `reindex_file` のトランザクション内で `DELETE FROM tasks WHERE vault_name = ?1 AND file_path = ?2` を追加

### [High] 色のみで検索一致箇所を伝達（アクセシビリティ）
- 指摘者: Accessibility Advocate
- 場所: `cli/src/commands/dive.rs:213-215`
- 影響: 赤緑色覚異常（人口の約8%）のユーザーがハイライトを認識不可
- 対処: 反転表示 + 色の併用、または記号マーカーの併用

### [High] データ保持・削除のライフサイクル管理不在
- 指摘者: Compliance & Privacy Guard
- 場所: `core/src/db.rs`（全般）
- 影響: 索引化データが無期限に保持される。GDPR/CCPA リスク
- 対処: retention_days 設定、purge_expired()、purge_all_user_data() API

### [High] Makefile test-all が clean を含みビルドキャッシュ破壊
- 指摘者: DX Advocate
- 場所: `Makefile:78`
- 影響: テストのたびに全依存クレート再コンパイル（数秒→5分以上）
- 対処: test-all から clean を削除、clean-all に分離

### [High] ref/cli.md の検索モード名が実装と乖離
- 指摘者: DX Advocate
- 場所: `ref/cli.md:15`
- 影響: keyword→fts、semantic→vec に名称変更したがドキュメント未追従
- 対処: ドキュメントを実装に合わせて更新

### [High] 機密データの分類・取り扱い機構なし
- 指摘者: Compliance & Privacy Guard
- 場所: `core/src/indexer.rs`, `mcp/src/handler.rs`
- 影響: MCP 経由で個人データがフィルタリングされずに露出
- 対処: 機密パターンベースのプレフィルタ、MCP マスキングモード

## コンフリクト調整結果

特になし。指摘は各専門領域に限定されており相互矛盾なし。

## 未完了エージェント

なし（22/22 完了）

## 市場投入への影響評価（SRE）

SRE/Ops Specialist がいくつかの観測容易性・運用面の課題を指摘。重要度は Low-Medium。

## 修正対応

Checking Team の指摘に基づき以下の修正を実施:

| # | 指摘 | 対応 |
|---|------|------|
| 1 | Hybrid mode score boost inverted | ✅ `apply_filters_and_boost` に `search_mode` パラメータ追加。Hybrid時は score/=0.3 で正方向boost。テスト更新。 |
| 2 | Color-only match highlighting | ✅ ANSI escape を `\x1b[1;31m` → `\x1b[1;7;31m`（bold+inverse+red）に変更。色覚異常者でも反転表示で認識可能。 |
| 3 | Makefile test-all clean | ✅ `test-all` から `clean` を削除し `clean-all` に分離。 |
| 4 | ref/cli.md mode name | ✅ `keyword`→`fts`、`semantic`→`vec` に修正。 |
| 5 | reindex_file task cleanup | ✅ 確認済み。実際には既に実装済みだった（line 434 `DELETE FROM tasks`）。 |

未対応（設計判断が必要）:
- データ保持ライフサイクル（GDPR/CCPA）— プロジェクトの性質上、ユーザー判断に委ねる領域
- 機密データ分類 — 同上
