# PBI: VLM 抽出結果のキャッシュ

## ユーザーストーリー
コスト意识のあるユーザーとして、再インデックス時に VLM API 呼び出しがスキップされる仕組みがほしい、なぜなら同じ PDF の再インデックスで無駄な API 課金が発生するため

## ビジネス価値
- VLM API コストの大幅削減（再インデックス時の不要呼び出し防止）
- 大規模 Vault（数百 PDF）での運用コストの予測可能性向上

## 前提条件
- VLM ベース PDF 抽出機能が実装済みであること
- PDF バイナリの SHA-256 ハッシュが計算可能であること

## BDD 受け入れシナリオ

```gherkin
Scenario: 初回インデックス時に VLM 抽出が実行される
  Given PDF ファイル "scan.pdf" が未インデックス
  When `shiotsuchi chart` を実行する
  Then VLM API が 1 回呼び出される
  And 抽出結果が DB の `file_cache.vlm_hash` にキャッシュされる

Scenario: PDF バイナリ未変化時に VLM API がスキップされる
  Given "scan.pdf" がインデックス済み
  And VLM 抽出結果がキャッシュされている
  When `shiotsuchi chart` を再実行する
  Then VLM API が呼び出されない

Scenario: PDF バイナリ変化時に VLM 抽出が再実行される
  Given "scan.pdf" がインデックス済み
  And "scan.pdf" の内容が変更されている
  When `shiotsuchi chart` を実行する
  Then VLM API が 1 回呼び出される

Scenario: `shiotsuchi clean` で VLM キャッシュがクリアされる
  Given VLM 抽出結果がキャッシュされている
  When `shiotsuchi clean` を実行する
  Then 次回 `shiotsuchi chart` 時に VLM API が呼び出される
```

## 受け入れ基準
- [ ] VLM 抽出結果を `file_cache` テーブルにキャッシュ（`vlm_hash` カラム追加）
- [ ] PDF バイナリの SHA-256 ハッシュを計算し、キャッシュハッシュと比較
- [ ] ハッシュ一致時は VLM API 呼び出しをスキップ
- [ ] ハッシュ不一致時は再抽出を実行
- [ ] `shiotsuchi clean` でキャッシュをクリア

## テスト戦略（TDD レッド → グリーン → リファクタ）

### Unit Test（各シナリオに対応）
- `test_vlm_first_index_calls_api` — 初回は API 呼び出し
- `test_vlm_reindex_skips_unchanged` — 再インデックスでスキップ
- `test_vlm_reindex_calls_changed` — 内容変化時に再実行
- `test_vlm_cache_cleared_by_clean` — `shiotsuchi clean` でクリア

### Integration Test
- PDF インデックス → 再インデックス → VLM 呼び出し回数の検証（モック or ログ）

## 実装アプローチ

### DB スキーマ変更
```sql
ALTER TABLE file_cache ADD COLUMN vlm_hash TEXT;
```

### 処理フロー
```
PDF ファイル
    ↓
SHA-256 ハッシュ計算
    ↓
file_cache.vlm_hash と比較
    ├── ハッシュ一致 → キャッシュからテキスト取得
    └── ハッシュ不一致 → VLM API 呼び出し → 結果を DB に保存
```

## 見積もり
8 ポイント

## 技術的考慮事項

### DB マイグレーション
- v10 → v11 マイグレーションで `vlm_hash` カラムを追加
- 既存行の `vlm_hash` は `NULL`（次回インデックス時に計算）

### 既存コードとの連携
- `core/src/db.rs`: `file_cache` テーブルに `vlm_hash` カラム追加
- `core/src/indexer.rs`: VLM 抽出前にハッシュ比較を追加
- `core/src/db.rs`: `upsert_file_cache` に `vlm_hash` パラメータ追加

## Definition of Done
- [ ] `test_vlm_first_index_calls_api` がパスする
- [ ] `test_vlm_reindex_skips_unchanged` がパスする
- [ ] `test_vlm_reindex_calls_changed` がパスする
- [ ] `test_vlm_cache_cleared_by_clean` がパスする
- [ ] 全テストがパスする
