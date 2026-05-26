# PBI: 同義語管理 CLI と専用ファイル対応

TODO: WebとAIを駆使してシソーラスを作成する方法を考えて将来追記する

## ユーザーストーリー
検索管理者として、同義語辞書をコマンドラインから増減・一覧確認したい、なぜなら `config.toml` を直接編集するのはミスが多く、登録状況の把握が難しいから

## ビジネス価値
- 同義語辞書のメンテナンス性が向上し、継続的な検索精度改善が現実的な作業になる
- `config.toml` 編集ミス（TOML 構文エラー、値の型間違い）を防止する
- 現在の同義語設定をいつでも確認できる

## BDD 受け入れシナリオ

```gherkin
Scenario: 同義語を追加する
  Given 同義語辞書ファイルが存在する
  When ユーザーが `shiotsuchi synonym add "AWS" "Amazon Web Services"` を実行する
  Then 辞書に "AWS" → ["Amazon Web Services"] が追加される
  And 辞書ファイルが有効な TOML として保存される

Scenario: 同義語を一覧表示する
  Given 辞書に "AWS" → ["Amazon Web Services", "アマゾン"] が登録されている
  When ユーザーが `shiotsuchi synonym list` を実行する
  Then "AWS" の行と "Amazon Web Services", "アマゾン" が表示される

Scenario: 同義語を削除する
  Given 辞書に "AWS" → ["Amazon Web Services"] が登録されている
  When ユーザーが `shiotsuchi synonym remove "AWS"` を実行する
  Then 辞書から "AWS" エントリが削除される

Scenario: 専用ファイルがない場合は新規作成する
  Given 同義語辞書ファイルがまだ存在しない
  When ユーザーが `shiotsuchi synonym add "k8s" "Kubernetes"` を実行する
  Then 新しい辞書ファイルが作成される
  And 辞書に "k8s" → ["Kubernetes"] が保存される
```

## 受け入れ基準
- [ ] `shiotsuchi synonym add <word> <synonym>` コマンドで同義語を追加できる
- [ ] `shiotsuchi synonym list` コマンドで全エントリを表示できる
- [ ] `shiotsuchi synonym remove <word>` コマンドでエントリを削除できる
- [ ] 辞書は専用ファイル（`~/.config/shiotsuchi/thesaurus.toml`）に保存される
- [ ] 専用ファイルが存在しない場合、`synonym` コマンド実行時に自動生成される
- [ ] 専用ファイルの内容は起動時に `ShiotsuchiConfig.synonyms` にマージされる
- [ ] 重複追加の防止（同じ語＝同義語ペアの再追加は無視または警告）
- [ ] 存在しないエントリの削除はエラーメッセージを表示
- [ ] 辞書なしでも従来通り動作する

## 見積もり
5 ポイント

## 技術的考慮事項
- 影響ファイル: 新規 `cli/src/commands/synonym.rs`、`core/src/config.rs`
- 辞書ファイル: `~/.config/shiotsuchi/thesaurus.toml`
- `synonym` は `shiotsuchi` CLI のサブコマンドとして追加
- 新規ファイル作成時のパーミッションは 0o600（config.toml と同様）

---

## ⚠️ 実装者向け注記

### 前提

PBI-08（同義語展開）は完了済み。`core/src/search.rs` に `expand_synonyms()` が実装され、`ShiotsuchiConfig.synonyms` が設定から読み込まれる。現状は `config.toml` の `[synonyms]` セクションのみ対応。

### 実装手順

1. **`cli/src/commands/synonym.rs` を新規作成する**
   - `SynonymArgs`（clap のサブコマンド定義）
   - `run_synonym(args, thesaurus_path)` 関数
   - サブコマンド: `Add`, `Remove`, `List`

2. **`core/src/config.rs` に thesaurus ファイルパス解決を追加する**
   ```rust
   pub fn thesaurus_path() -> PathBuf {
       xdg_config_home().join("shiotsuchi").join("thesaurus.toml")
   }
   ```

3. **専用ファイルの読み込み・マージロジックを `ShiotsuchiConfig` に追加する**
   ```rust
   // ShiotsuchiConfig.load() 内で:
   let thesaurus_path = thesaurus_path();
   if thesaurus_path.exists() {
       if let Ok(thesaurus_cfg) = Config::builder()
           .add_source(File::from(&thesaurus_path))
           .build()
           .and_then(|c| c.try_deserialize::<HashMap<String, Vec<String>>>())
       {
           // config.toml の synonyms より専用ファイルを優先マージ（または警告表示）
           // 設計判断: 専用ファイルが設定より優先されるか、マージされるかを決める
       }
   }
   ```

4. **`cli/src/main.rs` に `Commands::Synonym` を追加する**
   ```rust
   #[command(subcommand)]
   enum SynonymCommands {
       Add { word: String, synonym: String },
       Remove { word: String },
       List,
   }
   ```

5. **エッジケース対応**
   - 空の `synonym list` → "登録されていません" メッセージ
   - `synonym add` の値にスペースが含まれる場合 → クォーテーション必須
   - 既存エントリへの追加（同一キーへの別同義語追加）→ 追記動作にするかエラーにするか

### ファイル構成

```
cli/src/
├── commands/
│   ├── synonym.rs       # 新規
│   └── ...              # 既存
core/src/
└── config.rs            # thesaurus_path() 追加、マージロジック追加
```

### 落とし穴

- `config.toml` と `thesaurus.toml` の両方に同じキーがある場合の優先順位。専用ファイルを優先すると、ユーザーが `config.toml` に書いた設定が無視される。設計方針を明確にすること。
- TOML シリアライズ時のキーソート順。同義語追加のたびにファイルが書き換わるので、順序が安定していること（BTreeMap の使用など）。
- `ShiotsuchiConfig::load()` は現状 fallible ではない（`unwrap_or_else` でエラーを握りつぶしている）。同義語ファイルの load 失敗は致命的ではないので、同様に警告＋無視でよい。

## Definition of Done
- [ ] 同義語追加・削除・一覧のテストがパスする
- [ ] 専用ファイルの読み込み・マージのテストがパスする
- [ ] コードレビュー完了
