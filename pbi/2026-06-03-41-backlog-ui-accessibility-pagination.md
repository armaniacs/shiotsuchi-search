# PBI: UI アクセシビリティ + ページネーション改善

## ユーザーストーリー
アクセシビリティに配慮したユーザーとして、ブラウザ UI がキーボード操作可能でスクリーンリーダーに対応していることがほしい、なぜなら現在の UI はマウス操作のみ対応で、キーボードユーザーが操作できないため

## ビジネス価値
- WCAG 2.1 AA 準拠によるアクセシビリティ改善
- 大量ファイル Vault での UX 改善（ページネーション）
- モバイル環境での操作性向上

## 前提条件
- HTTP API サーバーのブラウザ UI (`/ui`) が実装済みであること

## BDD 受け入れシナリオ

```gherkin
Scenario: タブがキーボードで操作できる
  Given ブラウザ UI が開いている
  When Tab キーで "Stats" タブにフォーカスを移動し Enter キーを押す
  Then Stats パネルが表示される
  And 検索パネルは非表示になる

Scenario: 検索結果カードがキーボードで開ける
  Given 検索結果が 1 件以上表示されている
  When Tab キーで最初の結果カードにフォーカスを移動し Enter キーを押す
  Then ファイルビューアモーダルが開く
  And モーダル内にファイル内容が表示される

Scenario: モーダルが開いた状態でフォーカスがモーダル内に留まる
  Given ファイルビューアモーダルが開いている
  When Tab キーを 5 回押す
  Then フォーカスはモーダル内の要素を循環する
  And モーダル背後の要素にフォーカスが移動しない

Scenario: ファイル一覧の API がページ分割される
  Given 100 ファイルがインデックスされている
  When `GET /api/v1/list?offset=0&limit=50` をリクエストする
  Then 50 ファイルが返される
  And `total` は 100

Scenario: ファイル一覧の 2 ページ目が取得できる
  Given 100 ファイルがインデックスされている
  When `GET /api/v1/list?offset=50&limit=50` をリクエストする
  Then 残り 50 ファイルが返される

Scenario: フェッチがタイムアウトした場合にエラーが表示される
  Given API サーバーが応答しない
  When 検索を実行する
  Then 15 秒以内にエラーメッセージが表示される
```

## 受け入れ基準
- [ ] タブ群に `role="tablist"` / `role="tab"` / `aria-selected` を設定
- [ ] 検索入力に `<label>` 要素を追加
- [ ] モーダルにフォーカストラップを実装
- [ ] クリック可能な要素に `role="button"` / `tabindex="0"` / `keydown` を追加
- [ ] `/api/v1/list` に `offset` / `limit` パラメータを追加
- [ ] UI 側で無限スクロールまたは「さらに読み込み」ボタンを実装
- [ ] `AbortController` で fetch タイムアウト（15秒）を設定
- [ ] `<html lang="en">` に修正（現在は `lang="ja` だが UI は英語）

## テスト戦略（TDD レッド → グリーン → リファクタ）

### Unit Test（各シナリオに対応）
- `test_tab_keyboard_navigation` — Tab でタブ切替
- `test_result_card_keyboard_open` — Enter でモーダル表示
- `test_modal_focus_trap` — Tab でフォーカス循環
- `test_list_pagination_offset_limit` — offset/limit でページ分割
- `test_list_pagination_second_page` — 2 ページ目の取得
- `test_fetch_timeout_error` — タイムアウト時のエラー表示

### Manual Test
- スクリーンリーダー（VoiceOver / NVDA）での操作確認
- キーボードのみでの全操作フロー確認

## 実装アプローチ

### ファイル構成
- `core/src/server/ui.html`: UI の改良
- `core/src/server/handlers.rs`: `/api/v1/list` のページネーション対応
- `core/src/server/types.rs`: `ListParams` の追加

### ページネーション API
```
GET /api/v1/list?offset=0&limit=50
→ {"files": [...], "total": 1000, "offset": 0, "limit": 50}
```

## 見積もり
5 ポイント

## 技術的考慮事項

### パフォーマンス
- 大量ファイル時の DOM 插入をバーチャルスクロールで対応検討
- 現時点では「さらに読み込み」ボタンで十分

### 既存コードとの連携
- `core/src/server/handlers.rs`: `handle_list` に `offset` / `limit` パラメータ追加
- `core/src/db.rs`: `list_cached_paths` にページネーション対応

## Definition of Done
- [ ] `test_tab_keyboard_navigation` がパスする
- [ ] `test_result_card_keyboard_open` がパスする
- [ ] `test_modal_focus_trap` がパスする
- [ ] `test_list_pagination_offset_limit` がパスする
- [ ] `test_list_pagination_second_page` がパスする
- [ ] `test_fetch_timeout_error` がパスする
- [ ] 全テストがパスする
