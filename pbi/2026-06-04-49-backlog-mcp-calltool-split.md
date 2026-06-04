# PBI-49: MCP call_tool ツール別分割

**発端:** Maintainability Guardian - call_tool 関数肥大化
**影響:** `mcp/src/handler.rs` の `call_tool` が全ツールディスパッチを1関数で行い400行超に肥大
**対処:** ツール別ハンドラに分割 (各ツールを個別の関数に)
**工数:** 1-2日
