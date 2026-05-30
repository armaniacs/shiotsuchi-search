//! Interactive welcome screen and onboarding wizard.
//!
//! When `shiotsuchi` is run without a subcommand, this module handles:
//! - Detecting missing config → prompting to create it
//! - Showing a categorized command menu (TTY)
//! - Running a 3-step onboarding wizard (init → index → search)
//! - Showing text guidance (non-TTY)

use std::io::IsTerminal;
use std::path::Path;

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
/// - TTY: shows interactive banner and menu (to be implemented in later tasks).
pub fn run_welcome(
    cfg: &mut ShiotsuchiConfig,
    raw_notes_dir: Option<&Path>,
    raw_db_path: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = default_config_path();
    let is_tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();

    if !is_tty {
        // Non-TTY: show text guidance and exit
        if !config_path.exists() {
            eprintln!("{}", messages::WELCOME_NON_TTY_NO_CONFIG);
        } else {
            println!("{}", messages::WELCOME_NON_TTY_HINT);
        }
        return Ok(());
    }

    // TTY path — will be implemented in subsequent tasks
    eprintln!("TTY mode not yet implemented");
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
}
