# PBI: MCP Read-Write 拡張（ノート作成・追記・タグ付け）

**⚠️ この PBI は却下されました。** `pbi/AGENTS.md` を参照。

shiotsuchi-search は読み取り専用の検索エンジンであり、ノートの作成・編集・削除はプロジェクト範囲外。

## ビジネス価値
- AI が「新しいノートを作成」「既存ノートに追記」「タグ自動付与」できるようになる
- 知識ベースの管理をAIに委譲できる

## BDD 受け入れシナリオ

```gherkin
Scenario: AI が新しいノートを作成する
  Given MCP サーバーが起動している
  When AI が `create_note` ツールを呼ぶ（title, content, tags を渡す）
  Then Vault 内に Markdown ファイルが作成され、インデックスに追加される

Scenario: AI が既存ノートに追記する
  Given Vault 内に "プロジェクト.md" が存在する
  When AI が `append_to_note` ツールを呼ぶ
  Then ファイルの末尾に内容が追記され、インデックスが更新される
```

## 受け入れ基準
- [ ] `create_note` MCP ツールが実装される
- [ ] `append_to_note` MCP ツールが実装される
- [ ] `add_tags` MCP ツールが実装される
- [ ] パストラバーサル保護が書き込み操作にも適用される

## 見積もり
8 ポイント

## 技術的考慮事項
- 影響ファイル: `mcp/src/handler.rs`、`ref/mcp.md`
- セキュリティ: パストラバーサル保護必須

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
# 既存のMCPツール一覧を確認する
grep -n "fn handle\|tool_name\|\"search\|\"read\|\"create\|\"write" mcp/src/handler.rs | head -30
# パストラバーサル保護の既存実装を確認する
grep -n "canonicalize\|starts_with\|traverse\|path_traversal" mcp/src/handler.rs core/src/ -r | head -20
```

### セキュリティ要件（最重要）

**パストラバーサル保護を必ず実装すること。** 既存の `read_full_note` に実装がある場合は同じパターンを流用する。

```rust
fn safe_note_path(vault_dir: &Path, requested: &str) -> Result<PathBuf, Error> {
    let joined = vault_dir.join(requested);
    let canonical = joined.canonicalize()?;
    // vault_dir の外に出ていないか確認
    if !canonical.starts_with(vault_dir) {
        return Err(Error::PathTraversal);
    }
    Ok(canonical)
}
```

### 実装手順

1. **`create_note` ツールを実装する**
   - パラメータ: `title: str`, `content: str`, `tags: [str]`（optional）
   - ファイル名: タイトルをサニタイズして `.md` 拡張子を付ける
   - 作成後に `index_file` を呼んでインデックスを更新する

2. **`append_to_note` ツールを実装する**
   - パラメータ: `path: str`, `content: str`
   - 更新後にインデックスを再更新する

3. **MCP ツールスキーマを `ref/mcp.md` に追記する**

### 落とし穴

- ファイル名にはOSが許可しない文字（`/`, `:`, `\0` 等）が含まれないようにサニタイズする。
- 既に同名のファイルが存在する場合の動作を決める（上書き禁止 or 連番付与）。
- `index_file` の呼び出しは write 後に同期的に行う。非同期にすると MCP レスポンス後に index が更新されていない状態が発生する。

## Definition of Done
- [ ] 全 Write ツールのテストがパスする
- [ ] パストラバーサル保護のテストがパスする
- [ ] コードレビュー完了
