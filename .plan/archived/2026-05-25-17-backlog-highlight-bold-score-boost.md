# PBI: ハイライト・太字テキストのスコアブースト

## ユーザーストーリー
ノートに `==ハイライト==` や **太字** を多用するユーザーとして、強調した部分にマッチした場合に検索順位を上げたい、なぜなら強調箇所はそのノートで特に重要な情報だから

## ビジネス価値
- ユーザーの意図（強調 = 重要）を検索ランキングに反映
- より関連性の高いノートが上位に来る

## BDD 受け入れシナリオ

```gherkin
Scenario: ハイライト内のキーワードにマッチするとスコアが高い
  Given ノート A に "==プロジェクト==" があり、ノート B に本文中に "プロジェクト" がある
  When ユーザーが `shiotsuchi dive "プロジェクト"` を実行する
  Then ノート A がノート B より上位に表示される

Scenario: ハイライトなしのノートも検索対象になる
  When ユーザーが `shiotsuchi dive "検索語"` を実行する
  Then ハイライトなしのノートも通常スコアで返される
```

## 受け入れ基準
- [x] インデックス時に `==text==` と `**text**` を検出してマーカーを付与する
- [x] 検索スコア計算時にハイライト部分マッチを追加ブーストする
- [x] ブースト倍率を設定で変更できる

## 見積もり
3 ポイント

## 技術的考慮事項
- 影響ファイル: `core/src/indexer.rs`、`core/src/search.rs`
- FTS5 の `rank` カスタマイズまたは別カラムでマーカー管理

---

## ⚠️ 実装者向け注記

### 現状確認

```bash
grep -n "highlight\|bold\|==\|\*\*\|emphasized" core/src/indexer.rs core/src/chunker.rs | head -20
```

### 実装方針

FTS5 の `rank` 関数をカスタマイズするのは複雑。以下の**後処理アプローチ**を推奨する：

1. **インデックス時にハイライト・太字テキストを抽出してメタデータに格納する**  
   `chunks` テーブルに `emphasized_content TEXT` カラムを追加する：
   ```rust
   fn extract_emphasized(markdown: &str) -> Vec<String> {
       // ==text== と **text** を正規表現で抽出
       let re = Regex::new(r"==([^=]+)==|\*\*([^*]+)\*\*").unwrap();
       // ...
   }
   ```

2. **検索後に emphasized_content との一致をスコアに加算する**  
   FTS5 の BM25 スコアに加算ボーナスを付ける：
   ```rust
   if result.emphasized_content.contains(&query_token) {
       score *= 0.8;  // FTS スコアは低いほど良いので乗算で引く
   }
   ```

### 落とし穴

- FTS5 仮想テーブルへの `ALTER TABLE ADD COLUMN` はできない。チャンクの強調情報は別テーブル（`chunks_meta`）に格納するか、`chunks` テーブルを再作成するマイグレーションが必要。
- `**太字**` と `*イタリック*` を混同しないよう正規表現に注意する（`\*\*` の二重アスタリスクのみを対象にする）。
- `regex` クレートを `core/Cargo.toml` に追加する必要がある。

## Definition of Done
- [x] スコアブーストのテストがパスする
- [x] コードレビュー完了
