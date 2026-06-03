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
Scenario: PDF 内容未変化時は VLM 抽出をスキップ
  Given PDF ファイルがインデックス済み
  And VLM 抽出結果が DB にキャッシュされている
  When `shiotsuchi chart` を再実行する
  Then VLM API が呼び出されない
  And 既存のキャッシュされたテキストが使用される

Scenario: PDF 内容変化時は VLM 抽出を再実行
  Given PDF ファイルがインデックス済み
  And PDF ファイルが変更されている
  When `shiotsuchi chart` を実行する
  Then VLM API が呼び出される
  And 新しいテキストが DB に保存される
```

## 受け入れ基準
- [ ] VLM 抽出結果を `file_cache` テーブルにキャッシュ（`vlm_hash` カラム追加）
- [ ] PDF バイナリの SHA-256 ハッシュを計算し、キャッシュハッシュと比較
- [ ] ハッシュ一致時は VLM API 呼び出しをスキップ
- [ ] ハッシュ不一致時は再抽出を実行
- [ ] `shiotsuchi clean` でキャッシュをクリア

## テスト戦略（t_wada スタイル）

### Unit Test
- ハッシュ比較ロジックのテスト
- キャッシュヒット/ミスの分岐テスト

### Integration Test
- PDF インデックス → 再インデックス → VLM 呼び出し回数の検証

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
- [ ] PDF 内容未変化時に VLM API がスキップされる
- [ ] PDF 内容変化時に再抽出が実行される
- [ ] テストがパスする
