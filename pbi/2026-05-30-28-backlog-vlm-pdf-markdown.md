# PBI: VLM ベースの PDF Markdown 化（スキャン PDF 対応）

## ユーザーストーリー
スキャンした PDF（テキスト埋め込みなし）をノートに添付しているユーザーとして、その内容でも検索したい、なぜなら PBI-21 のネイティブテキスト抽出ではスキャン PDF はテキストが空になり検索対象外のままだから

## ビジネス価値
- PBI-21（Phase A）でインデックス済みだがテキスト空の PDF に対して VLM でテキストを生成
- スキャン文書・画像のみ PDF の内容を検索対象に追加
- PBI-21 の「空テキストでもインデックス」方針との連携で、再インデックスではなく更新として実装できる

## 前提条件
- PBI-21 が実装済みであること（`pdf` feature、`pdfium-render` bundled、空テキスト DB 残し）

## BDD 受け入れシナリオ

```gherkin
Scenario: スキャン PDF のテキストで検索できる
  Given ノートに `![[scan.pdf]]` が埋め込まれており、PDF はスキャン画像のみ（テキスト埋め込みなし）
  When ユーザーが VLM 抽出を有効にして `shiotsuchi chart` を実行する
  Then そのノートが VLM 抽出テキストで検索結果に含まれる

Scenario: VLM 抽出は初回のみ実行される
  Given PDF ファイルが変更されていない
  When `shiotsuchi chart` を再実行する
  Then VLM は再実行されずキャッシュ済み結果を使う
```

## 受け入れ基準
- [ ] テキストが空の PDF に対して VLM でページ画像 → テキスト変換を実行する
- [ ] 結果は既存の `file_cache` mtime キャッシュで管理し再実行を防ぐ
- [ ] VLM プロバイダー（OpenAI / Anthropic / Gemini 等）を設定ファイルで選択できる
- [ ] VLM 機能は feature flag または設定でオフにできる
- [ ] API キー未設定時はスキップして警告ログを出す

## 見積もり
8 ポイント

## 技術的考慮事項

### アプローチ候補

**候補 A: `edgequake-pdf2md` ライブラリ利用**
- `pdfium-render` で PDF をページ画像化 → VLM API で Markdown 化 → FTS5 更新
- `edgequake-pdf2md` は内部で `pdfium-render` を使っており、PBI-21 との bundled 共有が可能か要確認
- ライブラリ API: `convert_from_bytes(&bytes, &config).await?`

**候補 B: 自前実装**
- PBI-21 の `pdfium-render` でページを画像バイト列に変換
- Anthropic / OpenAI API クライアント（`ureq` 等）で直接呼び出し
- `edgequake-llm` クレートで複数プロバイダーを抽象化する選択肢もあり

### PBI-21 との連携ポイント
- PBI-21 でテキストが空の PDF は DB に `hash` が記録されている
- VLM 抽出成功時は `db.reindex_file()` で `IndexResult::Updated` として上書き
- `file_cache` の mtime キャッシュはそのまま流用可能

### コスト目安（参考）
- GPT-4.1: 50 ページ約 $0.40
- Amazon Nova Lite: 50 ページ約 $0.01
- ローカル VLM（Ollama + llava 等）: API コストゼロ

## Definition of Done
- [ ] スキャン PDF の VLM テキスト抽出テストがパスする
- [ ] コードレビュー完了
