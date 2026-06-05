# PBI-57: MCP 全エンドポイントへのレート制限追加

## ユーザーストーリー
MCP サーバー運用者として、全 MCP ツールエンドポイントにレート制限がほしい、なぜなら `get_surrounding_context` や `index_status` にもレート制限がないと、クライアントの誤動作や攻撃で DB リソースが枯渇するリスクがあるから

## 発端
Checking Team レビュー（`docs/superpowers/plans/2026-06-05-2142-review-PBI-49-handler-split.md`）の Medium 指摘。Red Team Leader、Blue Team Leader、System Architect の 3 名が同内容を指摘。

## ビジネス価値
- DoS 攻撃 / クライアント誤動作からの保護
- 全エンドポイントで一貫したリソースガバナンス
- SRE/Ops 観点の安全性向上

## 前提条件
- `mcp/src/handler/search.rs` に `SEARCH_RATE_LIMITER`（10 req/s の SlidingWindowRateLimiter）が既存
- 現状: `search_local_notes` のみレート制限あり、`get_surrounding_context` と `index_status` は制限なし

## BDD 受け入れシナリオ

```gherkin
Scenario: get_surrounding_context がレート制限される
  Given レートリミッターが上限 3 req/s で設定されている
  When 1 秒間に 4 回 `get_surrounding_context` を呼び出す
  Then 最初の 3 回は正常なレスポンスが返る
  And 4 回目はレート制限エラーが返る

Scenario: index_status がレート制限される
  Given レートリミッターが上限 3 req/s で設定されている
  When 1 秒間に 4 回 `index_status` を呼び出す
  Then 最初の 3 回は正常なレスポンスが返る
  And 4 回目はレート制限エラーが返る

Scenario: 共通レートリミッターで全ツール呼び出しは合計カウントされる
  Given レートリミッターが上限 5 req/s で設定されている
  When `search_local_notes` と `get_surrounding_context` を交互に 6 回呼び出す
  Then 最初の 5 回は正常なレスポンスが返る
  And 6 回目はレート制限エラーが返る（共通カウンターのため）
```

## 受け入れ基準
- [ ] `get_surrounding_context` にレート制限が実装されている
- [ ] `index_status` にレート制限が実装されている
- [ ] レート制限超過時にわかりやすいエラーメッセージが返る
- [ ] 既存の `search_local_notes` のレート制限は維持される
- [ ] レート制限ユニットテストが追加されている

## テスト戦略（t_wada スタイル）

### 統合テスト
- `test_get_surrounding_context_rate_limited` — 直接ハンドラテストでレート制限確認
- `test_index_status_rate_limited` — 同様
- `test_rate_limiter_sliding_window` — 既存テスト（維持）

### 単体テスト
- `test_general_rate_limiter_blocks_after_limit` — 新規
- `test_general_rate_limiter_resets_after_window` — 新規
- `test_general_rate_limiter_shared_counter` — 新規（共通カウンターで複数ツールの呼び出しが合算されることを確認）

## 実装アプローチ
- **Outside-In**: 統合テスト（失敗）→ 実装（グリーン）→ リファクタリング
- **Red-Green-Refactor**: TDD サイクル

### 実装方針（2案）

**案 A: call_tool エントリポイントで共通レート制限（推奨）**
- `handler/mod.rs` に `GENERAL_RATE_LIMITER`（50 req/s）を追加
- `call_tool` 関数内で全ツール呼び出し前にチェック
- `search_local_notes` は従来の `SEARCH_RATE_LIMITER`（10 req/s）も維持（二重チェック）
- 利点: 1箇所の変更で全エンドポイントを保護、実装がシンプル
- 欠点: 検索とステータスで同じ閾値になる

**案 B: エンドポイントごとに個別レート制限**
- 各ハンドラファイルに個別のレートリミッターを追加
- 利点: エンドポイントごとに異なる閾値を設定可能
- 欠点: コード重複、保守性低下

## 深掘りセッション — 2026-06-06

### 挑戦した仮定
| 仮定 | リスク | 発見 | 決定 |
|------|--------|------|------|
| 二重チェック（GENERAL 50 + SEARCH 10）が適切 | 中 | エラーメッセージに矛盾が生じる。search が GENERAL を通り SEARCH で止まった場合、GENERAL のメッセージ（50 req/s）が返る | エラーメッセージを一般化: `"Rate limit exceeded. Please wait before trying again."`（数値なし） |
| rebuild_index は call_tool 対象外でよい | 高 | main.rs が直接 spawn_rebuild を呼び、レート制限をバイパスしていた | handler::check_rate_limit() 関数を公開し、main.rs から呼べるようにする。rebuild もレート制限の対象に含める |
| 50 req/s が適切なデフォルト値 | 高 | index_status は 1-10ms の軽量クエリ。しかし実測データがないため根拠が不十分 | 一旦 50 req/s でリリースし、実測後に調整（変更は定数1行） |
| SEARCH_RATE_LIMITER の二重チェック維持 | 中 | 実質的な制限値は 10 req/s（先に SEARCH が fire）。GENERAL は実質 dead code になる可能性 | SEARCH_RATE_LIMITER は search.rs に維持。GENERAL は他のエンドポイント用 |
| index_status は軽量 | 中 | db.stats() は単一クエリ、1-10ms。実用上レート制限なしでも問題ないレベル | 50 req/s で一旦実装。一貫性のための保護 |

### 設計上の注意: 共通カウンター vs per-tool独立
案A（共通リミッター）では全ツール呼び出しが同一カウンターを共有する。これは「search_local_notes が多用されると index_status がレート制限される」という副作用をもたらす。この挙動は意図的（全体リソース保護）であり、per-tool独立カウンターは案A では実現しない。per-tool独立が必要なら案Bへの変更が必要。現時点は案A（共通カウンター）を採用。

### 新たに発見したリスク
- `rebuild_index` が main.rs で special-case 処理されているため、新しい tool を handler に追加した開発者が「call_tool を通れば勝手に制限される」と思い込む可能性
- デフォルトレートの根拠がないままリリースすると、後で「なぜ 50 なのか」説明できない技術負債になる

### 未解決の疑問
- 本番運用での実際のレート分布（どのツールが何 req/s で呼ばれるか）

### 決定事項
1. **エラーメッセージ**: `"Rate limit exceeded. Please wait before trying again."` — 数値なし、一般化
2. **rebuild_index**: `handler::check_rate_limit()` 関数を公開。main.rs が呼び出し、通過後に spawn_rebuild を実行
3. **デフォルトレート**: 50 req/s。リリース後に実測して調整

**採用: 案 A（共通レート制限）+ 案 C（check_rate_limit 関数公開）**

```rust
// handler/mod.rs
/// General rate limiter for all MCP tools (50 requests/second).
static GENERAL_RATE_LIMITER: LazyLock<SlidingWindowRateLimiter> =
    LazyLock::new(|| SlidingWindowRateLimiter::new(50));

/// Check the general rate limit. Returns false if rate limited.
pub fn check_rate_limit() -> bool {
    GENERAL_RATE_LIMITER.allow()
}

pub fn call_tool(...) -> Result<Value, Box<dyn std::error::Error>> {
    if !check_rate_limit() {
        return Ok(json!({
            "content": [{
                "type": "text",
                "text": "Rate limit exceeded. Please wait before trying again."
            }],
            "isError": true
        }));
    }
    // ... dispatch ...
}
```

```rust
// main.rs (rebuild_index handling)
if !handler::check_rate_limit() {
    McpResponse::success(
        req.id,
        json!({
            "content": [{
                "type": "text",
                "text": "Rate limit exceeded. Please wait before trying again."
            }],
            "isError": true
        }),
    )
} else {
    spawn_rebuild(vaults.clone(), &db_path, &stdout, args, progress_token);
    McpResponse::success(... "Rebuild started. ...")
}
```

## 見積もり
1 ポイント（30分〜1時間）

## 技術的考慮事項
- 依存関係: なし（`SlidingWindowRateLimiter` は既存）
- テスタビリティ: `SlidingWindowRateLimiter::new(n)` でテスト可能
- 互換性: MCP クライアント互換性に影響なし（新規エラーレスポンスが追加されるのみ）
- レート制限値: 50 req/s（一旦固定、実測後に調整）
- エラーメッセージ: 数値なし一般化メッセージ

## 実装者向け注記

### 現状コードの確認
```bash
# 既存のレート制限実装を確認
grep -rn "RATE_LIMITER\|rate_limiter" mcp/src/handler/
```

既存の `search_local_notes` の `SEARCH_RATE_LIMITER`（10 req/s）は維持すること。`GENERAL_RATE_LIMITER`（50 req/s）は全エンドポイントのベースライン保護として追加する。

### 実装手順
1. `handler/mod.rs` に `GENERAL_RATE_LIMITER` + `pub fn check_rate_limit()` を追加
2. `call_tool` 関数の冒頭で `check_rate_limit()` をチェック
3. `main.rs` の rebuild_index 分岐でも `handler::check_rate_limit()` をチェック
4. `handler/mod.rs` の `#[cfg(test)]` ブロック内にある `use shiotsuchi_core::rate_limiter::SlidingWindowRateLimiter;` を**削除**し、ファイル先頭（`use serde_json::Value;` の周辺）に通常スコープの `use` として追加する
5. テスト: 直接ハンドラテストを追加
6. `cargo test -p shiotsuchi-mcp` で全テストパス確認

### 落とし穴
- `SEARCH_RATE_LIMITER` を削除しない（search は 10 req/s の厳格制限を維持）
- テストでレート制限を使うときは `SlidingWindowRateLimiter::new(n)` で独立したインスタンスを使う（static を使わない）
- `main.rs` の rebuild 分岐は1箇所のみ（`req.method == "tools/call"` かつ `name == "rebuild_index"` の分岐、line ~336）。ここにチェックを入れること
- エラーメッセージに数値を含めないこと（deep dig 決定事項）

## Definition of Done
- [ ] 全 BDD シナリオが自動テストとして実装されパスする
- [ ] `get_surrounding_context` と `index_status` にレート制限が実装されている
- [ ] `rebuild_index` も `handler::check_rate_limit()` 経由でレート制限されている
- [ ] エラーメッセージが数値なし一般化形式である
- [ ] `cargo test -p shiotsuchi-mcp` が全テストパス（新規テスト追加後、41+n tests）
- [ ] 既存の `search_local_notes` のレート制限が維持されている
