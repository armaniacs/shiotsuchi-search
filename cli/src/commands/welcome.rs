//! Interactive welcome screen and onboarding wizard.
//!
//! When `shiotsuchi` is run without a subcommand, this module handles:
//! - Detecting missing config → prompting to create it
//! - Showing a categorized command menu (TTY)
//! - Running a 3-step onboarding wizard (init → index → search)
//! - Showing text guidance (non-TTY)

use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use crate::commands;
use crate::config::{default_config_path, ShiotsuchiConfig};
use crate::messages;

// ──────────────────────────────────────────────
// Menu definition
// ──────────────────────────────────────────────

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

// ──────────────────────────────────────────────
// Banner display
// ──────────────────────────────────────────────

/// Display the welcome banner with categorized command listing.
/// Content adapts based on config/DB existence state.
fn show_banner(config_exists: bool, db_exists: bool) {
    let version = format!("Shiotsuchi Search  v{}", env!("CARGO_PKG_VERSION"));
    let inner_w: usize = 50;
    let pad_v = inner_w.saturating_sub(version.chars().count());
    let left_v = pad_v / 2;
    let right_v = pad_v - left_v;

    println!("╔{}╗", "═".repeat(inner_w));
    println!("║{}{}{}║", " ".repeat(left_v), version, " ".repeat(right_v));
    println!("║  {}  ║", messages::WELCOME_TAGLINE);
    println!("║{}║", " ".repeat(inner_w));

    if !config_exists {
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

    // Category listing (informational, not selectable — Select menu follows)
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

/// Run the welcome/guidance screen when no subcommand is given.
///
/// - Non-TTY: prints a brief guidance message and exits with code 0.
/// - TTY: shows interactive banner and categorized command menu.
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

    // ── TTY: show banner and menu loop ──
    let mut config_exists = config_path.exists();
    let mut db_exists = db_path.exists();
    show_banner(config_exists, db_exists);

    loop {
        let items = menu_items(config_exists, db_exists);
        let selection = dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
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
                if let Err(e) = run_onboarding(
                    config_exists, db_exists, cfg, &config_path,
                    raw_notes_dir, raw_db_path,
                ) {
                    eprintln!("⚠️ オンボーディング中にエラーが発生しました: {}", e);
                }
                *cfg = ShiotsuchiConfig::load();
                config_exists = config_path.exists();
                db_exists = cfg.resolved_db_path().exists();
                show_banner(config_exists, db_exists);
            }
            _ => {
                if let Err(e) = run_single_command(
                    choice, cfg, &config_path, raw_notes_dir, raw_db_path,
                ) {
                    eprintln!("⚠️ エラー: {}", e);
                }
            }
        }
    }

    Ok(())
}

// ──────────────────────────────────────────────
// Command dispatch and onboarding
// ──────────────────────────────────────────────

/// Build a `DiveArgs` with default search settings.
/// Shared between `run_onboarding` Step 3 and `run_single_command(Search)`.
fn build_search_args(query: String) -> commands::dive::DiveArgs {
    commands::dive::DiveArgs {
        query,
        json: false,
        limit: 20,
        format: commands::dive::OutputFormat::Table,
        mode: commands::dive::CliSearchMode::Hybrid,
        model_path: None,
        vault: None,
        tag: None,
        since: None,
        fuzzy: false,
        alpha: None,
        mmr: false,
        lambda: 0.5,
        threshold: None,
    }
}

/// Run the 3-step onboarding wizard: init → index → search.
/// Each step shows a pre-flight summary and asks for confirmation.
/// Steps are skipped based on config_exists/db_exists state.
fn run_onboarding(
    config_exists: bool,
    db_exists: bool,
    cfg: &ShiotsuchiConfig,
    config_path: &Path,
    raw_notes_dir: Option<&Path>,
    raw_db_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    use dialoguer::Confirm;
    use dialoguer::theme::ColorfulTheme;

    // ── Step 1: Config ──
    if !config_exists {
        println!("\n🔰 Step 1/3: 設定ファイルを作成します");
        println!("  作成先: {}", config_path.display());
        let notes_dir = raw_notes_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| {
            cfg.resolved_vaults().first()
                .map(|(_, d)| d.clone())
                .unwrap_or_else(|| PathBuf::from("."))
        });
        println!("  ノート: {}", notes_dir.display());

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
        let vault_display = cfg.resolved_vaults().first()
            .map(|(_, d)| d.display().to_string())
            .unwrap_or_else(|| ".".to_string());
        println!("  ボールト: {}", vault_display);

        // Check if API-based embedder is configured → add cost warning
        if let shiotsuchi_core::config::EmbedderConfig::Api { endpoint, ..} = &cfg.embedder {
            println!("  ⚠️  埋め込みに外部 API を使用します: {}", endpoint);
            println!("  💰  チャンク単位で課金が発生する可能性があります。");
        }

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
        if let shiotsuchi_core::config::EmbedderConfig::Api { endpoint, ..} = &cfg.embedder {
            println!("  ⚠️  埋め込みに外部 API を使用します: {}", endpoint);
            println!("  💰  チャンク単位で課金が発生する可能性があります。");
        }
        if !Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("データベースが存在します。再インデックスしますか？")
            .default(false)
            .interact()?
        {
            // Skip re-index, proceed to Step 3
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
    let args = build_search_args(query);
    let start = std::time::Instant::now();
    let results = commands::dive::run_dive(
        &args, &db_path, &cfg.resolved_vaults(),
        &cfg.indexing.user_dictionary, &cfg.synonyms,
        args.fuzzy, args.alpha, args.mmr, args.lambda, args.threshold,
    )?;
    commands::dive::print_results(&results, &args.query, &args.format, start.elapsed());

    // ── Completion screen ──
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

/// Execute a single menu command (non-onboarding).
/// To be implemented in a subsequent task.
fn run_single_command(
    choice: MenuChoice,
    cfg: &ShiotsuchiConfig,
    config_path: &Path,
    raw_notes_dir: Option<&Path>,
    raw_db_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    use dialoguer::Confirm;
    use dialoguer::theme::ColorfulTheme;

    match choice {
        MenuChoice::Stats => {
            commands::tide::run_tide(&cfg.resolved_db_path())?;
            println!();
        }
        MenuChoice::Doctor => {
            commands::doctor::run_doctor(
                cfg, &cfg.resolved_db_path(),
                &cfg.resolved_vaults(), &cfg.indexing, &cfg.vlm,
            )?;
            println!("✅ 診断が完了しました。問題があれば表示されたメッセージに従ってください");
        }
        MenuChoice::Init => {
            let init_args = commands::init::InitArgs { force: false, yes: false };
            commands::init::run_init(&init_args, cfg, config_path, raw_notes_dir, raw_db_path)?;
            if Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("✅ 設定ファイルを作成しました。オンボーディングを続けて index → search まで完了しませんか？")
                .default(true)
                .interact()?
            {
                run_onboarding(true, false, cfg, config_path, raw_notes_dir, raw_db_path)?;
            }
        }
        MenuChoice::Setup => {
            let setup_args = commands::setup::SetupArgs { check: false };
            commands::setup::run_setup(&setup_args)?;
            println!("✅ モデルのセットアップが完了しました。次に index を実行してベクトルインデックスを有効にしてください");
        }
        MenuChoice::Index => {
            commands::chart::run_chart(
                &commands::chart::ChartArgs { vault: None, quiet: false, force: false },
                &cfg.resolved_vaults(), &cfg.resolved_db_path(),
                &cfg.indexing, &cfg.embedder, &cfg.vlm,
            )?;
            if Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt("✅ インデックスが完了しました。続けて search で検索してみませんか？")
                .default(true)
                .interact()?
            {
                run_onboarding(true, true, cfg, config_path, raw_notes_dir, raw_db_path)?;
            }
        }
        MenuChoice::Search => {
            let db_path = cfg.resolved_db_path();
            if !db_path.exists() {
                eprintln!("{}", crate::messages::ERR_DB_NOT_FOUND);
                if Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt("オンボーディングを開始して index → search まで進めますか？")
                    .default(true)
                    .interact()?
                {
                    run_onboarding(config_path.exists(), false, cfg, config_path, raw_notes_dir, raw_db_path)?;
                }
                return Ok(());
            }
            let query: String = dialoguer::Input::with_theme(&ColorfulTheme::default())
                .with_prompt("検索クエリを入力してください")
                .interact_text()?;

            let start = std::time::Instant::now();
            let args = build_search_args(query);
            let results = commands::dive::run_dive(
                &args, &db_path, &cfg.resolved_vaults(),
                &cfg.indexing.user_dictionary, &cfg.synonyms,
                args.fuzzy, args.alpha, args.mmr, args.lambda, args.threshold,
            )?;
            commands::dive::print_results(&results, &args.query, &args.format, start.elapsed());
        }
        MenuChoice::Onboarding | MenuChoice::Exit => {
            unreachable!("handled in menu loop")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_choice_from_index_mapping() {
        assert!(matches!(MenuChoice::from_index(0), MenuChoice::Onboarding));
        assert!(matches!(MenuChoice::from_index(1), MenuChoice::Init));
        assert!(matches!(MenuChoice::from_index(2), MenuChoice::Setup));
        assert!(matches!(MenuChoice::from_index(3), MenuChoice::Search));
        assert!(matches!(MenuChoice::from_index(4), MenuChoice::Index));
        assert!(matches!(MenuChoice::from_index(5), MenuChoice::Stats));
        assert!(matches!(MenuChoice::from_index(6), MenuChoice::Doctor));
        assert!(matches!(MenuChoice::from_index(7), MenuChoice::Exit));
        assert!(matches!(MenuChoice::from_index(99), MenuChoice::Exit)); // fallback
    }

    #[test]
    fn test_menu_items_length_is_8() {
        let items = menu_items(true, true);
        assert_eq!(items.len(), 8, "menu should have 8 items");
    }

    #[test]
    fn test_menu_items_onboarding_label_changes_with_state() {
        let no_config = menu_items(false, false);
        assert!(no_config[0].contains("オンボーディングを開始"), "no config: should suggest starting onboarding");

        let config_no_db = menu_items(true, false);
        assert!(config_no_db[0].contains("オンボーディングを続ける"), "config but no DB: should suggest continuing");

        let all_exists = menu_items(true, true);
        assert!(all_exists[0].contains("クイックオンボーディング"), "config+DB: should suggest quick onboarding");
    }

    #[test]
    fn test_stdin_is_not_terminal_in_test_env() {
        assert!(!std::io::stdin().is_terminal());
    }

    #[test]
    fn test_show_banner_always_shows_onboarding_option() {
        let items_cfg_no = menu_items(false, false);
        let items_db_no = menu_items(true, false);
        let items_all = menu_items(true, true);

        assert!(items_cfg_no[0].contains("オンボーディング"));
        assert!(items_db_no[0].contains("オンボーディング"));
        assert!(items_all[0].contains("オンボーディング"));
    }

    #[test]
    fn test_run_welcome_non_tty_path_still_works() {
        let mut cfg = ShiotsuchiConfig::default();
        let result = run_welcome(&mut cfg, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_banner_category_keywords_present() {
        let items = menu_items(true, true);
        assert!(items.iter().any(|i| i.contains("init")));
        assert!(items.iter().any(|i| i.contains("setup")));
        assert!(items.iter().any(|i| i.contains("search")));
        assert!(items.iter().any(|i| i.contains("index")));
        assert!(items.iter().any(|i| i.contains("stats")));
        assert!(items.iter().any(|i| i.contains("doctor")));
        assert!(items.iter().any(|i| i.contains("exit")));
    }

    // ── build_search_args ────────────────────────────────────────

    #[test]
    fn test_build_search_args_produces_expected_defaults() {
        let query = "project plan".to_string();
        let args = super::build_search_args(query.clone());
        assert_eq!(args.query, query);
        assert!(!args.json);
        assert_eq!(args.limit, 20);
        assert!(matches!(args.format, crate::commands::dive::OutputFormat::Table));
        assert!(matches!(args.mode, crate::commands::dive::CliSearchMode::Hybrid));
        assert!(args.model_path.is_none());
        assert!(args.vault.is_none());
        assert!(args.tag.is_none());
        assert!(args.since.is_none());
        assert!(!args.fuzzy);
        assert!(args.alpha.is_none());
        assert!(!args.mmr);
        assert_eq!(args.lambda, 0.5);
        assert!(args.threshold.is_none());
    }

    #[test]
    fn test_build_search_args_accepts_japanese_query() {
        let query = "日本語クエリ".to_string();
        let args = super::build_search_args(query.clone());
        assert_eq!(args.query, query);
    }
}
