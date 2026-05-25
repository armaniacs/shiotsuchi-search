# PBI: mtime + size 二段階スキャンによる高速インクリメンタル判定

## ユーザーストーリー
大規模ノートVaultを管理するユーザーとして、インデックス更新コマンドを高速に実行したい、なぜなら数万ファイル規模では毎回 SHA-256 全計算を待つのが苦痛だから

## ビジネス価値
- インデックス更新コマンド（`chart`）の実行時間を大幅短縮（目安: 未変更ファイルはハッシュ計算をスキップし I/O を削減）
- ユーザーが「更新が遅い」と感じるフラストレーションを解消
- 数万ファイル規模での実用性を確保

## BDD 受け入れシナリオ

```gherkin
Scenario: 変更のないファイルはハッシュ計算をスキップする
  Given ノートVaultに1000ファイルが存在し、インデックスが最新の状態である
  When ユーザーが `shiotsuchi chart` を実行する
  Then mtime と file_size が一致するファイルは SHA-256 計算をスキップする
  And インデックスの内容は変化しない

Scenario: 変更されたファイルのみ再インデックスする
  Given インデックス済みのVaultで、3ファイルが更新されている
  When ユーザーが `shiotsuchi chart` を実行する
  Then mtime または file_size が変化した3ファイルのみ SHA-256 再計算とトークナイズが行われる
  And 残りのファイルはスキップされる

Scenario: 既存DBからのマイグレーション
  Given mtime/file_size カラムを持たない旧バージョンのDBが存在する
  When ユーザーが `shiotsuchi chart` を初回実行する
  Then DBスキーマが自動マイグレーションされ mtime/file_size カラムが追加される
  And エラーなくインデックス処理が完了する
```

## 受け入れ基準
- [ ] `notes_meta` テーブルに `file_size INTEGER` と `mtime INTEGER` カラムが追加される
- [ ] スキャン時に mtime + size が一致するファイルは SHA-256 計算をスキップする
- [ ] mtime + size が変化したファイルのみ SHA-256 再計算 → 差分があればトークナイズ・DB更新
- [ ] 旧スキーマからの自動マイグレーションが動作する
- [ ] 既存テストがすべてパスする

## テスト戦略（t_wada スタイル）

### E2E テスト
- `chart` コマンド実行後、変更ファイルのみが再インデックスされることを確認

### 統合テスト
- `notes_meta` スキーママイグレーションの動作検証
- mtime/size キャッシュヒット時にDB更新が発生しないことを検証

### 単体テスト
- `mtime + size` 変化なし → ハッシュ計算スキップのロジック
- `mtime` のみ変化 → ハッシュ再計算が走るロジック
- ハッシュ一致 → DB更新スキップのロジック
- 旧スキーマ検出 → マイグレーション実行のロジック

## 実装アプローチ
- **Outside-In**: E2E → 統合 → 単体の順でテストを先に書く
- **Red-Green-Refactor**: 各レイヤーで TDD サイクルを適用

## 見積もり
3 ポイント（要チームでの見積もり）

## 技術的考慮事項
- 影響ファイル: `core/src/db.rs`（スキーマ・クエリ）、`core/src/indexer.rs`（スキャンロジック）
- `mtime` は Unix タイムスタンプ（秒）で格納
- マイグレーションは `ALTER TABLE notes_meta ADD COLUMN` で対応
- 依存関係: なし

---

## ⚠️ 実装者向け注記（ジュニア開発者必読）

### 現状コードの確認（着手前に必ず読むこと）

**この機能は既に実装されています。**

`core/src/indexer.rs` の `index_file_with_embedder` 関数（236行目付近）を見ると、
mtime の fast-path チェックが既に実装されています：

```rust
let mtime = file_mtime(file_path);
if let Ok(Some(cached_mtime)) = db.cached_mtime(vault_name, relative_path) {
    if cached_mtime == mtime {
        // ... スキップ
    }
}
let hash = sha256_hex(&content);
if let Ok(Some(cached)) = db.cached_hash(...) { ... }  // ← hash も二段チェック
```

`core/src/db.rs` の `file_cache` テーブルにも `mtime INTEGER` が存在します（182行目付近）。

### このPBIで実際にやること

**着手前に `cargo test -p shiotsuchi-core` を実行し、既存テストが全て通ることを確認する。**

その上で以下を評価・対応する：

1. **`file_size` が未実装かどうかを確認する**  
   `grep -n "file_size" core/src/db.rs` を実行して結果を確認。  
   もし存在しなければ `file_cache` テーブルに `file_size INTEGER` を追加してさらに高速化できる。  
   （現状は mtime のみの fast-path）

2. **既存の `test_index_file_skips_via_mtime_fast_path` テストを読む**  
   `core/src/indexer.rs` の 715行目付近。このテストが何を検証しているか理解してから作業する。

3. **改善の余地があれば実装し、なければ「既に実装済み」と報告する**  
   file_size の追加は任意。既存実装で十分なら PBI をクローズして良い。

### 落とし穴

- `mtime` はミリ秒精度で格納されている（秒ではない）。コードを読む際に混同しないこと。
- `file_cache` と `notes_meta` は別テーブル。混同注意。インクリメンタル判定は `file_cache` 側。
- Rayon による並列処理あり。スレッドセーフな操作のみ行うこと。

## Definition of Done
- [ ] 全 BDD シナリオが自動テストとして実装されパスする
- [ ] 既存テストカバレッジが維持される
- [ ] コードレビュー完了
- [ ] リファクタリング完了（グリーン後）
- [ ] `ref/architecture.md` のデータモデル説明を更新済み
