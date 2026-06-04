# PBI-52: ONNX Runtime バイナリ SHA 検証

**発端:** Supply Chain & Dependency Sentinel (スコア70)
**影響:** `ort` の `download-binaries` feature によりビルド時にONNX Runtimeバイナリが自動ダウンロードされるが、SHA-256等の整合性検証がない
**対処:** `build.rs` にダウンロードバイナリのチェックサム検証機構を追加
**工数:** 1日
**状態:** 未着手

## 背景

- `ort` crate は `download-binaries` feature でビルド時に ONNX Runtime バイナリをダウンロード
- 現状は検証なしでダウンロードされたバイナリを使用
- サプライチェーン攻撃のリスク

## BDD 受け入れシナリオ

```gherkin
Scenario: ONNX Runtime バイナリが正しく検証される
  Given ONNX Runtime バイナリがダウンロードされている
  When ビルドを実行する
  Then バイナリの SHA-256 ハッシュが事前定義された値と一致する
  And ビルドが成功する

Scenario: ONNX Runtime バイナリが破損している
  Given ONNX Runtime バイナリが破損している
  When ビルドを実行する
  Then ハッシュ不一致エラーが発生する
  And ビルドが失敗する

Scenario: ort crate が既に検証を行っている
  Given ort crate の download-binaries が検証機能を提供している
  When ビルドを実行する
  Then 追加の検証は実行されない
  And ビルドが成功する
```

## TDD アプローチ

### Phase 1: 調査（テスト前）

1. **ort crate のドキュメント確認**: `download-binaries` の検証機能を調査
2. **リスク評価**: 実際のサプライチェーン攻撃リスクを評価
3. **方針決定**: 検証が必要かどうかを判断

### Phase 2: 検証が必要な場合（テスト追加 → 実装）

```rust
// build.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_onnx_binary_hash_verification() {
        // ONNX Runtime バイナリのハッシュが正しいことを確認
        let binary_path = get_onnx_binary_path();
        let hash = compute_sha256(&binary_path);
        assert_eq!(hash, EXPECTED_ONNX_HASH);
    }

    #[test]
    fn test_onnx_binary_hash_mismatch_detected() {
        // ハッシュ不一致が検出されることを確認
        let fake_path = Path::new("/tmp/fake_onnx_binary");
        std::fs::write(&fake_path, b"fake binary").unwrap();
        let hash = compute_sha256(&fake_path);
        assert_ne!(hash, EXPECTED_ONNX_HASH);
    }
}
```

### Phase 3: 実装

```rust
// build.rs
fn compute_sha256(path: &Path) -> String {
    let bytes = std::fs::read(path).unwrap();
    let hash = sha2::Sha256::digest(&bytes);
    hex::encode(hash)
}

fn verify_onnx_binary(path: &Path) -> bool {
    let hash = compute_sha256(path);
    hash == EXPECTED_ONNX_HASH
}
```

## 実装方針

### 方法1: ort crate の標準検証（推奨）

`ort` crate は `download-binaries` 時にバイナリの整合性検証を提供している可能性がある。まずドキュメントを確認。

### 方法2: カスタム検証

`build.rs` でダウンロード後に SHA-256 ハッシュを計算し、事前定義されたハッシュと照合。

```rust
// build.rs
fn verify_onnx_binary(path: &Path) -> bool {
    let bytes = std::fs::read(path).unwrap();
    let hash = sha2::Sha256::digest(&bytes);
    hash.as_slice() == EXPECTED_HASH
}
```

### 方法3: 純粋にortに任せる

`ort` crate の `download-binaries` が既に検証を行っている場合は、追加の検証は不要。その場合は PBI をクローズ。

## 初期調査が必要

- `ort` crate の `download-binaries` の実装を確認
- ダウンロード先URLと検証メカニズムを調査
- 実際のリスク評価
