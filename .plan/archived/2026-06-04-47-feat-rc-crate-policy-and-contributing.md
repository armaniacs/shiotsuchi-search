# PBI-47: RC クレートアップグレード方針の文書化と CONTRIBUTING.md 作成

## ユーザーストーリー
開発者として、RC バージョンの依存クレートをいつ stable にアップグレードすべきかの判断基準がほしい、また VLM 機能を有効化する際のセキュリティレビュー手順が文書化されていることを保証したい、なぜなら暗黙的な判断基準に依存すると upgrade のタイミングを逃したり、セキュリティレビュー漏れが発生するから

## ビジネス価値
- 依存関係アップグレードの判断基準明確化
- VLM feature 有効化時のセキュリティレビュー手順の標準化
- 新規コントリビューターの参入障壁低減
- プロジェクトの持続可能性向上

## 発端
2件の Checking Team レビューからの積み残し:

1. **RC クレート理由書**（`plans/2026-06-01-2146-review-checking-team-2.md` Medium）: `notify 9.0.0-rc.4` と `ort 2.0.0-rc.12` を使っている理由と stable アップグレード条件が文書化されていない。Cargo.toml にコメントは追加済みだが、方針文書は存在しない。
2. **VLM セキュリティレビュー手順**（`plans/2026-06-01-1053-review-checking-team.md` + 2回目 Medium）: VLM を有効化する際のセキュリティレビュー手順を CONTRIBUTING.md に明文化することが未実施。

## 前提条件
- なし（ドキュメントのみの PBI）

## BDD 受け入れシナリオ

```gherkin
Scenario: RC クレートの方針が文書化されている
  Given `docs/RC-CRATE-POLICY.md` が存在する
  When そのドキュメントを読む
  Then 使用中の RC クレート一覧が記載されている
  And 各クレートの stable アップグレード条件が記載されている
  And アップグレードのトリガーとなるイベント（stable release, 脆弱性報告等）が定義されている

Scenario: CONTRIBUTING.md が存在する
  Given `CONTRIBUTING.md` がリポジトリルートに存在する
  When そのドキュメントを読む
  Then セットアップ手順が記載されている
  And VLM feature を有効化する際の注意事項が記載されている
  And PR 作成・レビューのフローが記載されている

Scenario: VLM 有効化時の追加レビュー手順が記載されている
  Given CONTRIBUTING.md に VLM セクションが存在する
  When VLM feature を有効化する変更をレビューする
  Then 以下の確認項目がリストアップされている:
    - 外部 API へのデータ送信の同意フロー
    - API キー管理（環境変数 vs config file）
    - edgequake-* 推移的依存の監査
```

## 受け入れ基準
- [ ] `docs/RC-CRATE-POLICY.md` が作成されている
- [ ] 使用中の RC クレート（notify, ort）とその理由が記載されている
- [ ] 各クレートの stable アップグレード条件が明記されている
- [ ] `CONTRIBUTING.md` がリポジトリルートに作成されている
- [ ] VLM feature 有効化時のセキュリティレビュー手順が含まれている
- [ ] 新規コントリビューター向けのセットアップ手順が含まれている

## テスト戦略
- ドキュメントのみの PBI のため、自動テストは不要
- レビューによる確認

## 実装アプローチ

### 1. `docs/RC-CRATE-POLICY.md`
```markdown
# RC Crate Policy

## 現状
| Crate | Version | Type | Reason |
|-------|---------|------|--------|
| notify | 9.0.0-rc.4 | RC | v9 API が必要だが stable 未リリース |
| ort | 2.0.0-rc.12 | RC | ONNX Runtime 2.0 API が必要だが stable 未リリース |

## Upgrade 条件
- notify: stable 9.x リリース → 即座にアップグレード
- ort: stable 2.x リリース → 1週間の様子見後にアップグレード
- Security advisory: 該当 crate に影響する脆弱性 → 即座に対処
```

### 2. `CONTRIBUTING.md`
- リポジトリセットアップ手順
- ビルド・テスト手順
- PR 作成フロー
- VLM 機能のセキュリティノート:
  - VLM はデフォルトで無効
  - 有効化時は `docs/Support-PDF.md` の注意事項を確認
  - 外部 API に送信されるデータの種類を理解する
  - API キーは環境変数で管理する
- RC クレートポリシーへのリンク

## 見積もり
1 ポイント（半日）

## 技術的考慮事項
- `CONTRIBUTING.md` は GitHub が自動でリンク表示するため、リポジトリルートに配置
- 既存の `CLAUDE.md` があるので、CONTRIBUTING.md は CLAUDE.md を補完する位置づけ（CLAUDE.md は AI 向け、CONTRIBUTING.md は人間向け）
- `docs/RC-CRATE-POLICY.md` は更新が必要になるたびにメンテナンスする。PBI-42（RC→stable 移行）完了後はこのドキュメントも更新・または削除する

## 実装者向け注記

### 現状コードの確認
```bash
# 既存の RC コメント
grep -B2 "rc\." core/Cargo.toml

# 既存のドキュメント一覧
ls docs/*.md

# リポジトリルートの既存ドキュメント
ls *.md
```

### 実装手順
1. `docs/RC-CRATE-POLICY.md` を作成
2. `CONTRIBUTING.md` を作成
3. 内容をレビュー
4. 必要に応じて `ref/cli.md` や他のドキュメントからリンクを追加

### 落とし穴
- ドキュメントが古くなると価値が下がる。PBI-42 完了時には必ず RC-CRATE-POLICY.md を更新するよう、PBI-42 の Definition of Done に含めることを推奨
- CONTRIBUTING.md が CLAUDE.md と重複しすぎないように注意。CLAUDE.md は主に AI エージェント向けの詳細な実装コンテキストであり、CONTRIBUTING.md は人間の開発者向けの入門ガイド
