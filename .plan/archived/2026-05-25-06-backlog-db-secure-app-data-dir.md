# PBI: DB をセキュアなアプリデータ領域に自動配置

## ユーザーストーリー
個人ノートをインデックスするユーザーとして、DB ファイルを安全な場所に自動で作成してほしい、なぜなら chmod 600 の手動設定はユーザーの知識に依存した危うい設計であり Windows では通用しないから

## ビジネス価値
- DB ファイルがデフォルトで OS のセキュアなアプリデータ領域に作成される
- Windows / macOS / Linux 全プラットフォームでデフォルト安全

## BDD 受け入れシナリオ

```gherkin
Scenario: 初回セットアップで DB がセキュアな場所に作成される
  Given DB パスを設定していない状態で初回 chart を実行する
  When ユーザーが `shiotsuchi chart` を実行する
  Then DB ファイルが XDG_DATA_HOME（Linux）/ AppData（Windows）/ Application Support（macOS）に作成される

Scenario: 明示的に DB パスを指定した場合はそちらを使う
  Given config.toml に db_path を指定している
  When ユーザーが `shiotsuchi chart` を実行する
  Then 指定した DB パスが使われる
```

## 受け入れ基準
- [ ] デフォルト DB パスが OS のセキュアなアプリデータ領域になる
- [ ] `dirs` クレート等で OS 別パスを解決する
- [ ] 明示的 db_path 指定は引き続き有効

## 見積もり
2 ポイント

## 技術的考慮事項
- `dirs` または `directories` クレートを使用
- 既存ユーザーの DB 移行パスを案内するメッセージを表示

---

## ⚠️ 実装者向け注記

### 着手前の調査

```bash
# デフォルト DB パスの現状実装を確認する
grep -rn "db_path\|default_db_path\|dirs::" core/src/ cli/src/ | head -30
cat core/src/paths.rs 2>/dev/null || grep -rn "fn default_db_path" core/src/
```

`core/src/paths.rs` や `cli/src/config.rs` に `default_db_path()` 関数があるはず。  
`dirs = "6"` は既に `core/Cargo.toml` の依存に含まれている。

### 実装手順

1. **`default_db_path()` の現状実装を確認する**  
   既に `dirs::data_dir()` を使っている可能性がある。その場合はこの PBI はほぼ完了。

2. **未実装なら `core/src/paths.rs` を修正する**：
   ```rust
   pub fn default_db_path() -> PathBuf {
       dirs::data_dir()
           .unwrap_or_else(|| PathBuf::from("."))
           .join("shiotsuchi")
           .join("db.sqlite3")
   }
   ```
   プラットフォーム別の解決先：
   - macOS: `~/Library/Application Support/shiotsuchi/db.sqlite3`
   - Linux: `~/.local/share/shiotsuchi/db.sqlite3`
   - Windows: `%APPDATA%\shiotsuchi\db.sqlite3`

3. **DB ファイルを作成する際にディレクトリも自動作成する**  
   `fs::create_dir_all(db_path.parent().unwrap())` を DB open 前に実行。

### 落とし穴

- 既存ユーザーがカレントディレクトリの `db.sqlite3` を使っている場合、パスが変わると混乱する。  
  初回起動時に「DB パスが変わりました。移行は...」という案内を出すこと。
- `dirs::data_dir()` が `None` を返す環境（CI 等）でもクラッシュしないようにフォールバックを入れる。

## Definition of Done
- [ ] 全プラットフォームでデフォルトパスが適切に解決される
- [ ] コードレビュー完了
