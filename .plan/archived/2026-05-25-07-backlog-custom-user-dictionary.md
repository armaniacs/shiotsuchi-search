# PBI: Vaporetto カスタムユーザー辞書対応

## ユーザーストーリー
専門用語や造語を多用するユーザーとして、独自の辞書を追加してトークナイズをチューニングしたい、なぜなら一般辞書にない「Claude」「k8s」「プロジェクトコードネーム」などが正しく分割されず検索精度が頭打ちになるから

## ビジネス価値
- ユーザー固有の専門用語・新語の検索精度向上
- 使えば使うほど改善される体験を提供

## BDD 受け入れシナリオ

```gherkin
Scenario: カスタム辞書の単語がトークンとして扱われる
  Given config.toml にカスタム辞書ファイルのパスを設定している
  And 辞書に "Claude" という単語が登録されている
  When "Claude について" というノートをインデックスする
  Then "Claude" が1つのトークンとして分割される

Scenario: カスタム辞書がない場合はデフォルト動作
  Given カスタム辞書を設定していない
  When ノートをインデックスする
  Then デフォルトモデルのみでトークナイズされる
```

## 受け入れ基準
- [ ] config.toml で辞書ファイルパスを指定できる
- [ ] 辞書ファイルの単語がトークナイズ時に優先される
- [ ] 辞書なしでも従来通り動作する

## 見積もり
5 ポイント

## 技術的考慮事項
- Vaporetto のユーザー辞書 API またはトークン後処理で対応
- 影響ファイル: `core/src/tokenizer.rs`、`cli/src/config.rs`

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
cat core/src/tokenizer.rs | head -100
grep -n "UserDictionary\|user_dict\|custom" core/src/tokenizer.rs
```

Vaporetto v0.6 のユーザー辞書 API を確認すること：  
https://docs.rs/vaporetto/0.6/vaporetto/struct.KyteaModel.html  
（ユーザー辞書のサポート状況はバージョンによって異なる）

### 実装方針

**Vaporetto がユーザー辞書をサポートしていない場合**: トークン後処理アプローチを使う。

```rust
// カスタム辞書のエントリに完全一致するテキストが含まれていれば、
// そのトークンを分割せずに保持する後処理フィルタ
fn apply_user_dictionary(tokens: Vec<String>, dict: &[String]) -> Vec<String> {
    // ...
}
```

**設定ファイルへの追加**（`core/src/config.rs` の `IndexingConfig` に）：
```toml
[indexing]
user_dictionary = ["Claude", "k8s", "ChatGPT", "プロジェクトX"]
```

### 落とし穴

- Vaporetto は学習済みモデルベースのため、単語リストをそのまま追加することができない。  
  後処理（トークン化後にカスタム語を検出してマージ）が現実的なアプローチ。
- 長い複合語（「Amazon Web Services」など）の後処理マージは、トークン境界をまたぐため複雑になる。まずは単一語の辞書から始める。

## Definition of Done
- [ ] カスタム辞書でトークナイズが改善されるテストがパスする
- [ ] コードレビュー完了
