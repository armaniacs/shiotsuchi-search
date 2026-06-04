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
