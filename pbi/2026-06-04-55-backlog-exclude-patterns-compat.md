# PBI-55: exclude_patterns → exclude_dirs 後方互換性

**発端:** Legacy Bridge Architect (スコア90)
**影響:** `exclude_patterns` → `exclude_dirs` のリネームに後方互換性がない
**対処:** 旧キーのフォールバックまたは明示的拒否
**工数:** 完了済み（異なるアプローチで解決）
**状態:** 解決済み

## 解決状況

### 選択されたアプローチ: 明示的拒否（エラーメッセージ付き）

自動フォールバックではなく、**明示的にエラーを返す** 方式を採用。

```rust
// core/src/models.rs
/// Renamed from `exclude_patterns` — the old key will cause a deserialize error.
pub exclude_dirs: Vec<String>,
```

- `deny_unknown_fields` により `exclude_patterns` キーはコンパイルエラー
- エラーメッセージに旧キー名が含まれ、ユーザーに新しいキー名を案内

## BDD 受け入れシナリオ

```gherkin
Scenario: 旧キー exclude_patterns でエラーが返される
  Given config.toml に exclude_patterns = ["build"] が記載されている
  When 設定を読み込む
  Then エラーが発生する
  And エラーメッセージに exclude_patterns が含まれる

Scenario: 新キー exclude_dirs で設定が読み込まれる
  Given config.toml に exclude_dirs = ["build"] が記載されている
  When 設定を読み込む
  Then 設定が正しく読み込まれる
  And exclude_dirs の値が ["build"] になる

Scenario: exclude_dirs が未設定の場合はデフォルト値が使用される
  Given config.toml に exclude_dirs が記載されていない
  When 設定を読み込む
  Then exclude_dirs の値が ["node_modules"] になる
```

## TDD アプローチ

### Phase 1: テスト（グリーン維持）

既存テストが全て通過することを確認:
```rust
#[test]
fn test_exclude_patterns_rename_backward_compat_denied() {
    let toml = r#"
        [indexing]
        exclude_patterns = ["build"]
    "#;
    let result: Result<ShiotsuchiConfig, _> = toml::from_str(toml);
    assert!(result.is_err(), "old `exclude_patterns` key must be rejected");
}

#[test]
fn test_exclude_dirs_accepts_new_key() {
    let toml = r#"
        [indexing]
        exclude_dirs = ["build"]
    "#;
    let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.indexing.exclude_dirs, vec!["build"]);
}

#[test]
fn test_exclude_dirs_default_deserialize() {
    let toml = r"
        [indexing]
    ";
    let config: ShiotsuchiConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.indexing.exclude_dirs, vec!["node_modules"]);
}
```

### Phase 2: 判断理由

- **自動フォールバックのリスク**: ユーザーが意図せず古い設定が適用される可能性
- **明示的拒否のメリット**: 設定ファイルの更新を促し、意図しない動作を防止
- **ドキュメント化済み**: CHANGELOG v0.4.18 で `exclude_dirs` への移行を明記

## 結論

この PBI は **クローズ** します。自動フォールバックではなく、明示的拒否 + エラーメッセージの方針で解決済み。
