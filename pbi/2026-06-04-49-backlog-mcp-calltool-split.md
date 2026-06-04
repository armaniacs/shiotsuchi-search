# PBI-49: MCP call_tool ツール別分割

**発端:** Maintainability Guardian - call_tool 関数肥大化
**影響:** `mcp/src/handler.rs` の `call_tool` が全ツールディスパッチを1関数で行い406行に肥大
**対処:** ツール別ハンドラに分割 (各ツールを個別の関数に)
**工数:** 1-2日
**状態:** 未着手

## 現状

- `mcp/src/handler.rs`: 406行
- `call_tool` 関数が `search_local_notes`, `get_surrounding_context`, `index_status`, `rebuild_index` の全ディスパッチを1関数で処理
- 各ツールのロジックが `call_tool` 内にネストしている
- 既存テスト: 10個の `#[test]` が `handler.rs` に存在（ツールディスパッチの統合テスト）

## BDD 受け入れシナリオ

```gherkin
Scenario: search_local_notes が個別ハンドラで処理される
  Given MCP サーバーが起動している
  When "search_local_notes" ツールが呼び出される
  Then handle_search_local_notes() が呼ばれる
  And 結果が MCP レスポンスとして返される

Scenario: get_surrounding_context が個別ハンドラで処理される
  Given MCP サーバーが起動している
  When "get_surrounding_context" ツールが呼び出される
  Then handle_get_surrounding_context() が呼ばれる

Scenario: index_status が個別ハンドラで処理される
  Given MCP サーバーが起動している
  When "index_status" ツールが呼び出される
  Then handle_index_status() が呼ばれる

Scenario: rebuild_index が個別ハンドラで処理される
  Given MCP サーバーが起動している
  When "rebuild_index" ツールが呼び出される
  Then handle_rebuild_index() が呼ばれる

Scenario: 未知のツール名でエラーが返される
  Given MCP サーバーが起動している
  When "unknown_tool" ツールが呼び出される
  Then McpError::UnknownTool が返される

Scenario: 既存テストが全て通過する
  Given 分割前の全テストが通過している
  When ツール別分割を実装する
  Then 分割後も全てのテストが通過する
```

## TDD アプローチ

### Phase 1: 既存動作の保証（レッド → グリーン）

1. **既存テストの確認**: `cargo test -p shiotsuchi-mcp` で全テスト通過を確認
2. **リファクタリングの安全性**: テストが既存動作を保証しているため、リファクタリング中にテストが失敗すれば動作が変わったことを検出できる

### Phase 2: 分割実装（グリーン → リファクタリング）

1. **`call_tool` を match 文のみに分割**: 各ツールのロジックを `handle_*` 関数に移動
2. **テスト実行**: 各ステップで `cargo test -p shiotsuchi-mcp` を実行
3. **テストが失敗した場合**: 元のコードに戻し、正确的に移動

### Phase 3: 個別ハンドラのテスト追加

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_tool_dispatches_search_local_notes() {
        // search_local_notes が正しくディスパッチされることを確認
    }

    #[test]
    fn test_call_tool_dispatches_get_surrounding_context() {
        // get_surrounding_context が正しくディスパッチされることを確認
    }

    #[test]
    fn test_call_tool_dispatches_index_status() {
        // index_status が正しくディスパッチされることを確認
    }

    #[test]
    fn test_call_tool_dispatches_rebuild_index() {
        // rebuild_index が正しくディスパッチされることを確認
    }

    #[test]
    fn test_call_tool_returns_error_for_unknown_tool() {
        // 未知のツール名でエラーが返されることを確認
    }
}
```

## 実装方針

```rust
// 分割後
async fn call_tool(...) -> Result<...> {
    match tool_name {
        "search_local_notes" => handle_search_local_notes(...).await,
        "get_surrounding_context" => handle_get_surrounding_context(...).await,
        "index_status" => handle_index_status(...).await,
        "rebuild_index" => handle_rebuild_index(...).await,
        _ => Err(McpError::UnknownTool(tool_name)),
    }
}

// 各ハンドラは独立した関数
async fn handle_search_local_notes(...) -> Result<...> { ... }
async fn handle_get_surrounding_context(...) -> Result<...> { ... }
async fn handle_index_status(...) -> Result<...> { ... }
async fn handle_rebuild_index(...) -> Result<...> { ... }
```

## メリット

- 各ツールのロジックが独立し、テスト容易性が向上
- 新ツール追加時の変更箇所が限定される
- コンテキスト保持量が削減され、AI エージェントの作業効率が向上
