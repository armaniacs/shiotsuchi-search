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
