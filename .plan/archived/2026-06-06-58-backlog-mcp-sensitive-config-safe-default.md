# PBI-58: MCP 機密データマスキングのデフォルト有効化

## ユーザーストーリー
MCP サーバー利用者として、機密データマスキングがデフォルトで有効になってほしい、なぜなら現在は `ToolContext.sensitive_config` が `Option` 型で、`None` を渡すと呼び出し元の設定漏れでメールアドレスや電話番号が意図せず露出するリスクがあるから

## 発端
Checking Team レビュー（`docs/superpowers/plans/2026-06-05-2142-review-PBI-49-handler-split.md`）の Medium 指摘。Red Team Leader が指摘。

## ビジネス価値
- Safe by Default: 実装者が何もしなくても機密情報が保護される
- 情報漏洩リスクの低減
- 設定漏れによる事故防止（テストコードが常に `None` を渡すパターンを防止）

## 前提条件
- `core/src/sensitive.rs` に `mask_sensitive_data()` が実装済み
- `core/src/config.rs` に `SensitiveDataConfig` が定義済み
- `mcp/src/main.rs` の `dispatch()` が `sensitive_config: Option<&SensitiveDataConfig>` を受け取る
- 現状: `mcp/src/handler/mod.rs:81` の `make_test_ctx` が常に `None` を渡す

## BDD 受け入れシナリオ

```gherkin
Scenario: デフォルトで機密データがマスキングされる
  Given sensitive_config が明示的に指定されていない
  When 検索結果にメールアドレス "user@example.com" が含まれる
  Then レスポンスでは "user@example.com" がマスキングされている

Scenario: テストコードでもマスキングが有効
  Given make_test_ctx でハンドラをテストする
  When テストデータに電話番号 "090-1234-5678" が含まれる
  Then レスポンスではマスキングされている

Scenario: 明示的に無効化できる
  Given sensitive_config に SensitiveDataConfig { enabled: false } を指定する
  When 検索結果にメールアドレスが含まれる
  Then レスポンスではマスキングされていない
```

## 受け入れ基準
- [ ] `ToolContext.sensitive_config` が `Option<&SensitiveDataConfig>` から `&SensitiveDataConfig` に変更されている
- [ ] `make_test_ctx` がデフォルトで有効なマスキング設定を使用する
- [ ] `mcp/src/main.rs` の `dispatch()` がデフォルトの `SensitiveDataConfig` を渡す
- [ ] 全テストがパスする
- [ ] 明示的に無効化する方法が維持されている

## テスト戦略（t_wada スタイル）

### 統合テスト（修正）
- `test_handle_search_local_notes_masks_sensitive_data` — 検索結果にメールアドレスを含めてマスキング確認
- `test_handle_get_surrounding_context_masks_sensitive_data` — コンテキスト結果のマスキング確認

### 単体テスト（既存・維持）
- `sensitive.rs` の全テスト（既存 20+ tests）

## 実装アプローチ
- **Outside-In**: テスト（失敗）→ 実装（グリーン）→ リファクタリング
- **既存の動作を変えずに、デフォルトを安全側に倒す**

### 実装方針

```rust
// handler/mod.rs
pub(crate) struct ToolContext<'a> {
    pub vaults: &'a [(String, PathBuf)],
    pub db_path: &'a Path,
    pub backlink_scoring: bool,
    pub sensitive_config: &'a SensitiveDataConfig,  // Option を削除
}

pub fn call_tool(
    name: &str,
    args: &Value,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    backlink_scoring: bool,
    sensitive_config: &SensitiveDataConfig,  // 非 Option に
) -> Result<Value, Box<dyn std::error::Error>> {
```

`mcp/src/main.rs` の `dispatch()` と `main()` から `Option` を外す:
```rust
pub fn dispatch(req, vaults, db_path, backlink_scoring, sensitive_config: &SensitiveDataConfig) {
    match handler::call_tool(name, args, vaults, db_path, backlink_scoring, sensitive_config) {
```

テストヘルパー:
```rust
pub(crate) fn make_test_ctx<'a>(
    _temp: &'a TempDir,
    vaults: &'a [(String, PathBuf)],
    db_path: &'a std::path::Path,
) -> ToolContext<'a> {
    ToolContext {
        vaults,
        db_path,
        backlink_scoring: true,
        sensitive_config: &SensitiveDataConfig::default(),  // デフォルト有効
    }
}
```

## 見積もり
2 ポイント（1〜2時間、main.rs と handler.rs の interface 変更あり）

## 技術的考慮事項
- 依存関係: `mcp/src/main.rs` の `dispatch()` 関数シグネチャ変更 — 呼び出し元 2 箇所の修正が必要
- テスタビリティ: `SensitiveDataConfig::default()` は `enabled: true` なので、テストで有効な状態を簡単に作れる
- 互換性: `dispatch()` は `pub` だが実際の呼び出しは `main.rs` 内のみ。外部 crate からの利用はない
- 影響範囲: `mcp/src/main.rs` + `mcp/src/handler/mod.rs` + テストコード

## 実装者向け注記

### 現状コードの確認
```bash
# dispatch 関数のシグネチャ
grep -n "pub fn dispatch" mcp/src/main.rs

# call_tool のシグネチャ
grep -n "pub fn call_tool" mcp/src/handler/mod.rs

# make_test_ctx の実装
grep -n "fn make_test_ctx" mcp/src/handler/mod.rs -A 10

# sensitive_config の使用箇所一覧
grep -rn "sensitive_config" mcp/src/
```

### 実装手順
1. `mcp/src/handler/mod.rs`: `ToolContext.sensitive_config` を `&SensitiveDataConfig`（非 Option）に変更
2. `mcp/src/handler/mod.rs`: `call_tool()` の `sensitive_config` パラメータを非 Option に変更
3. `mcp/src/handler/mod.rs`: `make_test_ctx()` の `sensitive_config` を `&SensitiveDataConfig::default()` に変更
4. `mcp/src/handler/search.rs` と `context.rs`: `mask_sensitive_data` の呼び出しを調整（`ctx.sensitive_config` が非 Option になる）
5. `mcp/src/main.rs`: `dispatch()` の `sensitive_config` パラメータを非 Option に変更
6. `mcp/src/main.rs`: `main()` 内の `dispatch()` 呼び出しを調整
7. `cargo test -p shiotsuchi-mcp` で全テストパス確認

### 落とし穴
- `main.rs` の `dispatch()` 呼び出しは 2 箇所ある（360 行目と 354 行目周辺）
- `dispatch()` のシグネチャ変更は `#[cfg(test)]` 内のテストも影響を受ける
- `search.rs` の `mask_sensitive_data(&markdown, ctx.sensitive_config)` は Option だったので `.unwrap_or()` 相当の処理が不要になる

## Definition of Done
- [ ] 全 BDD シナリオが自動テストとして実装されパスする
- [ ] `ToolContext.sensitive_config` が非 `Option` になっている
- [ ] `make_test_ctx` がデフォルト有効の設定を使用する
- [ ] `cargo test -p shiotsuchi-mcp` が全テストパス（41 tests）
- [ ] `cargo test -p shiotsuchi-core` が全テストパス（466 tests）
