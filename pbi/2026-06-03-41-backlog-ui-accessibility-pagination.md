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
  When Tab キーでタブにフォーカスを移動し Enter キーを押す
  Then 対応するパネルが表示される

Scenario: 検索結果がキーボードで操作できる
  Given 検索結果が表示されている
  When Tab キーで結果カードにフォーカスを移動し Enter キーを押す
  Then ファイルビューアモーダルが開く

Scenario: ファイル一覧がページ分割される
  Given 1000 ファイルがインデックスされている
  When Files タブを開く
  Then 最初の 50 ファイルが表示され「さらに読み込み」ボタンが表示される
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

## テスト戦略（t_wada スタイル）

### Unit Test
- ARIA 属性の存在確認
- キーボードイベントハンドラのテスト

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
- [ ] タブがキーボードで操作できる
- [ ] 検索結果がキーボードで操作できる
- [ ] ファイル一覧がページ分割される
- [ ] モーダルにフォーカストラップがある
- [ ] fetch タイムアウトが動作する
- [ ] テストがパスする
