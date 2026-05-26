# PBI: MCP スマートチャンク化とコンテキスト抽出

## ユーザーストーリー
Claude 等の AI と MCP 経由で連携するユーザーとして、長いノートを賢く切り出してトークンを節約したい、なぜなら数万文字のノートを丸ごと渡すとトークンを大量消費するから

## ビジネス価値
- LLM のトークン消費を削減しコストを抑制
- 見出し・段落単位の関連コンテキストを AI に渡すことで回答精度が向上

## BDD 受け入れシナリオ

```gherkin
Scenario: 検索ヒット箇所の前後コンテキストが抽出される
  Given MCP 経由で `search_notes` を呼ぶ
  When ヒットしたノートが 5000 文字以上の長文である
  Then ヒット箇所を含む段落とその前後 1 段落が切り出されて返される

Scenario: 短いノートは全文が返される
  Given ヒットしたノートが 500 文字未満である
  When MCP 経由で `search_notes` を呼ぶ
  Then 全文がそのまま返される
```

## 受け入れ基準
- [x] MCP `search_notes` レスポンスにスマートチャンクが含まれる
- [x] チャンクサイズ（文字数）を設定で変更できる
- [x] 短いノートは全文返却

## 見積もり
5 ポイント

## 技術的考慮事項
- 影響ファイル: `mcp/src/handler.rs`、`core/src/search.rs`
- Markdown の見出し（`#`）を区切りとして段落分割

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
# 現状のチャンキング実装を確認する
cat core/src/chunker.rs | head -60
grep -n "split_into_chunks\|ChunkSearchResult\|content" core/src/search.rs | head -20
grep -n "search_notes\|read_note\|snippet" mcp/src/handler.rs | head -20
```

`core/src/chunker.rs` に `split_into_chunks` が既にあり、見出しベースのチャンク分割は実装済みです。  
MCP の `search_notes` が返す `content` フィールドがすでにチャンク単位になっているか確認する。

### 実装の焦点

現状の MCP レスポンスが既にチャンク単位なら、このPBIは：
1. **チャンクサイズの設定化**（`max_chunk_chars` を config で変更可能にする）
2. **前後コンテキスト付加**（ヒットチャンクの前後チャンクも一緒に返す `context_chunks: 1` オプション）

に絞られる。

### 実装手順（コンテキスト付加）

```rust
// mcp/src/handler.rs の search_notes ハンドラで
// ヒットしたチャンクの隣接チャンクも取得して付加する
fn get_context_chunks(db: &NoteDatabase, chunk_id: i64, window: usize) -> Vec<Chunk>
```

### 落とし穴

- `ChunkSearchResult` の `content` フィールドは既にチャンク内容が入っている。  
  これをさらに細かく分割すると二重チャンキングになるため注意。
- MCP レスポンスのスキーマ変更は Claude Desktop 側の期待と合わせる必要がある。フィールド追加は後方互換だが、フィールドの型変更は避けること。

## Definition of Done
- [x] チャンク抽出のテストがパスする
- [x] コードレビュー完了
