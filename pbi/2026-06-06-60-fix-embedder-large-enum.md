# PBI-60: EmbedderBackend の large_enum_variant 修正 (DEV-64)

## ユーザーストーリー

開発者として、EmbedderBackend Enum のメモリレイアウトが最適化されていてほしい、なぜなら現状の Onnx 変種が 1200 バイトもある一方 Api 変種は 168 バイトで、このサイズ差がスタック上の無駄な領域確保につながっているから

## ビジネス価値

- Clippy 警告 `large_enum_variant` を解消する
- `EmbedderBackend` のメモリフットプリントを削減する（Onnx 変種の `Tokenizer` を Box 化）
- デフォルトビルドの全ユーザーに恩恵がある軽微なパフォーマンス改善

## 現状

```rust
enum EmbedderBackend {
    Onnx {
        session: RefCell<Session>,  // 大
        tokenizer: Tokenizer,       // ← Box 化候補
        model_id: String,
    },  // ~1200 bytes
    Api {
        client: ApiClient,
        model_id: String,
    },  // ~168 bytes
    #[cfg(test)]
    TestVec(Vec<f32>),
}
```

## BDD 受け入れシナリオ

```gherkin
Scenario: Tokenizer を Box でラップする
  Given EmbedderBackend::Onnx の tokenizer フィールドが Tokenizer 値を直接保持している
  When  そのフィールドを Box<Tokenizer> に変更する
  Then  コンパイルが通る
  And   既存の embedded テストがグリーン

Scenario: large_enum_variant 警告が消える
  Given cargo clippy が large_enum_variant を報告している
  When  Tokenizer を Box 化する
  Then  cargo clippy がその警告を報告しなくなる
```

## 受け入れ基準

- [ ] `EmbedderBackend::Onnx` の `tokenizer` フィールドが `Box<Tokenizer>` になっている
- [ ] `cargo clippy -p shiotsuchi-core` に `large_enum_variant` 警告が 0 件
- [ ] `cargo test -p shiotsuchi-core` がグリーン
- [ ] embedding の出力が変更前と一致する（リグレッションなし）

## テスト戦略（t_wada スタイル）

### 単体テスト
- `embedder::tests::*` — 既存テストが embedding パイプライン全体をカバーしている。Box 化による動作変更はないため、既存テストのグリーンが十分な検証となる。

### 検証コマンド

```bash
cargo build -p shiotsuchi-core
cargo clippy -p shiotsuchi-core
cargo test -p shiotsuchi-core  # embedder tests を含む
```

## 実装アプローチ

- **最小変更**: `core/src/embedder.rs` の1行のみ変更

```rust
// 変更前
tokenizer: Tokenizer,

// 変更後
tokenizer: Box<Tokenizer>,
```

## 見積もり

0.5時間（1行の変更 + コンパイル確認）

## 技術的考慮事項

- `Box<Tokenizer>` への変更は `Tokenizer` の `Deref` 実装があるため、すべての `.` アクセスは透過的に動作する。呼び出し元のコード変更は不要
- `EmbedderBackend` は `derive(Debug)` しており `Box<Tokenizer>` も `Debug` を実装しているため影響なし

## 実装者向け注記

### 現状コードの確認

```bash
grep -n "tokenizer: Tokenizer" core/src/embedder.rs
cargo clippy -p shiotsuchi-core 2>&1 | grep large_enum_variant
```

### 実装手順

1. `core/src/embedder.rs` の `tokenizer: Tokenizer` → `tokenizer: Box<Tokenizer>` に変更
2. Onnx 変種を生成している箇所を確認し、`Tokenizer` を `Box::new(...)` でラップする
3. `cargo build -p shiotsuchi-core` でコンパイル確認
4. `cargo clippy -p shiotsuchi-core` で large_enum_variant が消えたことを確認
5. `cargo test -p shiotsuchi-core` で全テストグリーン確認

### 落とし穴

- `Box::new(tokenizer_instance)` の追加箇所を見逃さないこと。`EmbedderBackend::Onnx { tokenizer: Box::new(tok), ... }` の形式が必要

## Definition of Done

- [ ] `cargo clippy -p shiotsuchi-core` が `large_enum_variant` を報告しない
- [ ] `cargo test -p shiotsuchi-core` が全テストグリーン
