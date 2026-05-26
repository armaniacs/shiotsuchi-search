//! Japanese user-facing message constants for the Shiotsuchi CLI.
//!
//! All `println!`, `eprintln!`, error messages, and user-facing outputs
//! in `cli/src/commands/*.rs` and `cli/src/main.rs` reference these constants.
//!
//! Core-library errors (`core/src/`) remain English; they are converted to
//! Japanese in this layer.

/// Format a message template by replacing `{}` placeholders with the given
/// arguments.  Use this instead of `format!()` because the format macros
/// require a string literal.
#[macro_export]
macro_rules! msg_fmt {
    ($template:expr $(, $arg:expr)* $(,)?) => {{
        let mut __s = ($template).to_string();
        $(
            __s = __s.replacen("{}", &format!("{}", $arg), 1);
        )*
        __s
    }};
}

// ──────────────────────────────────────────────
// main.rs — グローバルメッセージ
// ──────────────────────────────────────────────

pub const CLI_ABOUT: &str = "データの潮流を導く — 日本語対応ノート検索エンジン";

pub const ERR_DB_NOT_FOUND: &str = "データベースが見つかりません。先に `shiotsuchi chart` を実行してボールトをインデックスしてください";
pub const ERR_PREFIX: &str = "エラー";

// ──────────────────────────────────────────────
// dive.rs — 検索コマンド
// ──────────────────────────────────────────────

pub const DIVE_JSON_HELP: &str = "コンパクトな JSON で出力（非推奨: --format json を使用してください）";
pub const DIVE_LIMIT_HELP: &str = "最大結果件数";
pub const DIVE_FORMAT_HELP: &str = "出力フォーマット（デフォルト: テーブル。--json が設定されている場合は JSON）";
pub const DIVE_MODE_HELP: &str = "検索モード: fts（キーワード）, vec（セマンティック）, hybrid（デフォルト）";
pub const DIVE_MODEL_PATH_HELP: &str = "ONNX 埋め込みモデルファイルのパス（SHIOTSUCHI_EMBED_MODEL_PATH と XDG デフォルトを上書き）";
pub const DIVE_VAULT_HELP: &str = "特定のボールトに絞り込む";
pub const DIVE_TAG_HELP: &str = "フロントマターのタグで絞り込み（例: --tag project）";
pub const DIVE_SINCE_HELP: &str = "フロントマターの日付で絞り込み（ISO 8601, 例: --since 2026-01-01）";
pub const DIVE_FUZZY_HELP: &str = "あいまい検索を有効化（全角/半角・大文字/小文字の差異を吸収）";
pub const DIVE_ALPHA_HELP: &str = "ハイブリッド検索のブレンド比率 (0.0〜1.0)。0.0=ベクトル検索のみ, 1.0=FTS5 のみ, 未指定=RRF";

pub const ERR_VAULT_NOT_FOUND: &str = "ボールト '{}' は設定に定義されていません。利用可能なボールト: {}";
pub const WARN_EMBEDDER_LOAD_FAILED: &str = "[警告] 埋め込みモデルの読み込みに失敗しました: {}。FTS（キーワード検索）のみ使用します";
pub const WARN_EMBEDDER_NOT_FOUND: &str = "[警告] モデルファイルが見つかりません。ベクトル検索は無効です。\n キーワード検索（FTS5）のみで動作します。";
pub const ERR_SEMANTIC_DISABLED: &str = "セマンティック検索は利用できません: 'semantic' 機能が有効化されていません。デフォルト機能を有効にして再ビルドしてください";
pub const ERR_VEC_NO_MODEL: &str = "ベクトル検索にはモデルが必要です。SHIOTSUCHI_EMBED_MODEL_PATH を設定するか --model-path を指定してください";

pub const RESULTS_HEADER: &str = "検索結果: \"{}\"";
pub const RESULTS_COUNT: &str = "{} 件の結果が見つかりました ({:.3} 秒)";

// Search mode descriptions
pub const MODE_FTS_HELP: &str = "FTS5 によるキーワード検索（常に利用可能）";
pub const MODE_VEC_HELP: &str = "ベクトル検索（モデルが必要）";
pub const MODE_HYBRID_HELP: &str = "FTS + ベクトルのハイブリッド検索（デフォルト。モデルがない場合は FTS にフォールバック）";

// Output format descriptions
pub const FORMAT_TABLE_HELP: &str = "ファイルパス・ヘッダ・スニペット・スコアを含む書式付きテーブル";
pub const FORMAT_JSON_HELP: &str = "コンパクトな JSON 配列（1行）";
pub const FORMAT_JSON_PRETTY_HELP: &str = "見やすい形式の JSON 配列";

// ──────────────────────────────────────────────
// chart.rs — インデックス作成
// ──────────────────────────────────────────────

pub const CHART_FORCE_HELP: &str = "非推奨: `shiotsuchi init --force` を使用してください";
pub const CHART_QUIET_HELP: &str = "進行状況の表示を抑制する";
pub const CHART_FORCE_DEPRECATED: &str = "警告: --force は chart コマンドでは無効です。代わりに `shiotsuchi init --force` を使用してください";

pub const INFO_EMBEDDER_LOADED: &str = "[情報] 埋め込みモデルを読み込みました — ベクトルインデックスを有効化";
pub const WARN_EMBEDDER_LOAD: &str = "[警告] 埋め込みモデルを読み込めませんでした: {}";
pub const INFO_EMBEDDER_SKIPPED: &str = "[情報] 埋め込みモデルが見つかりません — ベクトルインデックスをスキップします。`shiotsuchi setup` を実行してセマンティック検索を有効にしてください";
pub const INDEX_SUMMARY: &str = "{} ファイルをインデックスしました（{} スキップ、{} エラー）";
pub const INDEX_PATTERN_WARN: &str = "  {} 個の無効なパターンが exclude_dirs に含まれています";

// ──────────────────────────────────────────────
// scan.rs — ファイル監視
// ──────────────────────────────────────────────

pub const SCAN_DEBOUNCE_HELP: &str = "非推奨: デバウンスは内部で管理されています";
pub const SCAN_DEBOUNCE_DEPRECATED: &str = "警告: --debounce は無効です。デバウンスは内部で管理されています";

// ──────────────────────────────────────────────
// clean.rs — 再インデックス
// ──────────────────────────────────────────────

pub const CLEAN_DB_NOT_FOUND: &str = "データベースが見つかりません: {}。先に `shiotsuchi chart` を実行してインデックスを作成してください";
pub const CLEAN_BACKUP_FAILED: &str = "警告: {} のバックアップに失敗しました: {}";
pub const CLEAN_RENAME_FAILED: &str = "警告: リネームに失敗しました（別デバイス?）、コピーにフォールバックします: {}";
pub const CLEAN_BACKUP_SAVED: &str = "バックアップを保存しました: {}";
pub const CLEAN_REINDEXED: &str = "再インデックスしました: {} ファイル（{} スキップ、{} エラー）";

// ──────────────────────────────────────────────
// dredge.rs — スタイルエントリ削除
// ──────────────────────────────────────────────

pub const DREDGE_DB_NOT_FOUND: &str = "エラー: データベースが見つかりません。先に `shiotsuchi chart` を実行してください";
pub const DREDGE_NO_STALE: &str = "期限切れのエントリはありません。";
pub const DREDGE_WOULD_REMOVE: &str = "{} 個の期限切れファイルを削除します:";
pub const DREDGE_REMOVED: &str = "{} 個の期限切れファイルを削除しました:";
pub const DREDGE_VACUUM_DONE: &str = "VACUUM 完了。";

// ──────────────────────────────────────────────
// tide.rs — 統計表示
// ──────────────────────────────────────────────

pub const TIDE_TOTAL_FILES: &str = "総ファイル数: {}";
pub const TIDE_TOTAL_CHUNKS: &str = "総チャンク数: {}";
pub const TIDE_DB_SIZE: &str = "DB サイズ: {} バイト";
pub const TIDE_EMBEDDER: &str = "埋め込みモデル: {}";
pub const TIDE_LAST_INDEXED: &str = "最終インデックス日時: {}";
pub const TIDE_NEVER_INDEXED: &str = "最終インデックス日時: なし";

// ──────────────────────────────────────────────
// delete.rs — ノート削除
// ──────────────────────────────────────────────

pub const DELETE_PATH_HELP: &str = "ボールトルートからの相対パス（例: meeting/notes.md）";
pub const ERR_DELETE_INVALID_PATH: &str = "無効なパス: 相対パスかつボールト内である必要があります";
pub const ERR_DELETE_NO_VAULTS: &str = "ボールトが設定されていません。削除するものはありません。";
pub const ERR_DELETE_PATH_ESCAPES: &str = "パスがボールトディレクトリの外に出ようとしています";
pub const DELETED_FILE: &str = "削除しました: {}";

// ──────────────────────────────────────────────
// init.rs — 設定初期化
// ──────────────────────────────────────────────

pub const INIT_FORCE_HELP: &str = "既存の設定ファイルを上書きする";
pub const INIT_YES_HELP: &str = "非対話モード: 検出された除外候補をすべて自動承認する";
pub const ERR_INIT_CONFIG_EXISTS: &str = "設定ファイルがすでに存在します: {}。上書きするには --force を使用してください";
pub const INFO_INIT_NOTES_DIR_DEFAULT: &str = "情報: --notes-dir が指定されていません。カレントディレクトリをスキャンします: {}";
pub const INFO_INIT_USE_NOTES_DIR: &str = "情報: --notes-dir <PATH> で別のボールトルートを指定できます";
pub const ERR_INIT_NOTES_DIR_MISSING: &str = "ノートディレクトリが存在しません: {}";
pub const ERR_INIT_NO_TTY: &str = "対話モードには TTY が必要です。--yes ですべての除外候補を自動承認するか、ターミナルで実行してください";
pub const INFO_INIT_AUTO_ACCEPT: &str = "情報: {} 件の除外候補を自動承認します";
pub const INIT_CONFIG_CREATED: &str = "設定ファイルを作成しました: {}";
pub const INIT_EXCLUDED_DIRS: &str = "{} 個のディレクト{}をインデックスから除外しました";
pub const INIT_NEXT_STEP: &str = "次に `shiotsuchi chart` を実行してボールトをインデックスしてください";
pub const INIT_BACKED_UP: &str = "既存の設定をバックアップしました: {}";

// ──────────────────────────────────────────────
// setup.rs — セットアップ
// ──────────────────────────────────────────────

pub const SETUP_CHECK_HELP: &str = "変更を行わずにセットアップ状態を確認する";
pub const SETUP_MODEL_FOUND: &str = "埋め込みモデルが見つかりました: {}";
pub const SETUP_MODEL_SIZE: &str = "  サイズ: {:.1} MB";
pub const SETUP_CHECKSUM_OK: &str = "  チェックサム: OK（SHA-256 が期待値と一致）";
pub const SETUP_CHECKSUM_MISMATCH: &str = "  チェックサム: 不一致 — モデルファイルが破損しているか、異なるソースからのものです";
pub const SETUP_CHECKSUM_EXPECTED: &str = "  期待される SHA-256: {}";
pub const SETUP_CHECKSUM_ERROR: &str = "  チェックサム: ハッシュ計算エラー: {}";
pub const SETUP_CHECKSUM_SKIPPED: &str = "  チェックサム: スキップ（期待ハッシュが設定されていません）";
pub const SETUP_SEMANTIC_AVAILABLE: &str = "セマンティック検索が利用可能です。";
pub const SETUP_MODEL_NOT_FOUND: &str = "埋め込みモデルが見つかりません。";
pub const SETUP_EXPECTED_LOCATION: &str = "期待される場所: {}";
pub const SETUP_CHECK_ALSO_ENV: &str = "（SHIOTSUCHI_EMBED_MODEL_PATH が設定されている場合はそれもチェックされます）";
pub const SETUP_RUN_SETUP: &str = "セットアップ手順を表示するには `shiotsuchi setup`（--check なし）を実行してください";
pub const SETUP_TITLE: &str = "Shiotsuchi セットアップ — セマンティック検索モデル";
pub const SETUP_STEPS_INTRO: &str = "ベクトル検索とハイブリッド検索を有効にするには、ONNX 埋め込みモデルファイルを以下の場所に配置してください:\n  {}";
pub const SETUP_CREATE_DIR: &str = "  1. ディレクトリを作成:\n     mkdir -p {}";
pub const SETUP_DOWNLOAD_STEPS: &str = "  2. モデルをダウンロードして '{}' に保存（データファイル: {}）";
pub const SETUP_EXPECTED_HASH: &str = "     期待される SHA-256: {}";
pub const SETUP_VERIFY_STEPS: &str = "  3. セットアップを確認:\n     shiotsuchi setup --check";
pub const SETUP_ALT_ENV: &str = "または、SHIOTSUCHI_EMBED_MODEL_PATH 環境変数をモデルファイルのパスに設定してください。";
pub const SETUP_DIR_CREATED: &str = "ディレクトリを作成しました: {}";

// ──────────────────────────────────────────────
// config.rs — 設定サブコマンド
// ──────────────────────────────────────────────

pub const CONFIG_NOTES_DIR_HELP: &str = "スキャンするボールトルート（デフォルトは設定済みの全ボールト）";
pub const CONFIG_NO_CANDIDATES: &str = "{} に除外候補は見つかりませんでした";
pub const CONFIG_CANDIDATES_HEADER: &str = "{} の除外候補:";
pub const CONFIG_CANDIDATE_ITEM: &str = "  {}. {} [{}] ({} ファイ{})";
pub const CONFIG_RUN_INIT: &str = "これらの除外設定を反映するには `shiotsuchi init --force` を実行してください";
pub const CONFIG_MANUAL_HINT: &str = "または設定ファイルの [indexing] セクションに手動で追加してください";

// ──────────────────────────────────────────────
// config_migrate.rs — 設定移行
// ──────────────────────────────────────────────

pub const CONFIG_MIGRATE_HELP: &str = "設定ファイルのパス（デフォルト: XDG デフォルト）";
pub const ERR_CONFIG_NOT_FOUND: &str = "設定ファイルが見つかりません: {}";
pub const ERR_CONFIG_ALREADY_NEW: &str = "設定はすでに新しい形式です — 移行は不要です";
pub const CONFIG_MIGRATED: &str = "設定の移行が完了しました。";
pub const CONFIG_MIGRATE_BACKUP: &str = "バックアップを保存しました: {}";
pub const CONFIG_MIGRATE_NEW: &str = "新しい形式を書き込みました: {}";

// ──────────────────────────────────────────────
// log.rs — ファイル一覧
// ──────────────────────────────────────────────

pub const LOG_NO_FILES: &str = "まだインデックスされたファイルはありません。先に `shiotsuchi chart` を実行してください";
pub const LOG_HEADER: &str = "{:<60} パス";
pub const LOG_TOTAL: &str = "\n合計: {} ファイル";

// ──────────────────────────────────────────────
// main.rs — DB パス移行
// ──────────────────────────────────────────────

pub const DB_PATH_MIGRATION_NOTICE: &str = "\
[情報] デフォルトのデータベース保存先が変わりました。
  以前: ~/.cache/shiotsuchi/db.sqlite3
  今後: {}（OS のセキュアなアプリケーションデータ領域）

古いデータベースファイルが見つかりました。新しい場所に移行するには:
  1. `shiotsuchi doctor` を実行してデータベースを診断
  2. その後 `shiotsuchi clean` で再インデックスしてください

手動で移行する場合はファイルをコピーしてください:
  cp {} {}

または、古い設定のまま config.toml の [database] セクションで db_path を直接指定することもできます。";

// ──────────────────────────────────────────────
// doctor.rs — 診断
// ──────────────────────────────────────────────

pub const DOCTOR_CONFIG_OK: &str = "[OK] 設定: {}";
pub const DOCTOR_CONFIG_ERROR: &str = "[!!] 設定: {}（パースエラー: {}）";
pub const DOCTOR_CONFIG_FIX_PROMPT: &str = "[indexing] から不明なフィールド '{}' を削除しますか？";
pub const DOCTOR_CONFIG_FIXED: &str = "[OK] 設定: 修正完了";
pub const DOCTOR_CONFIG_FIX_FAILED: &str = "[!!] 設定: 修正に失敗しました: {}";
pub const DOCTOR_CONFIG_OLD_FORMAT: &str = "[..] 設定: {}（古い [vault] 形式 — 移行を推奨）";
pub const DOCTOR_CONFIG_MIGRATE_PROMPT: &str = "設定が古い [vault] 形式です。新しい形式に移行しますか？";
pub const DOCTOR_CONFIG_MIGRATED: &str = "[OK] 設定: 移行完了";
pub const DOCTOR_CONFIG_MIGRATE_FAILED: &str = "[!!] 設定: 移行に失敗しました: {}";
pub const DOCTOR_CONFIG_NOT_FOUND: &str = "[..] 設定: {}（見つかりません — デフォルトを使用します）";

pub const DOCTOR_DB_OK: &str = "[OK] データベース: {}（{} ファイル、{} チャンク）";
pub const DOCTOR_DB_STATS_FAILED: &str = "[!!] データベース: {}（開けましたが統計取得に失敗: {}）";
pub const DOCTOR_DB_REBUILD_PROMPT: &str = "データベースをゼロから再構築しますか？";
pub const DOCTOR_DB_REBUILT: &str = "[OK] データベース: 再構築完了（{} ファイル、{} チャンク）";
pub const DOCTOR_DB_REBUILD_FAILED: &str = "[!!] データベース: 再構築に失敗しました: {}";
pub const DOCTOR_DB_OPEN_FAILED: &str = "[!!] データベース: {}（開けませんでした: {}）";
pub const DOCTOR_DB_NOT_FOUND: &str = "[..] データベース: {}（見つかりません — `shiotsuchi chart` を実行）";
pub const DOCTOR_DB_CREATE_PROMPT: &str = "今すぐボールトをインデックスしますか？";
pub const DOCTOR_DB_CREATED: &str = "[OK] データベース: 作成完了（{} ファイル、{} チャンク）";
pub const DOCTOR_DB_INDEX_FAILED: &str = "[!!] データベース: インデックスに失敗しました: {}";

pub const DOCTOR_TOKENIZER_OK: &str = "[OK] トークナイザー: Vaporetto モデル読み込み完了";
pub const DOCTOR_TOKENIZER_FALLBACK: &str = "[..] トークナイザー: {}（FTS フォールバック）";
pub const DOCTOR_EMBEDDER_OK: &str = "[OK] 埋め込みモデル: ONNX モデル読み込み完了";
pub const DOCTOR_EMBEDDER_LOAD_FAILED: &str = "[..] 埋め込みモデル: 見つかりましたが読み込みに失敗しました: {}";
pub const DOCTOR_EMBEDDER_NOT_FOUND: &str = "[..] 埋め込みモデル: ONNX モデルが見つかりません（ベクトル検索は無効）";
pub const DOCTOR_EMBEDDER_HINT: &str = "     [ヒント] `shiotsuchi setup` を実行して埋め込みモデルをインストールしてください";

pub const DOCTOR_VAULT_NONE: &str = "[..] ボールト: 設定されていません";
pub const DOCTOR_VAULT_OK: &str = "[OK] ボールト '{}': {}";
pub const DOCTOR_VAULT_ERROR: &str = "[!!] ボールト '{}': {}";
pub const DOCTOR_VAULT_NOT_EXIST: &str = "     [ヒント] ディレクトリが存在しません。正しいパスを設定するか、ディレクトリを作成してください";

pub const DOCTOR_ALL_PASSED: &str = "\nすべてのチェックが合格しました。";
pub const DOCTOR_SOME_FAILED: &str = "\nいくつかのチェックに失敗しました。上記のメッセージを確認してください。";

pub const DOCTOR_BACKUP_SAVED: &str = "  バックアップを保存しました: {}";
