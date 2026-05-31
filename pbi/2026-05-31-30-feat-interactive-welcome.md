# PBI-30: サブコマンド未指定時にインタラクティブガイダンスを表示する

Linear: https://linear.app/armaniacs/issue/DEV-5/shiotsuchi-コマンドをtui対応にする

## ユーザーストーリー

shiotsuchi-search の新規ユーザーとして、`shiotsuchi` とだけ打ったときに、セットアップから使い方まで迷わない導線が欲しい。なぜなら、現状は clap のデフォルトエラーが表示されるだけで、どう操作を始めればいいのか分からないからだ。ガイダンスは単なる表示だけでなく、init → index → search の手順を実際に一緒に完了できるオンボーディング機能を含んで欲しい。インストール直後に `~/.config/shiotsuchi/config.toml` が存在しない場合は、対話的に設定ファイルを作成し、そのままインデックス、検索までシームレスに進めたい。

## ビジネス価値

- 新規ユーザーの「コマンドが分からない」離脱を防ぐ
- インストール直後に `shiotsuchi` と打つだけでセットアップ→インデックス→検索が完了する（Zero-to-search 体験）
- 3ステップのオンボーディングで、ユーザーが何も調べずに最初の検索結果にたどり着ける
- コマンド名を覚えていなくてもメニューから選択可能
- `--help` よりも親しみやすいファーストエクスペリエンス
- コマンドがカテゴリ別に整理され、初心者でも目的の操作を直感的に選べる

## 既実装確認

```bash
# サブコマンド必須の設定を確認
grep -n "subcommand" cli/src/main.rs

# dialoguer の使用パターンを確認
grep -rn "dialoguer" cli/src/

# 既存の「サブコマンド無し」ハンドリングを確認
grep -rn "subcommand_required\|no_subcommand\|command.is_none\|None.*command" cli/src/

# 設定ファイルのパス解決を確認
grep -n "default_config_path\|xdg_config_home" core/src/config.rs
```

**結果:**
- `#[command(subcommand)]` が `command: Commands` (非Optional) に設定されており、サブコマンドが必須（`cli/src/main.rs:74`）
- dialoguer は `init.rs` と `doctor.rs` で `ColorfulTheme` と共に使用済み
- サブコマンド未指定時のカスタムハンドリングは未実装
- `default_config_path()` は `~/.config/shiotsuchi/config.toml`（または `$XDG_CONFIG_HOME`）を返す（`core/src/config.rs:147-149`）
- `ShiotsuchiConfig::load()` は設定ファイル不在時もエラーにせずデフォルト設定で動作する（`core/src/config.rs:312-339`）

## BDD受け入れシナリオ

```gherkin
Scenario: インストール直後にオンボーディングを完了する（config 未存在 → init → index → search）
  Given shiotsuchi-search がインストールされている
  And   ~/.config/shiotsuchi/config.toml が存在しない
  When  ユーザーが `shiotsuchi` を実行する
  Then  ウェルカムバナーが表示される
  And   「🔰 オンボーディングを開始」がメニューの先頭に表示される
  And   init → index → search の3ステップがバナー内に表示される
  When  ユーザーが「オンボーディングを開始」を選択する
  Then  Step 1: 設定ファイル作成が開始される
  And   対話的に init が実行される（除外候補の選択等）
  And   設定ファイル作成後、自動的に Step 2 へ進むか確認される
  When  ユーザーが「次へ」を選択する
  Then  Step 2: インデックス作成が開始される
  And   ノートのインデックスが実行される
  And   インデックス完了後、自動的に Step 3 へ進むか確認される
  When  ユーザーが「次へ」を選択する
  Then  Step 3: 検索クエリの入力が促される
  When  ユーザーが検索クエリを入力する
  Then  検索結果が表示される
  And   「🎉 オンボーディング完了！」というメッセージが表示される
  And   通常のコマンド選択メニューに戻る

Scenario: 設定ファイルはあるがデータベースが未作成の状態でオンボーディング
  Given ~/.config/shiotsuchi/config.toml が存在する
  And   データベースファイルが存在しない
  When  ユーザーが `shiotsuchi` を実行する
  Then  ウェルカムバナーが表示される
  And   「⚡ オンボーディングを続ける」がメニューに表示される
  When  ユーザーが「オンボーディングを続ける」を選択する
  Then  Step 1 をスキップして Step 2（index）から開始される
  And   以降はフルオンボーディングと同じ流れで search まで進む

Scenario: 設定ファイルもデータベースも存在する状態で起動する
  Given ~/.config/shiotsuchi/config.toml が存在する
  And   データベースファイルが存在する
  When  ユーザーが `shiotsuchi` を実行する
  Then  直接ウェルカムバナーと選択メニューが表示される
  And   メニューに「🚀 クイックオンボーディング（再インデックス＋検索体験）」が含まれる
  And   バナー内にクイックスタートガイドが表示される

Scenario: メニューから search を選択して検索する
  Given 設定ファイルとデータベースが存在する
  When  ユーザーがメニューから "search" を選択する
  And   プロンプトに検索クエリを入力する
  Then  検索結果が表示される
  And   メニューに戻る

Scenario: メニューから init を選択して設定を作成する
  Given 設定ファイルが存在しない状態でメニューが表示されている
  When  ユーザーがメニューから "init" を選択する
  Then  init の対話的フローが開始される
  And   設定ファイル作成後、「次に index を実行してオンボーディングを続けますか？」と確認される
  When  ユーザーが「はい」を選択する
  Then  Step 2（index）に進む
  When  ユーザーが「いいえ」を選択する
  Then  メニューに戻る

Scenario: 既存のサブコマンド指定は従来通り動作する
  Given ユーザーが `shiotsuchi index` のようにサブコマンドを指定する
  When  コマンドが実行される
  Then  メニューやオンボーディングを経由せず従来通りコマンドが実行される
  And   既存の全引数（--notes-dir, --db-path, --verbose 等）が正常に動作する

Scenario: 非TTY環境でサブコマンド無しで起動する（config 未存在）
  Given パイプやリダイレクト経由で `shiotsuchi` が実行される
  And   ~/.config/shiotsuchi/config.toml が存在しない
  When  サブコマンドが指定されていない
  Then  「設定ファイルが見つかりません。shiotsuchi init を実行してください」というメッセージが表示される
  And   終了コード 0 で正常終了する

Scenario: 非TTY環境でサブコマンド無しで起動する（config 存在）
  Given パイプやリダイレクト経由で `shiotsuchi` が実行される
  And   ~/.config/shiotsuchi/config.toml が存在する
  When  サブコマンドが指定されていない
  Then  ウェルカムバナーが表示される
  And   「サブコマンドを指定するか --help を参照」というメッセージで終了する
  And   終了コード 0 で正常終了する
```

## UI/UX設計 — 現状の `--help` よりわかりやすい画面

### 現状の `--help` の問題点

現在の `shiotsuchi --help` は clap が自動生成する以下のような出力である:

```
データの潮流を導く — 日本語対応ノート検索エンジン

Usage: shiotsuchi [OPTIONS] <COMMAND>

Commands:
  index     ノートボールトをインデックスしてデータベースを構築する
  search    ノートボールトをキーワード・セマンティック・ハイブリッド検索で探索する
  watch     ボールトを監視してファイル変更を自動的にインデックスする
  ...

Options:
  ...
```

**問題点:**
- コマンドがアルファベット順（あるいは定義順）で並び、目的別に整理されていない
- 新規ユーザーが「まず何をすればいいか」の順序が分からない
- 全てのコマンドがフラットに並び、情報量が多い

### 求めるTUI画面

以下のように、**オンボーディングエントリ・カテゴリ分類・「次へ」で進むウィザード形式**を含む構成にする:

#### 初回起動時（config 未存在）— ウェルカムバナー

```
╔══════════════════════════════════════════════════╗
║         Shiotsuchi Search  v0.4.x                ║
║     データの潮流を導く — 日本語対応ノート検索エンジン  ║
║                                                  ║
║  🔰 はじめての方へ                                ║
║     この画面では以下の3ステップを一緒に進められます   ║
║     ① 設定ファイルを作る                          ║
║     ② ノートをインデックスする                    ║
║     ③ 検索してみる                               ║
║                                                  ║
╚══════════════════════════════════════════════════╝
```

#### 初回起動時 — Select メニュー

```
> 🚀 オンボーディングを開始  (init → index → search を一緒に完了)
  ── セットアップ ──
  init    設定ファイルを作成・編集する
  setup   埋め込みモデルをインストールする
  ── 検索・操作 ──
  search  ノートを検索する
  index   ノートをインデックスする
  watch   ファイル変更を監視する
  ── 情報・メンテナンス ──
  stats   統計情報を表示する
  doctor  環境の状態を診断する
  ── 終了 ──
  exit    終了する
```

#### オンボーディング中 — Step 完了画面例（index 完了後）

```
✅ Step 2/3 完了: ノートのインデックスが完了しました
   42 ファイルをインデックスしました（3 スキップ、0 エラー）

  次は Step 3: ノートを検索してみましょう

  ⚡ 検索クエリを入力してください:
```

#### オンボーディング完了画面

```
╔══════════════════════════════════════════════════╗
║         🎉 オンボーディング完了！                  ║
║                                                  ║
║   これで shiotsuchi-search を使い始める準備が      ║
║   整いました。                                    ║
║                                                  ║
║   引き続きメニューから操作を選べます:              ║
║     search  ノートを検索する                      ║
║     index   再インデックスする                     ║
║     watch   ファイル変更を監視する                 ║
║     ...                                          ║
╚══════════════════════════════════════════════════╝
```

#### 通常起動時（config + DB 存在）— ウェルカムバナー

```
╔══════════════════════════════════════════════════╗
║         Shiotsuchi Search  v0.4.x                ║
║     データの潮流を導く — 日本語対応ノート検索エンジン  ║
║                                                  ║
║  🔰 はじめての方もこちら:                         ║
║    メニューの「🚀 クイックオンボーディング」を       ║
║    選ぶと使い方を体験できます                      ║
║                                                  ║
╚══════════════════════════════════════════════════╝
```

**注**: dialoguer の `Select` はセクションヘッダを非選択項目として含められない。そのため、以下の戦略で実現する:

1. **バナー内でカテゴリを表示**: `show_banner()` で分類を一覧表示する
2. **Select はフラットリスト**: 各項目の先頭にカテゴリプレフィックス（例: `[onboard] 🚀 オンボーディング` / `[setup] init`）
3. **オンボーディングは Select から選択**: 内部で init → index → search を順次実行。各ステップ完了後は `Confirm` で「次へ進みますか？」と確認し、ユーザーが承認したら次に進む

### オンボーディングフロー詳細

```
オンボーディング選択
  │
  ├─ Step 1: 設定ファイル作成（config 未存在の場合のみ）
  │   ├─ 既に存在 → スキップ
  │   └─ 未存在 → run_init() → ✅ 完了
  │        └─ 「次へ進みますか？」→ Yes → Step 2
  │
  ├─ Step 2: インデックス作成（DB 未作成または更新が必要な場合）
  │   ├─ DB 未作成 → run_chart() → ✅ 完了
  │   └─ DB 既存 → 「再インデックスしますか？」→ Yes で実行
  │        └─ 「次へ進みますか？」→ Yes → Step 3
  │
  └─ Step 3: 検索
      ├─ dialoguer::Input でクエリ入力
      ├─ run_dive() → 結果表示
      └─ 🎉 オンボーディング完了 → メニューに戻る
```

各ステップは単なる案内表示ではなく、**実際にコマンドを実行する。** 「次へ」の Confirm で承認を得てから次のステップに進むことで、ユーザーは各工程で何が起きているかを理解しながら進められる。

### 「次の一手」ガイダンス（単一コマンド実行時）

メニューから個別のコマンドを実行した場合も、オンボーディングに誘導する:

| 実行したコマンド | 表示する「次の一手」 |
|---|---|
| init (config 未存在) | `✅ 設定ファイルを作成しました。オンボーディングを続けて index → search まで完了しませんか？ [Yes/No]` → Yes でオンボーディング Step 2 へ |
| index (DB 未作成) | `✅ インデックスが完了しました。続けて search で検索してみませんか？ [Yes/No]` → Yes で Step 3 へ |
| setup | `✅ モデルのセットアップが完了しました。次に index を実行してベクトルインデックスを有効にしてください` |
| search (DB 無) | `⚠️ データベースが見つかりません。オンボーディングを開始して index まで進めますか？ [Yes/No]` → Yes でオンボーディング Step 1 から |
| doctor | `✅ 診断が完了しました。問題があれば表示されたメッセージに従ってください` |

## 受け入れ基準

### オンボーディング
- [ ] config 未存在 + TTY: メニュー先頭に「🚀 オンボーディングを開始」が表示される
- [ ] オンボーディング選択時、config 未存在なら Step 1（init）→ Step 2（index）→ Step 3（search）が順次実行される
- [ ] 各ステップ完了後、「次へ進みますか？」の確認があり、Yes で次ステップに進む
- [ ] config 存在 + DB 未存在: メニューに「⚡ オンボーディングを続ける」が表示され、Step 1 をスキップして Step 2 から開始される
- [ ] config + DB 存在: メニューに「🚀 クイックオンボーディング」が表示され、再インデックス＋検索体験ができる
- [ ] オンボーディング完了後、「🎉 オンボーディング完了！」メッセージが表示され、通常メニューに戻る

### メニューとガイダンス
- [ ] ウェルカムバナーに 3ステップの案内（init → index → search）が含まれる
- [ ] メニュー項目がカテゴリ別に分類されて表示される（オンボーディング / セットアップ / 検索・操作 / 情報・メンテナンス / 終了）
- [ ] 各コマンドの説明が `--help` よりも平易な言葉で書かれている（エンジニア以外にも伝わる表現）
- [ ] 個別コマンド実行後、その結果に応じてオンボーディングに誘導する「次の一手」確認が表示される

### 後方互換
- [ ] `shiotsuchi <subcommand>` は従来通り動作する（メニューやオンボーディングを経由しない）
- [ ] 非TTY環境ではメニューを表示せずテキストガイダンスを表示して終了
- [ ] 既存の全テストがパスする

## テスト戦略 — Red-Green-Refactor（鉄則: 実装より先に失敗するテストを書く）

**Iron Law**: プロダクションコードは、それをテストする失敗するテストが先に存在しなければ、一切書いてはならない。「後でテストを書く」「参考として残す」「テストファーストで書き直すつもりでいったん書く」も禁止。既存のコードを流用したくなったら削除してからテストファーストで書き直す。

**検証**: 各テストは必ず RED → GREEN を確認すること。テストが通ったまま実装を始めてはならない（通った時点で、そのテストは既存の動作をテストしており、新しい動作をテストしていない可能性が高い）。

### テスト不能領域の特定（重要）

dialoguer の `Select::interact()` と `Confirm::interact()` は TTY を要求するため、通常の CI 環境ではテストできない。この PBI では以下を**分離**する:

| 領域 | テスト方法 | テスト可能？ |
|------|-----------|------------|
| clap パース（command が `None` になること） | `Cli::try_parse_from()` | ✅ 既存テストと同様 |
| 非TTYパス（welcome.rs の分岐） | `tempfile` で config を作成、stdin をパイプに差し替え | ✅ 単体テスト可 |
| `show_banner()` / `menu_items()` 出力 | 標準出力キャプチャ | ✅ 単体テスト可 |
| `MenuChoice::from_index()` マッピング | 通常のマッピングテスト | ✅ 単体テスト可 |
| dialoguer の対話部分（Select, Confirm, Input） | e2e テスト（`script` コマンドで疑似 TTY）または手動確認 | ❌ CI では不可 → 手動確認で代替 |
| `run_onboarding()` 内部フロー | `Confirm` のモック or 手動確認 | ⚠️ 構造化次第 |

**対策**: dialoguer に依存する部分（`run_onboarding`, `execute_menu_choice` の対話部分）は手動確認とする。その代わり、それらの関数に渡す**直前までのロジック**（メニュー項目の構築、マッピング、出力）はすべて単体テストでカバーする。

### テストケース一覧（RED フェーズで書く順）

#### RED-1: clap がサブコマンド無しを許容する

```rust
// cli/src/main.rs の tests モジュールに追加（既存の parse_cli を流用）
#[test]
fn test_no_subcommand_parses_as_none() {
    let cli = Cli::try_parse_from(["shiotsuchi"]).unwrap();
    assert!(cli.command.is_none(), "no subcommand should result in command=None");
}
```

**期待する失敗**: `Commands` がまだ `Option<Commands>` でないためコンパイルエラー。

#### RED-2: サブコマンドありは従来通りパースできる

```rust
// 既存テストの後方互換確認 — 変更不要だが、動作確認のために明示
#[test]
fn test_subcommand_still_works_with_option() {
    let cli = Cli::try_parse_from(["shiotsuchi", "index"]).unwrap();
    assert!(cli.command.is_some(), "index subcommand should still parse");
}
```

**期待する失敗**: 同上（`Commands` が `Option` になる前はパースエラーではなくなる？）

→ 実はこのテストは既存のコードでは **パスする**（clap は `Option<Commands>` になる前からサブコマンドを必須としていた）。このテストは「後方互換の証拠」として残す。RED を確認できない例外的なケースなので、テストが既存動作と変わらないことを確認した上で追加する。

#### RED-3: 非TTY + config 未存在でガイダンスメッセージが出力される

```rust
#[test]
fn test_non_tty_no_config_shows_guidance() {
    use std::io::IsTerminal as _;
    // テスト用の一時ディレクトリに config が存在しない状態を作る
    // welcome::show_banner が出力するメッセージを検証するために、
    // config_path を一時パスに差し替える必要がある。
    // FIXME: config_path を注入可能にする設計変更が必要
}
```

**注**: このテストは `default_config_path()` がハードコードされているため、現状の設計では実行が難しい。config_path を引数で注入可能にするリファクタリング（または環境変数での上書き）が必要。このテストのためだけの変更であれば、まずは e2e テストで代用する。

#### RED-4: menu_items のラベル一覧が正しい

```rust
#[test]
fn test_menu_items_returns_correct_items() {
    let items = menu_items(true, true); // config + DB 存在
    assert_eq!(items.len(), 8, "should have 8 menu items");
    assert!(items[0].contains("オンボーディング"));
    assert!(items[1].contains("init"));
    assert!(items[7].contains("exit"));
}

#[test]
fn test_menu_items_onboarding_label_changes_with_state() {
    let no_config = menu_items(false, false);
    assert!(no_config[0].contains("オンボーディングを開始"));

    let config_no_db = menu_items(true, false);
    assert!(config_no_db[0].contains("オンボーディングを続ける"));

    let all_exists = menu_items(true, true);
    assert!(all_exists[0].contains("クイックオンボーディング"));
}
```

#### RED-5: MenuChoice::from_index のマッピング

```rust
#[test]
fn test_menu_choice_from_index() {
    assert!(matches!(MenuChoice::from_index(0), MenuChoice::Onboarding));
    assert!(matches!(MenuChoice::from_index(1), MenuChoice::Init));
    assert!(matches!(MenuChoice::from_index(4), MenuChoice::Index));
    assert!(matches!(MenuChoice::from_index(7), MenuChoice::Exit));
    assert!(matches!(MenuChoice::from_index(99), MenuChoice::Exit)); // fallback
}
```

#### RED-6: show_banner の出力にキーワードが含まれる

```rust
#[test]
fn test_show_banner_contains_onboarding_keywords() {
    let mut output = Vec::new();
    // show_banner の出力先を差し替え可能にしておく
    // （引数で writer を受け取る、またはテスト用にキャプチャ関数を用意）
    todo!("Capture show_banner output and verify content");
}
```

#### RED-7: 非TTY起動時の完全系テスト（e2e）

```rust
// e2e/src/lib.rs に追加
#[test]
fn test_no_subcommand_non_tty_shows_banner() {
    let output = std::process::Command::new("shiotsuchi")
        .env_remove("SHIOTSUCHI_NOTES_DIR")
        .stdin(std::process::Stdio::null()) // 非TTY
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Shiotsuchi Search"), "banner should appear");
    assert_eq!(output.status.code(), Some(0), "should exit 0");
}
```

### RED-GREEN-REFACTOR サイクル

各テストケースは以下のサイクルで実装する:

```
1. RED: テストを書く
2. VERIFY RED: cargo test で失敗を確認
   ↓ 失敗理由が「機能が未実装だから」であることを確認（typo や環境問題ではない）
3. GREEN: テストを通す最少のコードを書く
   ↓ YAGNI — 次のテストで必要になるまでは汎用化しない
4. VERIFY GREEN: cargo test で全テストが通ることを確認
   ↓ 他のテストを壊していないことも確認
5. REFACTOR: コードを整理する（テストはグリーンのまま）
   ↓ 動作を追加しない。リファクタリング後も全テストが通ることを確認
6. 次の RED へ
```

**テストピラミッド比率**: E2E(2) : 統合(7) : 単体(12) = 約 1:3:6

**手動確認が必要な項目**（自動テスト不可のため、実装後に手動で確認）:
- dialoguer の Select メニューが正しく表示されること
- オンボーディングの各ステップが正しい順序で進行すること
- 「次へ進みますか？」Confirm が正しく動作すること
- エラーメッセージ表示後にメニューに戻ること

## 実装アプローチ（TDD）

### 処理フロー

```
shiotsuchi (サブコマンド無し)
  │
  ├─ TTY? ─── No ──→ テキストガイダンス表示 → 終了(code 0)
  │
  └─ Yes ──→ バナー表示
        │
        ├─ config 未存在 ──→ メニュー先頭に「🚀 オンボーディングを開始」
        ├─ config 存在 + DB 未存在 ──→ メニュー先頭に「⚡ オンボーディングを続ける」
        └─ config + DB 存在 ──→ メニュー先頭に「🚀 クイックオンボーディング」
              │
              ↓
        メニューループ
              │
              ├── オンボーディング選択 ──→ onboarding_flow() → メニューに戻る
              ├── init 選択 ──→ run_init() → 「続けて index する？」→ Yes→onboarding_flow(from_step=2)
              ├── index 選択 ──→ run_chart() → 「続けて search する？」→ Yes→onboarding_flow(from_step=3)
              ├── その他コマンド選択 ──→ 実行 → メニューに戻る
              └── exit ──→ 終了
```

### オンボーディング内部フロー

```
onboarding_flow(from_step)
  │
  ├─ from_step <= 1 かつ config 未存在 ──→ Step 1: run_init()
  │     └─ ✅ 完了 → 「次へ進みますか？」→ Yes → Step 2 へ
  │
  ├─ from_step <= 2 かつ DB 未作成 ──→ Step 2: run_chart()
  │     └─ ✅ 完了 → 「次へ進みますか？」→ Yes → Step 3 へ
  │
  ├─ from_step <= 2 かつ DB 既存 ──→ 「再インデックスしますか？」
  │     ├─ Yes → run_chart() → ✅ → Step 3 へ
  │     └─ No → Step 3 へ
  │
  └─ Step 3: 検索
        ├─ dialoguer::Input でクエリ入力
        ├─ run_dive() → 結果表示
        └─ 🎉 オンボーディング完了メッセージ → メニューに戻る
```

### 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `cli/src/main.rs` | `command` を `Option<Commands>` に変更、`main()` に `None` 分岐追加 |
| `cli/src/commands/mod.rs` | `pub mod welcome;` 追加 |
| `cli/src/commands/welcome.rs` | **新規**: config不在検出 + init誘導 + ウェルカムバナー + メニュー |
| `cli/src/messages.rs` | config不在メッセージ・ウェルカムメッセージ・メニュー項目追加 |

### 実装手順

#### Step 1: `cli/src/main.rs` — `command` を Optional に

```rust
#[derive(Parser)]
#[command(
    name = "shiotsuchi",
    version,
    long_version = crate::build_info::long_version(),
    about = crate::messages::CLI_ABOUT
)]
struct Cli {
    #[arg(long, env = "SHIOTSUCHI_NOTES_DIR", global = true)]
    notes_dir: Option<std::path::PathBuf>,

    #[arg(long, env = "SHIOTSUCHI_DB_PATH", global = true)]
    db_path: Option<std::path::PathBuf>,

    #[arg(long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Commands>,  // ← Option に変更
}
```

**注意**: clap は `Option<Commands>` を自動的にサブコマンド任意として扱う。`subcommand_required` の明示的な設定は不要。

#### Step 2: `main()` に分岐を追加

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = <Cli as clap::CommandFactory>::command()
        .after_help(build_info::help_footer())
        .long_version(build_info::long_version());
    let cli = <Cli as clap::FromArgMatches>::from_arg_matches(&cmd.get_matches())?;

    let env = env_logger::Env::default()
        .filter_or("RUST_LOG", if cli.verbose { "debug" } else { "warn" });
    env_logger::Builder::from_env(env).init();

    let mut cfg = config::ShiotsuchiConfig::load();
    if let Some(ref dir) = cli.notes_dir {
        cfg.vaults.insert(
            "default".to_string(),
            config::VaultEntry { notes_dir: Some(dir.clone()), db_path: None },
        );
    }
    if let Some(ref db) = cli.db_path {
        cfg.database.db_path = Some(db.clone());
    }

    let resolved_vaults = cfg.resolved_vaults();
    let db_path = cfg.resolved_db_path();

    // Migration notice (既存コード) ...

    match cli.command {
        None => {
            // サブコマンド無し → ウェルカムガイダンスへ
            commands::welcome::run_welcome(&mut cfg, cli.notes_dir.as_deref(), cli.db_path.as_deref())?;
        }
        Some(Commands::Chart(args)) => {
            // 既存の処理（そのまま）— 全アームを Some() でラップ
        }
        Some(Commands::CheckIgnore(args)) => { ... }
        Some(Commands::Clean(_args)) => { ... }
        // ... 以下、全 Commands バリアントを Some() でラップ
    }
    Ok(())
}
```

#### Step 3: `cli/src/commands/welcome.rs` — 新規作成

要点:
1. `default_config_path()` を確認し、config が存在しない場合は init への誘導を表示
2. TTY かつ config 不在 → dialoguer の `Confirm` で「作成しますか？」と確認 → `run_init` を呼ぶ
3. 非TTY → 簡潔なテキストガイダンスを表示して終了
4. TTY + config 存在（または init 完了後）→ バナー + メニュー

```rust
use clap::CommandFactory;
use dialoguer::{theme::ColorfulTheme, Confirm, Select};
use std::io::IsTerminal;
use std::path::Path;

use crate::config::{default_config_path, ShiotsuchiConfig};
use crate::messages;
use crate::commands;

// ──────────────────────────────────────────────
// Public entry point
// ──────────────────────────────────────────────

pub fn run_welcome(
    cfg: &mut ShiotsuchiConfig,
    raw_notes_dir: Option<&Path>,
    raw_db_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = default_config_path();
    let db_path = cfg.resolved_db_path();
    let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();

    if !is_tty {
        if !config_path.exists() {
            eprintln!("{}", messages::WELCOME_NON_TTY_NO_CONFIG);
        } else {
            println!("{}", messages::WELCOME_NON_TTY_HINT);
        }
        return Ok(());
    }

    // ── Show banner with contextual onboarding info ──
    let mut config_exists = config_path.exists();
    let mut db_exists = db_path.exists();
    show_banner(config_exists, db_exists);

    // Main menu loop
    loop {
        let items = menu_items(config_exists, db_exists);
        let selection = Select::with_theme(&ColorfulTheme::default())
            .items(&items)
            .default(0)
            .interact()?;

        let choice = MenuChoice::from_index(selection);
        match choice {
            MenuChoice::Exit => {
                println!("{}", messages::WELCOME_EXIT);
                break;
            }
            MenuChoice::Onboarding => {
                // onboarding は独立実行 → 完了後に cfg を再読み込み
                if let Err(e) = run_onboarding(
                    config_exists, db_exists, cfg, &config_path,
                    raw_notes_dir, raw_db_path,
                ) {
                    eprintln!("⚠️ オンボーディング中にエラーが発生しました: {}", e);
                }
                // 設定ファイルが新しく作成された可能性があるので cfg を再読み込み
                *cfg = ShiotsuchiConfig::load();
                config_exists = config_path.exists();
                db_exists = cfg.resolved_db_path().exists();
                show_banner(config_exists, db_exists);
            }
            _ => {
                if let Err(e) = execute_menu_choice(
                    choice, cfg, &config_path, raw_notes_dir, raw_db_path,
                ) {
                    eprintln!("⚠️ エラー: {}", e);
                }
            }
        }
    }
}
```

#### Step 4: `show_banner` — オンボーディング案内 + カテゴリ別表示

`show_banner()` は現在の状態に応じて表示内容を変える:

- **config 未存在**: 「はじめての方へ 3ステップ案内」を表示
- **config + DB 未存在**: 「⚡ 続きから始める」案内を表示
- **config + DB 存在**: 通常バナー（簡易版クイックスタート案内）

```rust
fn show_banner(config_exists: bool, db_exists: bool) {
    let version = format!("Shiotsuchi Search  v{}", env!("CARGO_PKG_VERSION"));
    let inner_w = 50;
    let pad_v = inner_w.saturating_sub(version.chars().count());
    let left_v = pad_v / 2;
    let right_v = pad_v - left_v;

    println!("╔{}╗", "═".repeat(inner_w));
    println!("║{}{}{}║", " ".repeat(left_v), version, " ".repeat(right_v));
    println!("║  {}  ║", messages::WELCOME_TAGLINE);
    println!("║{}║", " ".repeat(inner_w));

    if !config_exists {
        // First-run: show onboarding welcome
        println!("║  🔰 はじめての方へ                         ║");
        println!("║     この画面では以下の3ステップを            ║");
        println!("║     一緒に進められます                      ║");
        println!("║     ① 設定ファイルを作る                    ║");
        println!("║     ② ノートをインデックスする               ║");
        println!("║     ③ 検索してみる                          ║");
    } else if !db_exists {
        println!("║  ⚡ オンボーディングの続きから始めましょう    ║");
        println!("║     ② ノートをインデックスする               ║");
        println!("║     ③ 検索してみる                          ║");
    } else {
        println!("║  🔰 はじめての方も: 「🚀 クイック            ║");
        println!("║     オンボーディング」で使い方を体験できます  ║");
    }

    println!("║{}║", " ".repeat(inner_w));
    println!("╚{}╝", "═".repeat(inner_w));
    println!();

    // Category listing (informational, always shown)
    println!("実行する操作を選んでください (上下キー:移動, Enter:決定):");
    println!();
    println!("  🚀 オンボーディング  (init → index → search を一緒に完了)");
    println!();
    println!("  ── セットアップ ──");
    println!("  init     設定ファイルを作成・編集する");
    println!("  setup    埋め込みモデルをインストールする");
    println!();
    println!("  ── 検索・操作 ──");
    println!("  search   ノートを検索する");
    println!("  index    ノートをインデックスする");
    println!();
    println!("  ── 情報・メンテナンス ──");
    println!("  stats    統計情報を表示する");
    println!("  doctor   環境の状態を診断する");
    println!();
    println!("  ── 終了 ──");
    println!("  exit     終了する");
    println!();
}

/// Menu items. First item is always onboarding (context-sensitive label).
fn menu_items(config_exists: bool, db_exists: bool) -> Vec<String> {
    let onboarding_label = if !config_exists {
        "[onboard] 🚀 オンボーディングを開始  (init → index → search)"
    } else if !db_exists {
        "[onboard] ⚡ オンボーディングを続ける  (index → search)"
    } else {
        "[onboard] 🚀 クイックオンボーディング  (再インデックス → 検索)"
    };

    vec![
        onboarding_label.to_string(),
        "[setup]  init    設定ファイルを作成・編集する".to_string(),
        "[setup]  setup   埋め込みモデルをインストールする".to_string(),
        "[search] search  ノートを検索する".to_string(),
        "[search] index   ノートをインデックスする".to_string(),
        "[info]   stats   統計情報を表示する".to_string(),
        "[info]   doctor  環境の状態を診断する".to_string(),
        "         exit    終了する".to_string(),
    ]
}

enum MenuChoice {
    Onboarding,
    Init,
    Setup,
    Search,
    Index,
    Stats,
    Doctor,
    Exit,
}

impl MenuChoice {
    fn from_index(i: usize) -> Self {
        match i {
            0 => MenuChoice::Onboarding,
            1 => MenuChoice::Init,
            2 => MenuChoice::Setup,
            3 => MenuChoice::Search,
            4 => MenuChoice::Index,
            5 => MenuChoice::Stats,
            6 => MenuChoice::Doctor,
            _ => MenuChoice::Exit,
        }
    }
}
```

**注意**: `MenuChoice::from_index()` のインデックスは `menu_items()` のベクター順と完全に一致する必要がある。項目を追加・削除・並び替える場合は両方を同時に変更すること。

#### Step 5: `cli/src/messages.rs` — メッセージ追加

```rust
// ──────────────────────────────────────────────
// welcome.rs — ウェルカムメニュー・config不在
// ──────────────────────────────────────────────

pub const WELCOME_TAGLINE: &str = "データの潮流を導く — 日本語対応ノート検索エンジン";
pub const WELCOME_QUICKSTART: &str = "クイックスタート: init → index → search";
pub const WELCOME_CONFIG_NOT_FOUND: &str = "\
設定ファイルが見つかりません。
shiotsuchi-search を使うには、まずノートの場所を設定する必要があります。";
pub const WELCOME_CONFIG_CREATE_PROMPT: &str = "設定ファイルを対話的に作成しますか？";
pub const WELCOME_CONFIG_SKIP_HINT: &str = "`shiotsuchi init` を実行すると後から設定できます。";
pub const WELCOME_NON_TTY_NO_CONFIG: &str = "\
設定ファイルが見つかりません。
`shiotsuchi init` を実行して設定ファイルを作成してください。";
pub const WELCOME_NON_TTY_HINT: &str = "サブコマンドを指定するか、`shiotsuchi --help` で使い方を確認してください。";
pub const WELCOME_EXIT: &str = "またのお越しをお待ちしています。";
```

#### Step 6: `run_onboarding` — 3ステップオンボーディング

```rust
/// Run the onboarding wizard: init → index → search.
/// Each step is executed sequentially with user confirmation between steps.
/// Before each step, a pre-flight summary is shown for user confirmation.
fn run_onboarding(
    config_exists: bool,
    db_exists: bool,
    cfg: &ShiotsuchiConfig,
    config_path: &Path,
    raw_notes_dir: Option<&Path>,
    raw_db_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    use dialoguer::Confirm;

    // ── Step 1: Config ──
    if !config_exists {
        println!("\n🔰 Step 1/3: 設定ファイルを作成します");
        println!("  作成先: {}", config_path.display());
        println!("  ノート: {}", raw_notes_dir.unwrap_or_else(|| {
            cfg.resolved_vaults().first()
                .map(|(_, d)| d.as_path())
                .unwrap_or_else(|| Path::new("."))
        }).display());
        if !Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("この内容で設定ファイルを作成しますか？")
            .default(true)
            .interact()?
        {
            println!("オンボーディングを中断しました。メニューからいつでも再開できます。");
            return Ok(());
        }
        let init_args = commands::init::InitArgs { force: false, yes: false };
        commands::init::run_init(&init_args, cfg, config_path, raw_notes_dir, raw_db_path)?;
        println!("✅ Step 1/3 完了: 設定ファイルを作成しました");

        if !Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Step 2 に進んでノートをインデックスしますか？")
            .default(true)
            .interact()?
        {
            println!("オンボーディングを中断しました。メニューからいつでも再開できます。");
            return Ok(());
        }
    }

    // ── Step 2: Index ──
    if !db_exists {
        println!("\n⚡ Step 2/3: ノートをインデックスします");
        println!("  ボールト: {}", cfg.resolved_vaults().first()
            .map(|(_, d)| d.display().to_string())
            .unwrap_or_else(|| ".".to_string()));
        if !Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("この内容でインデックスを実行しますか？")
            .default(true)
            .interact()?
        {
            println!("オンボーディングを中断しました。メニューからいつでも再開できます。");
            return Ok(());
        }
        commands::chart::run_chart(
            &commands::chart::ChartArgs { vault: None, quiet: false, force: false },
            &cfg.resolved_vaults(), &cfg.resolved_db_path(),
            &cfg.indexing, &cfg.embedder, &cfg.vlm,
        )?;
        println!("✅ Step 2/3 完了: ノートのインデックスが完了しました");
    } else {
        println!("\n⚡ Step 2/3: ノートを再インデックスします（すでにデータベースが存在します）");
        if !Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("データベースが存在します。再インデックスしますか？")
            .default(false)
            .interact()?
        {
            // Skip to step 3
        } else {
            commands::chart::run_chart(
                &commands::chart::ChartArgs { vault: None, quiet: false, force: false },
                &cfg.resolved_vaults(), &cfg.resolved_db_path(),
                &cfg.indexing, &cfg.embedder, &cfg.vlm,
            )?;
            println!("✅ Step 2/3 完了: ノートの再インデックスが完了しました");
        }
    }

    if !Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Step 3 に進んで検索を体験しますか？")
        .default(true)
        .interact()?
    {
        println!("オンボーディングを中断しました。メニューからいつでも検索できます。");
        return Ok(());
    }

    // ── Step 3: Search ──
    println!("\n🔍 Step 3/3: ノートを検索してみましょう");
    let query: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
        .with_prompt("検索クエリを入力してください")
        .interact_text()?;

    let db_path = cfg.resolved_db_path();
    let args = commands::dive::DiveArgs {
        query,
        format: None,
        json: false,
        limit: None,
        mode: None,
        model_path: None,
        fuzzy: false,
        alpha: None,
        tag: None,
        since: None,
        vault: None,
        mmr: false,
        lambda: None,
        threshold: None,
    };
    let start = Instant::now();
    commands::dive::run_dive(
        &args, &db_path, &cfg.resolved_vaults(),
        &cfg.indexing.user_dictionary, &cfg.synonyms,
        false, Some(0.5), false, None, None,
    )?;
    commands::dive::print_results(&[], &args.query, &args.effective_format(), start.elapsed());

    // ── Completion (no extra "press Enter" — caller's menu loop handles it) ──
    println!();
    println!("╔══════════════════════════════════════════════╗");
    println!("║         🎉 オンボーディング完了！            ║");
    println!("║                                              ║");
    println!("║  これで shiotsuchi-search を使い始める準備が   ║");
    println!("║  整いました。                                ║");
    println!("║                                              ║");
    println!("║  メニューからさらに操作を選べます:            ║");
    println!("║    search  ノートを検索する                   ║");
    println!("║    index   再インデックスする                  ║");
    println!("║    stats   統計情報を表示する                 ║");
    println!("║    ...                                       ║");
    println!("╚══════════════════════════════════════════════╝");
    println!();

    Ok(())
}
```

#### Step 7: `execute_menu_choice` — コマンド実行 + オンボーディング誘導

```rust
fn execute_menu_choice(
    choice: MenuChoice,
    cfg: &ShiotsuchiConfig,
    config_path: &Path,
    raw_notes_dir: Option<&Path>,
    raw_db_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    use dialoguer::Confirm;

    match choice {
        MenuChoice::Search => {
            let db_path = cfg.resolved_db_path();
            if !db_path.exists() {
                eprintln!("{}", messages::ERR_DB_NOT_FOUND);
                // オンボーディングに誘導
                if Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("オンボーディングを開始して index → search まで進めますか？")
                    .default(true)
                    .interact()?
                {
                    run_onboarding(false, false, cfg, config_path, raw_notes_dir, raw_db_path)?;
                }
                return Ok(());
            }
            let query: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                .with_prompt("検索クエリを入力してください")
                .interact_text()?;
            let args = commands::dive::DiveArgs {
                query,
                format: None, json: false, limit: None, mode: None,
                model_path: None, fuzzy: false, alpha: None,
                tag: None, since: None, vault: None,
                mmr: false, lambda: None, threshold: None,
            };
            let start = Instant::now();
            commands::dive::run_dive(
                &args, &db_path, &cfg.resolved_vaults(),
                &cfg.indexing.user_dictionary, &cfg.synonyms,
                false, Some(0.5), false, None, None,
            )?;
            commands::dive::print_results(&[], &args.query, &args.effective_format(), start.elapsed());
        }
        MenuChoice::Index => {
            commands::chart::run_chart(
                &commands::chart::ChartArgs { vault: None, quiet: false, force: false },
                &cfg.resolved_vaults(), &cfg.resolved_db_path(),
                &cfg.indexing, &cfg.embedder, &cfg.vlm,
            )?;
            // オンボーディングに誘導
            if Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("✅ インデックスが完了しました。続けて search で検索してみませんか？")
                .default(true)
                .interact()?
            {
                run_onboarding(true, true, cfg, config_path, raw_notes_dir, raw_db_path)?;
            }
        }
        MenuChoice::Stats => {
            commands::tide::run_tide(&cfg.resolved_db_path())?;
        }
        MenuChoice::Doctor => {
            commands::doctor::run_doctor(cfg, &cfg.resolved_db_path(),
                &cfg.resolved_vaults(), &cfg.indexing, &cfg.vlm)?;
            println!("✅ 診断が完了しました。問題があれば表示されたメッセージに従ってください");
        }
        MenuChoice::Init => {
            let init_args = commands::init::InitArgs { force: false, yes: false };
            commands::init::run_init(&init_args, cfg, config_path, raw_notes_dir, raw_db_path)?;
            // オンボーディングに誘導
            if Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("✅ 設定ファイルを作成しました。オンボーディングを続けて index → search まで完了しませんか？")
                .default(true)
                .interact()?
            {
                run_onboarding(true, false, cfg, config_path, raw_notes_dir, raw_db_path)?;
            }
        }
        MenuChoice::Setup => {
            let setup_args = commands::setup::SetupArgs { check: false, model_path: None };
            commands::setup::run_setup(&setup_args)?;
            println!("✅ モデルのセットアップが完了しました。次に index を実行してベクトルインデックスを有効にしてください");
        }
        MenuChoice::Onboarding | MenuChoice::Exit => unreachable!(), // handled in caller
    }
    Ok(())
}
```

**「次の一手」ガイダンス一覧**:

| アクション | 表示する確認 |
|-----------|------------|
| init 完了 (config無) | `✅ 設定ファイルを作成しました。オンボーディングを続けて index → search まで完了しませんか？` |
| index 完了 (DB無) | `✅ インデックスが完了しました。続けて search で検索してみませんか？` |
| search 実行 (DB無) | `⚠️ データベースが見つかりません。オンボーディングを開始して index → search まで進めますか？` |
| setup 完了 | ✅ モデルのセットアップが完了しました（オンボーディング誘導なし、次の操作はユーザーに委ねる） |
| doctor 完了 | ✅ 診断が完了しました（オンボーディング誘導なし、結果に応じて対処） |

**注**: `Search` の引数構築には `DiveArgs` の全フィールドを正しく設定する必要がある。オプション引数（`--mode`, `--limit`, `--tag`, `--since` 等）は最初のリリースではデフォルト値で動作させ、メニューでは扱わない。ユーザーが細かく指定したい場合は `shiotsuchi search <query>` のCLI直接入力を促す。

### 落とし穴

1. **`match` の全アーム変更漏れ**: `match cli.command` の各アームを `Some(Commands::Xxx)` に変更する必要がある。機械的な変更だが、1つでも漏れるとコンパイルエラーになる。全てのバリアントをカバーしているかを確認すること。

2. **既存テストの互換性**: 既存テストでは `Cli::try_parse_from(["shiotsuchi", "dive", ...])` のように必ずサブコマンドを指定している。これらのテストは `Commands` が `Option` になっても `Some(Commands::Dive(...))` としてマッチするため、テストコードの変更は**不要**。ただしテスト内で `cli.command` を直接 `Commands::Dive` と比較している箇所があれば修正が必要。

3. **非TTY時の動作**: dialoguer は非TTYで `.interact()` を呼ぶと panic/エラーになる。必ず `is_tty()` チェックを先に行い、非TTYではメニューを表示せずに終了する。

4. **init 実行後の cfg 再読み込み**: `run_welcome` は `&mut ShiotsuchiConfig` を受け取る。オンボーディング完了後は `*cfg = ShiotsuchiConfig::load()` で設定を再読み込みするが、`execute_menu_choice` から呼ばれる `run_onboarding` は `&ShiotsuchiConfig` を受け取るため、init 後に即座に cfg を更新できない。ただし `raw_notes_dir` と `raw_db_path` は `run_welcome` 経由で渡されるため、init 実行時に CLI 引数が正しく反映される。メニュー選択後に cfg が完全に最新である必要がある場合は、`execute_menu_choice` も `&mut` 化を検討する。

5. **Search の引数構築**: `dive` コマンドは `DiveArgs` 構造体（+ 多数のパラメータ）を必要とする。menu から search を呼ぶ際は dialoguer でクエリ文字列を入力させ、`DiveArgs` を簡易構築して `run_dive` に渡す。オプション引数（`--mode`, `--limit` 等）は最初のリリースではデフォルト値で動作させ、メニューでは扱わないことを明記する。

6. **エラーハンドリングの漏れ**: `execute_menu_choice` のエラーは `if let Err(e) = ...` でキャッチしてメニューに戻る。しかし `run_welcome` 内で `Select::interact()` 自体のエラー（例: Ctrl+C）は伝搬させる（プログラム終了）。Ctrl+C をキャッチして「終了しますか？」と確認するかは将来の拡張とする。

## 見積もり

**5ポイント**（新規ファイル作成 + main.rs の分岐追加 + メッセージ追加 + テスト。config不在検出と init への誘導を含むため前回よりやや増加）

## 技術的考慮事項

- **依存関係**: dialoguer は既存（`ColorfulTheme` 使用パターンが確立済み）
- **後方互換**: `shiotsuchi <subcommand>` の動作は全く変わらない。`--help`, `--version` も従来通り。
- **テスタビリティ**: 非TTYパスは標準出力をキャプチャしてテスト可能。TTYパス（dialoguer）はモック困難なため、テストは最小限に抑え手動確認を中心とする。
- **非機能要件**: メニュー表示は一瞬で完了するため性能影響なし。
- **UTF-8対応**: 日本語メッセージは問題ないが、罫線文字（`╔═╗`）は環境によって表示が異なる場合がある。必要に応じて ASCII 代替（`+--+`）にフォールバックする。

## 設計判断

### TTY チェック方法

`init.rs` では `dialoguer_stdin_is_tty()` という自作関数を使用しているが、`doctor.rs` では `std::io::stdin().is_terminal()` を使用。`std::io::IsTerminal` trait は Rust 1.70+ で安定化済み。プロジェクトの MSRV に合わせて一貫した方法を選ぶ。

### メニュー項目の選定とカテゴリ分類

すべてのサブコマンドをメニューに表示すると項目数が多すぎる（現状 16 コマンド）。初期リリースではよく使う 7 コマンド + オンボーディング に絞り、**カテゴリ別に分類**する:

| カテゴリ | コマンド | 平易な説明（`--help` との違い） |
|----------|---------|-------------------------------|
| 🔰 オンボーディング | `🚀 オンボーディング` | 3ステップを一緒に実行（init → index → search） |
| セットアップ | `init` | `--help`: 「設定ファイルを対話形式で初期化する」→ TUI: 「設定ファイルを作成・編集する」 |
| セットアップ | `setup` | `--help`: 「セマンティック検索用の埋め込みモデルをセットアップする」→ TUI: 「埋め込みモデルをインストールする」 |
| 検索・操作 | `search` | `--help`: 「ノートボールトをキーワード・セマンティック・ハイブリッド検索で探索する」→ TUI: 「ノートを検索する」 |
| 検索・操作 | `index` | `--help`: 「ノートボールトをインデックスしてデータベースを構築する」→ TUI: 「ノートをインデックスする」 |
| 情報・メンテナンス | `stats` | `--help`: 「インデックスの統計情報（ファイル数・チャンク数・DB サイズ等）を表示する」→ TUI: 「統計情報を表示する」 |
| 情報・メンテナンス | `doctor` | `--help`: 「設定・データベース・ボールトの状態を診断する」→ TUI: 「環境の状態を診断する」 |
| 終了 | `exit` | (メニュー専用) |

メニューから除外したコマンド（`watch`, `list`, `prune`, `tasks`, `config`, `support`, `clean`, `check-ignore`, `delete`, `synonym`, `setup`）は、メニュー下部に「その他のコマンドは `shiotsuchi --help` を参照」と表示する。

**カテゴリ分類の意図**: 「オンボーディング」→「セットアップ」→「検索・操作」→「情報・メンテナンス」の順は、新規ユーザーが実際に操作する流れ（まず設定、次に検索、最後に確認）に沿っている。これにより、ユーザーは自分の「今やるべきこと」がどのカテゴリにあるか直感的に判断できる。

**平易な説明の原則**: `--help` は網羅性を重視するが、TUIメニューでは「そのコマンドで何ができるか」を1秒で理解できる表現にする。技術的な詳細（「セマンティック・ハイブリッド検索」など）は省き、ユーザーの目的（「ノートを検索する」）を優先する。

### config 不在時の init 呼び出し

`run_init` は設定パス・ノートディレクトリ等の引数を取る。welcome から呼ぶ際は `cli.notes_dir` と `cli.db_path` をそのまま渡す。これにより `shiotsuchi --notes-dir ~/MyNotes`（サブコマンド無し）でも正しく設定ファイルが作成される。

## 実装者向け注記

### 現状コードの確認

（着手前に必ず実行すること）

```bash
# サブコマンド必須の設定を確認
grep -n "command:" cli/src/main.rs

# dialoguer の使用パターンを確認（テンプレート）
grep -rn "dialoguer" cli/src/commands/init.rs cli/src/commands/doctor.rs

# 既存のコマンド引数構造体を確認（menu から呼び出すため）
grep -n "pub struct.*Args" cli/src/commands/*.rs

# config パス解決を確認
grep -n "default_config_path\|fn load" core/src/config.rs

# 全テストのパスを確認してから着手
cargo test -p shiotsuchi-cli
```

### 実装手順

```bash
# 1. main.rs: command を Option に変更
# 2. main.rs: match アームを Some() でラップ
# 3. commands/welcome.rs を新規作成（config不在検出 + init誘導 + バナー + メニュー）
# 4. commands/mod.rs に pub mod welcome を追加
# 5. messages.rs にウェルカムメッセージ・config不在メッセージを追加
# 6. テスト追加（非TTYパス + clapパース）
# 7. cargo test -p shiotsuchi-cli で全テスト通過確認
```

### キーとなるコードスニペット

```rust
// main.rs — command を Option に
#[command(subcommand)]
command: Option<Commands>,

// main.rs — 分岐
match cli.command {
    None => commands::welcome::run_welcome(&cfg, cli.notes_dir.as_deref(), cli.db_path.as_deref())?,
    Some(Commands::Chart(args)) => { ... }
    // ... 全アームを Some() でラップ
}

// welcome.rs — config不在検出 + init誘導
let config_path = default_config_path();
if !config_path.exists() {
    if is_tty {
        eprintln!("{}", messages::WELCOME_CONFIG_NOT_FOUND);
        let create = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(messages::WELCOME_CONFIG_CREATE_PROMPT)
            .default(true)
            .interact()?;
        if create {
            commands::init::run_init(&init_args, cfg, &config_path, raw_notes_dir, raw_db_path)?;
        }
    } else {
        eprintln!("{}", messages::WELCOME_NON_TTY_NO_CONFIG);
        return Ok(());
    }
}
```

### 落とし穴

1. **`unused import` 警告**: `Commands` enum を使わないサブコマンドがある場合、`#![allow(unused_imports)]` または個別に unused を抑制する
2. **`Box<dyn Error>` の伝播**: `welcome.rs` から各コマンドの `run_*` 関数を呼ぶ際、エラー型が統一されているか確認
3. **clap の `get_matches` 呼び出し**: `main()` 内で `cmd.get_matches()` と `Cli::from_arg_matches()` を使っている。`command` が `None` の場合もエラーにならないことを clap の動作として確認する
4. **`run_init` の引数が複雑**: `InitArgs { force: false, yes: false }` と `notes_dir`, `db_path` を正しく渡すこと。特に `raw_notes_dir` は `Option<&Path>`、`raw_db_path` は `Option<&Path>` であることに注意
5. **`dialoguer_stdin_is_tty()` vs `IsTerminal`**: `init.rs` で使われている `dialoguer_stdin_is_tty()` は dialoguer 内部の TTY 判定用。`welcome.rs` では `std::io::stdin().is_terminal()`（Rust 標準）を使用する。両者の結果が異なるケースがある場合に備えて、確実に動作する方を選ぶ
6. **非TTY・config存在の分岐**: フロー図にある通り、非TTYかつconfig存在の場合はバナー表示後に簡潔なガイダンスを表示して終了する。dialoguer を一切呼ばないこと。

## Definition of Done

### TDD 検証（Iron Law）
- [ ] 全てのプロダクションコードに対して、それをテストする失敗するテストが先に存在した
- [ ] 各テストの RED（失敗）を確認した（コンパイルエラーではなく「機能未実装」による失敗）
- [ ] 各テストの GREEN（成功）を確認した
- [ ] 各 REFACTOR 後も全テストがグリーンであることを確認した
- [ ] テスト名に "and" が含まれていない（1テスト=1動作）

### 自動テスト
- [ ] clap: サブコマンド無しで `command` が `None` になる
- [ ] clap: サブコマンドありで `command` が `Some(...)` になる（後方互換）
- [ ] `menu_items()`: config/DB の状態に応じて正しいラベルを返す
- [ ] `MenuChoice::from_index()`: 全インデックスのマッピングが正しい
- [ ] 非TTY + config 未存在: ガイダンスメッセージが標準出力に表示される
- [ ] 非TTY + config 存在: ガイダンスメッセージが標準出力に表示される
- [ ] `cargo test -p shiotsuchi-cli` がグリーン（既存テストも全てパス）

### 手動確認（dialoguer 対話部分）
- [ ] config 未存在 + TTY: メニュー先頭に「🚀 オンボーディングを開始」が表示される
- [ ] オンボーディング選択時、事前確認 → init → index → search が順次実行される
- [ ] 各ステップ完了後、Confirm が表示され Yes/No で制御できる
- [ ] init 実行後、「Step 2 に進みますか？」で Yes → index、No → メニューに戻る
- [ ] 個別コマンド（init, index 等）実行後、オンボーディングに誘導する確認が表示される
- [ ] エラー発生時（DB 無しで search 等）、メッセージ表示後にメニューに戻る
- [ ] `shiotsushi <subcommand>` が従来通り動作する

### コード品質
- [ ] dialoguer の `.interact()` 呼び出しは全て `is_tty()` チェックの後にある
- [ ] エラーはメニューループでキャッチされ、メニューに戻る
- [ ] コードレビュー完了
- [ ] `ref/cli.md` にインタラクティブモードの記載を追加（任意）
