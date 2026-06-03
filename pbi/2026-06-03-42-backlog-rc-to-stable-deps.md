# PBI: RC プリリリース依存の stable 移行

## ユーザーストーリー
開発者として、依存パッケージが stable リリースであることがほしい、なぜなら RC バージョンはセキュリティパッチの供給が不安定で、API の破壊的変更が RC 間で発生する可能性があるため

## ビジネス価値
- セキュリティパッチの安定供給
- ビルドの再現性向上
- 供給チェーン攻撃のリスク低減

## 前提条件
- なし

## BDD 受け入れシナリオ

```gherkin
Scenario: ort の stable バージョンでビルドが通る
  When `cargo build --features semantic` を実行する
  Then ビルドが成功する
  And ort のバージョンが stable（rc でない）である

Scenario: notify の stable バージョンでファイル監視が動作する
  Given `shiotsuchi watch` が起動している
  When ファイルを作成する
  Then そのファイルがインデックスされる

Scenario: 全テストがパスする
  When `make test` を実行する
  Then 全テストがパスする
```

## 受け入れ基準
- [ ] `ort` を stable バージョンに更新し、ビルドが通る
- [ ] `notify` を stable バージョンに更新し、ファイル監視が動作する
- [ ] `edgequake-pdf2md` の推移的依存を確認し、必要に応じて更新
- [ ] 全テストがパスする

## テスト戦略（TDD レッド → グリーン → リファクタ）

### Integration Test（各シナリオに対応）
- `test_build_with_stable_ort` — ort stable でビルド成功
- `test_watch_with_stable_notify` — notify stable でファイル監視動作
- `test_all_tests_pass` — 全テストパス

## 実装アプローチ

### 更新手順
1. `cargo update -p ort` で最新 stable を取得
2. `cargo update -p notify` で最新 stable を取得
3. `core/Cargo.toml` のバージョン指定を更新
4. `cargo build --all-features` でビルド確認
5. `make test` で全テスト実行

### 注意点
- `ort` は `download-binaries` フィーチャーが有効なため、バイナリ互換性を確認
- `notify` は `RecommendedWatcher` API の変更に注意

## 見積もり
3 ポイント

## 技術的考慮事項

### リスク
- `ort` の stable リリースは ONNX Runtime 2.0 対応が前提
- `notify` の stable リリースは v9 API 変更を含む可能性

### 既存コードとの連携
- `core/Cargo.toml`: バージョン指定更新
- `Cargo.lock`: 依存関係更新
- `core/src/watcher.rs`: notify API 変更への対応（必要な場合）

## Definition of Done
- [ ] `test_build_with_stable_ort` がパスする
- [ ] `test_watch_with_stable_notify` がパスする
- [ ] `test_all_tests_pass` がパスする
- [ ] ort が stable バージョンである
- [ ] notify が stable バージョンである
