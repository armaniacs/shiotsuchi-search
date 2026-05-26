# PBI: Semantic 検索を Cargo feature flag でオプション化

## ユーザーストーリー
軽量なキーワード検索だけを使いたいユーザーとして、ONNX Runtime なしの小さなバイナリでインストールしたい、なぜなら数十 MB のバイナリは CI/CD や低スペック環境に不向きだから

## ビジネス価値
- バイナリサイズを大幅削減（`--no-default-features` ビルドで ort を除外）
- MCP サーバーの常駐メモリ使用量を削減
- FTS5 キーワード検索のみ使うユーザーへの選択肢を提供

## BDD 受け入れシナリオ

```gherkin
Scenario: デフォルトビルドでセマンティック検索が使える
  Given cargo build でビルドする（default features）
  When ユーザーが `shiotsuchi dive --semantic "検索語"` を実行する
  Then セマンティック検索結果が返される

Scenario: 軽量ビルドでキーワード検索のみ使える
  Given cargo build --no-default-features でビルドする
  When ユーザーが `shiotsuchi dive "検索語"` を実行する
  Then FTS5 キーワード検索結果が返される

Scenario: 軽量ビルドでセマンティック検索を試みるとわかりやすいエラーが出る
  Given cargo build --no-default-features でビルドする
  When ユーザーが `shiotsuchi dive --semantic "検索語"` を実行する
  Then "semantic feature is not enabled" 旨のエラーメッセージが表示される
  And 終了コード 1 で終了する
```

## 受け入れ基準
- [x] `Cargo.toml` に feature `semantic` を定義（デフォルト: enabled）
- [x] `ort` 依存・埋め込みモデル・ベクトル検索コードが `#[cfg(feature = "semantic")]` で囲まれる
- [x] `--no-default-features` ビルドでコンパイルエラーが出ない
- [x] semantic 無効時に `--semantic` フラグを使うと明確なエラーメッセージが返る
- [x] README にビルドオプションの説明が追記される

## テスト戦略（t_wada スタイル）

### E2E テスト
- デフォルトビルドで `dive --semantic` が結果を返すことを確認
- `--no-default-features` ビルドで `dive "query"` が FTS5 結果を返すことを確認

### 統合テスト
- feature フラグ切り替えによるコードパス分岐の動作検証
- semantic 無効時の CLI エラーハンドリング

### 単体テスト
- `#[cfg(feature = "semantic")]` の有無による関数呼び出し分岐
- semantic 無効時のエラー生成ロジック

## 実装アプローチ
- **Outside-In**: E2E → 統合 → 単体の順でテストを先に書く
- **Red-Green-Refactor**: 各レイヤーで TDD サイクルを適用

## 見積もり
5 ポイント（要チームでの見積もり）

## 技術的考慮事項
- 影響ファイル: `Cargo.toml`（workspace + 各クレート）、`core/src/lib.rs`、`core/src/search.rs`、`core/src/tokenizer.rs`、`cli/src/main.rs`、`mcp/src/handler.rs`
- `build.rs` の SHIOTSUCHI_MODEL_PATH 埋め込みも feature gate 対象
- 依存関係: なし（他 PBI と独立）

---

## ⚠️ 実装者向け注記（ジュニア開発者必読）

### 現状コードの確認

`core/Cargo.toml` を見ると現状の feature 定義は：
```toml
[features]
default = ["watcher", "async-index"]
watcher = ["dep:notify"]
async-index = ["dep:tokio"]
```

`ort` は feature gate されておらず、常に `[dependencies]` に含まれています。

`core/src/lib.rs` に `pub mod embedder;` がある。  
`core/src/embedder.rs`（または類似ファイル）が `ort` を使っている箇所を探すこと。

### 実装手順

1. **`ort` を使っているファイルをすべて洗い出す**
   ```bash
   grep -rn "ort::\|use ort\|embedder" core/src/
   ```

2. **`core/Cargo.toml` に feature を追加する**
   ```toml
   [features]
   default = ["watcher", "async-index", "semantic"]
   semantic = ["dep:ort", "dep:tokenizers"]
   ```
   `ort` と `tokenizers` を `optional = true` にする。

3. **`core/src/embedder.rs` 全体を `#[cfg(feature = "semantic")]` で囲む**
   ```rust
   #[cfg(feature = "semantic")]
   pub struct Embedder { ... }
   ```

4. **`core/src/lib.rs` の `pub mod embedder;` を条件付きにする**
   ```rust
   #[cfg(feature = "semantic")]
   pub mod embedder;
   ```

5. **`core/src/search.rs` の `embedder: Option<&Embedder>` 引数を条件コンパイル対応にする**  
   `semantic` feature が無効なら `embedder` は常に `None` として扱う。

6. **`core/build.rs` を確認する**  
   `SHIOTSUCHI_MODEL_PATH` の埋め込み処理も `#[cfg(feature = "semantic")]` 対応が必要。

7. **`cargo build --no-default-features` が通ることを確認する**

### 落とし穴

- `cli/src/main.rs` の `Commands::Dive` が `--semantic` や `--mode vec` オプションを持っている場合、feature 無効時のエラーハンドリングが必要。
- `mcp/src/handler.rs` にも embedder 参照がある可能性が高い。全て確認すること。
- `ort` は build time に ONNX バイナリをダウンロードする（`download-binaries` feature）。feature 無効時はこのダウンロードも発生しないことを確認する。
- テストの一部が embedder を使っている場合、`#[cfg(feature = "semantic")]` で囲む。

## Definition of Done
- [ ] 全 BDD シナリオが自動テストとして実装されパスする
- [ ] デフォルト・軽量ビルド両方で CI が通る
- [ ] コードレビュー完了
- [ ] リファクタリング完了（グリーン後）
- [ ] README のインストールセクションにビルドオプション追記済み
