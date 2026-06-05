# PBI-52: ONNX Runtime バイナリ SHA 検証

**発端:** Supply Chain & Dependency Sentinel (スコア70)
**影響:** `ort` の `download-binaries` feature によりビルド時に ONNX Runtime バイナリが自動ダウンロードされるが、ハッシュ比較処理が実装されていないため整合性検証が行われない。**ただし、この問題は shiotsuchi 側ではなく `ort`/`ort-sys` クレート側のバグであり、`ort-sys-2.0.0-rc.12` の `dist.txt` には期待ハッシュが存在するものの `VerifyReader` がダウンロードフローに接続されていない。**
**対処:** アップストリーム（`ort`/`ort-sys`）に issue/PR を提出する。shiotsuchi 側の `build.rs` への追加実装は不要（ビルド修正は不可能）。
**工数:** 調査完了。stable リリース待ち。
**状態:** ブロック（`ort 2.0.0` stable リリース待ち / upstream PR マージ待ち）

> `[patch]` 適用は **不可能**：ort-sys main ブランチの修正は `ort` rc.12 と API 互換性がないため、`ort-sys` だけ main に差し替えてもビルドエラーになる。ort 自体の stable リリースを待つしかない。

## 調査結果（2026-06-06）

### 事実確認

- **ort-sys-2.0.0-rc.12（crates.io）**: `VerifyReader` 構造体と `hex_str_to_bytes` 関数が実装されて**いるが未使用**（バグ）
- **ort-sys main ブランチ（HEAD）**: `build/main.rs:108` で `VerifyReader::new(reader)` を呼び出し、`build/main.rs:130-139` で計算ハッシュと `dist.hash` の比較を実行 → **修正済み**
- `dist.txt`: 各プラットフォームごとの expected hash が含まれている（確認済み）

- **shiotsuchi 側の関連事項**:
  - `core/Cargo.toml:39`: `ort = "2.0.0-rc.12"`、features = `["std", "download-binaries", "tls-rustls"]`
  - `core/src/constants.rs:12`: `EXPECTED_MODEL_SHA256 = ""` — **ランタイムで読み込む `.onnx` モデルファイルのハッシュ検証も未設定**

### 結論

PBI-52 の原型 claim は事実：`ort 2.0.0-rc.12` の `download-binaries` は SHA-256 検証を**行っていない**。

ただし、`pykeio/ort` main ブランチには既に修正がコミット済み。
**「ort を main ブランチに更新する」ことが最適解**である。

## 実装方針（main ブランチ採用）

### 方法A: Git ブランチ参照に切り替え（推奨）

```toml
# core/Cargo.toml
ort = { git = "https://github.com/pykeio/ort", branch = "main", default-features = false, features = ["std", "download-binaries", "tls-rustls"], optional = true }
```

**利点:**
- 修正が適用された最新コードを入手
- shiotsuchi 側の追加実装不要

**欠点:**
- コミットハッシュ固定でないため再現性が低い
- main ブランチの breaking change リスク

### 方法B: 公式 stable リリース待ち（安全）

crates.io に `ort 2.0.0`（または `2.0.0-rc.13`）が公開されるまで待機。

- リリースノートに "Build attestations" の記載があるため、間もなく stable 化の可能性あり
- 再現性が保たれる

### 方法C: Cargo.lock の ort-sys を Git ソースで上書き

```toml
# core/Cargo.toml は rc.12 のまま
# Cargo.lock で ort-sys を git ソースに patch
[patch."https://github.com/pykeio/ort"]
ort-sys = { git = "https://github.com/pykeio/ort", branch = "main" }
```

- 最小差分で修正導入
- 他 crate への影響を局所化

## 推奨: 方法B を優先、方法C をフォールバック

1. `ort` の stable リリース（`2.0.0` または `2.0.0-rc.13`）を 1〜2 週間待つ
2. リリースされない場合、方法C で `Cargo.lock patch` を適用
3. 最終手段として方法A

## BDD シナリオ（main ブランチ採用版）

```gherkin
Scenario: ort main ブランドの verify 修正を導入する
  Given ort を main ブランチ参照に更新した
  When cargo build を実行する
  Then ONNX Runtime バイナリの SHA-256 が dist.txt の期待値と一致する
  And ビルドが成功する

Scenario:  main ブランチでハッシュ不一致が検出される
  Given ort main が VerifyReader を使用している
  When 改ざんされたバイナリがダウンロードされる
  Then cargo build が失敗する
  And エラーメッセージにハッシュ不一致が表示される
```

## 関連ファイル

- `core/Cargo.toml:39`: ort dependency definition（更新対象）
- `core/src/constants.rs:5-12`: `EXPECTED_MODEL_SHA256` (ランタイムモデル検証用、現在空文字)
- `core/src/embedder.rs:612`: `verify_model_hash()` — ランタイム検証関数（SHA-256 確認）
- ort upstream fix: `pykeio/ort` main `ort-sys/build/main.rs:108,130-139`

## 参考: ort-sys の main ブランチ修正箇所

## 調査結果（2026-06-06）

### 事実確認

- **ort-sys-2.0.0-rc.12 ソース調査**:
  - `build/download/verify.rs`: `VerifyReader` 構造体と `hex_str_to_bytes` 関数が実装されて**いるが未使用**
  - `build/download/mod.rs`: `verify` モジュールをインポートしているが、`download_file()` 内で `VerifyReader` をラップしていない
  - `build/download/resolve.rs`: `Distribution { url, hash }` 構造体。`resolve_dist()` で `dist.hash` は正しく取得されるが、`build/main.rs` でハッシュ比較が行われていない
  - `dist.txt`: 各プラットフォームごとの expected hash が含まれている

- **shiotsuchi 側の関連事項**:
  - `core/Cargo.toml:39`: `ort = "2.0.0-rc.12"`、features = `["std", "download-binaries", "tls-rustls"]`
  - `core/src/constants.rs:12`: `EXPECTED_MODEL_SHA256 = ""` — **ランタイムで読み込む `.onnx` モデルファイルのハッシュ検証も未設定**

### 結論

PBI-52 の原型 claim は事実：`ort` の `download-binaries` は SHA-256 検証を**行っていない**。

ただし、これは `ort`/`ort-sys` クレート側の問題であり、修正は shiotsuchi の `build.rs` ではなく `ort-sys`  upstream で行うべき。

**方法3（純粋に ort に任せる）を推奨**：`ort-sys` 側に issue を提出し `VerifyReader` + `dist.hash` の比較を有効化するよう依頼。shiotsuchi 側の追加実装は不要。

### 二次的発見（別途検討推奨）

`core/src/constants.rs` の `EXPECTED_MODEL_SHA256` は空文字列のままである。

- これは **ランタイムで読み込む `model.onnx`（実データのニューラルネットワーク重み）** の検証用定数
- `EXPECTED_MODEL_SHA256` に実際のハッシュ値を設定することで、ランタイムでのモデル読み込み時に整合性確認が可能
- 別 PBI またはタスクとして検討

## 背景

- `ort` crate は `download-binaries` feature でビルド時に ONNX Runtime バイナリをダウンロード
- 現状は検証なしでダウンロードされたバイナリを使用
- **ただし、`ort-sys-2.0.0-rc.12` には検証用コード（`VerifyReader`）が存在するものの未接続**
- サプライチェーン攻撃のリスクは理論的に存在

## BDD 受け入れシナリオ（調査完了）

```gherkin
Scenario: ort-sys の download-binaries が正当に検証する（アップストリーム修正後の期待動作）
  Given ort-sys が dist.txt の hash と VerifyReader を正しく使用している
  When ビルドを実行する
  Then ダウンロードされたバイナリのハッシュが dist.txt の期待値と一致する
  And ビルドが成功する

Scenario:  ort-sys の download-binaries がハッシュ不一致を検出する（アップストリーム修正後の期待動作）
  Given ort-sys がハッシュ検証を実装している
  When 破損/改ざんされたバイナリがダウンロードされる
  Then ビルドが失敗する

Scenario: ort-sys が検証を実装しない場合の回避策
  Given ort-sys に検証バグが存在する
  When ユーザーが安全な環境を求める
  Then ORT_LIB_LOCATION 環境変数でシステム導入の ONNX Runtime を指定できる
  And TLS 保護されたダウンロードで中間者攻撃を緩和できる
```

## TDD アプローチ（調査完了）

### Phase 1: 調査 ✅ 完了

1. **ort crate のドキュメント確認** ✅: `ort-sys-2.0.0-rc.12` のソースを直接確認
2. **リスク評価** ✅: `VerifyReader` 未使用のバグを特定
3. **方針決定** ✅: shiotsuchi 側実装不要 → アップストリーム issue 提出

### Phase 2: 実装方針（shiotsuchi 側は不要）

方法1の「ort crate の標準検証」は **部分的に正しい**：`ort` には検証機構があるが、バグで無効化されている。

**推奨アクション:**
1. `ort`/`ort-sys` に issue/PR を提出（`VerifyReader` をダウンロードフローに接続）
2. replace #ifndef __EMSCRIPTEN__ のような Makefile ラッパーは不要。`ort-sys/build/main.rs` の修正を提案

### 参考：ort-sys の想定される修正ポイント

```rust
// ort-sys/build/main.rs の該当箇所（概念例）
let dist = match download::resolve_dist() {
    Ok(dist) => dist,
    Err(e) => { ... }
};
let dest = out_dir.join("onnxruntime").join(dist.artifact_name);
if dest.exists() { return; }

let verified_reader = match download::fetch_file(dist.url) {
    Ok(reader) => download::VerifyReader::new(reader),
    Err(e) => { ... }
};

// 展開
download::extract_tgz(&mut verified_reader, &temp_extract_dir)?;

// ハッシュ検証
let (calculated_hash, _) = verified_reader.finalize()?;
if calculated_hash[..] != download::hex_str_to_bytes(dist.hash) {
    panic!(...);
}
```

## PBI クローズ理由

| 項目 | 内容 |
|------|------|
| Claim は事実 | `ort` の `download-binaries` は SHA-256 検証を **行っていない**（バグ） |
| 修正場所 | shiotsuchi **ではない**。`ort-sys` upstream |
| shiotsuchi 側対応 | **不要** — 方法3 を適用。`ORT_LIB_LOCATION` fallback で代替可能 |
| TLS | `tls-rustls` 有効化により中間者攻撃は防止 |
| 追加検討 | `EXPECTED_MODEL_SHA256` の設定（ランタイム `.onnx` モデル検証）は別途 |

## 関連ファイル

- `core/Cargo.toml:39`: ort dependency definition
- `core/src/constants.rs:5-12`: `EXPECTED_MODEL_SHA256` (ランタイムモデル検証用、現在空文字)
- `core/src/embedder.rs:612`: `verify_model_hash()` — ランタイム検証関数（SHA-256 確認）
