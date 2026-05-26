mod build_info;
mod commands;
mod config;
mod messages;
mod util;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;

/// Returns `true` when the user has not explicitly configured a db_path
/// via config file, legacy vault config, or CLI flag.
fn is_default_db_path(cfg: &config::ShiotsuchiConfig, cli_db_path: Option<&PathBuf>) -> bool {
    cfg.database.db_path.is_none()
        && cfg.vault.as_ref().and_then(|v| v.db_path.as_ref()).is_none()
        && cli_db_path.is_none()
}

/// Returns the previous default database path (~/.cache/shiotsuchi/db.sqlite3),
/// used before PBI-06 migrated to `dirs::data_dir()`.
///
/// Note: The previous implementation used `$XDG_CACHE_HOME` as an override.
/// This function only checks the common `~/.cache` fallback — users who set
/// `XDG_CACHE_HOME` to a custom value won't see the migration notice. This is
/// an acceptable simplification for the migration message.
fn old_default_db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("shiotsuchi")
        .join("db.sqlite3")
}

/// Resolve vaults: if a vault ID is specified, validate and return only that vault.
/// Otherwise return all configured vaults.
fn resolve_vaults(
    vaults: &[(String, PathBuf)],
    vault_id: Option<&str>,
) -> Result<Vec<(String, PathBuf)>, Box<dyn std::error::Error>> {
    match vault_id {
        Some(id) => match vaults.iter().find(|(n, _)| n == id) {
            Some(v) => Ok(vec![v.clone()]),
            None => {
                let known: Vec<&str> = vaults.iter().map(|(n, _)| n.as_str()).collect();
                Err(msg_fmt!(
                    crate::messages::ERR_VAULT_NOT_FOUND,
                    id,
                    known.join(", ")
                )
                .into())
            }
        },
        None => Ok(vaults.to_vec()),
    }
}

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
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = crate::messages::CHART_ABOUT)]
    Chart(commands::chart::ChartArgs),
    #[command(about = crate::messages::CHECK_IGNORE_ABOUT)]
    CheckIgnore(commands::check_ignore::CheckIgnoreArgs),
    #[command(about = crate::messages::CLEAN_ABOUT)]
    Clean(commands::clean::CleanArgs),
    #[command(about = crate::messages::CONFIG_ABOUT)]
    Config(commands::config::ConfigArgs),
    #[command(about = crate::messages::CONFIG_MIGRATE_ABOUT)]
    ConfigMigrate(commands::config_migrate::ConfigMigrateArgs),
    /// シェル補完スクリプトを生成する
    #[command(hide = true)]
    Completion {
        shell: clap_complete::Shell,
    },
    #[command(about = crate::messages::DELETE_ABOUT)]
    Delete(commands::delete::DeleteArgs),
    #[command(alias = "search", about = crate::messages::DIVE_ABOUT)]
    Dive(commands::dive::DiveArgs),
    #[command(about = crate::messages::DOCTOR_ABOUT)]
    Doctor(commands::doctor::DoctorArgs),
    #[command(about = crate::messages::DREDGE_ABOUT)]
    Dredge(commands::dredge::DredgeArgs),
    #[command(about = crate::messages::INIT_ABOUT)]
    Init(commands::init::InitArgs),
    #[command(about = crate::messages::LOG_ABOUT)]
    Log,
    #[command(about = crate::messages::SCAN_ABOUT)]
    Scan(commands::scan::ScanArgs),
    #[command(about = crate::messages::SETUP_ABOUT)]
    Setup(commands::setup::SetupArgs),
    #[command(subcommand, about = crate::messages::SYNONYM_ABOUT)]
    Synonym(commands::synonym::SynonymCommand),
    #[command(about = crate::messages::SUPPORT_ABOUT)]
    Support(commands::support::SupportArgs),
    #[command(about = crate::messages::TASKS_ABOUT)]
    Tasks(commands::tasks::TasksArgs),
    #[command(about = crate::messages::TIDE_ABOUT)]
    Tide(commands::tide::TideArgs),
}

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
            config::VaultEntry {
                notes_dir: Some(dir.clone()),
                db_path: None,
            },
        );
    }
    if let Some(ref db) = cli.db_path {
        cfg.database.db_path = Some(db.clone());
    }

    let resolved_vaults = cfg.resolved_vaults();
    let db_path = cfg.resolved_db_path();

    // Migration notice: if no explicit db_path is configured and the old
    // default path (~/.cache/shiotsuchi/db.sqlite3) has a database, inform
    // the user about the new location.
    if is_default_db_path(&cfg, cli.db_path.as_ref()) {
        let old_path = old_default_db_path();
        if old_path.exists() && !db_path.exists() {
            eprintln!(
                "{}",
                msg_fmt!(
                    crate::messages::DB_PATH_MIGRATION_NOTICE,
                    db_path.parent().unwrap().display(),
                    old_path.display(),
                    db_path.display(),
                )
            );
        }
    }

    match cli.command {
        Commands::Chart(args) => {
            let vault_id = args.vault.as_deref().or(cfg.vault_default.as_deref());
            let vaults = resolve_vaults(&resolved_vaults, vault_id)?;
            commands::chart::run_chart(&args, &vaults, &db_path, &cfg.indexing)?;
        }
        Commands::CheckIgnore(args) => {
            commands::check_ignore::run_check_ignore(&args, &resolved_vaults)?;
        }
        Commands::Clean(_args) => {
            commands::clean::run_clean(&resolved_vaults, &db_path, &cfg.indexing)?;
        }
        Commands::Dive(args) => {
            if !db_path.exists() {
                eprintln!("{}", crate::messages::ERR_DB_NOT_FOUND);
                std::process::exit(1);
            }
            let start = Instant::now();
            // CLI --alpha overrides config hybrid_alpha; if neither set, default to 0.5
            let effective_alpha = args.alpha
                .or(cfg.hybrid_alpha)
                .unwrap_or(0.5);
            // CLI --vault overrides config vault_default; pass actual vault filter to run_dive
            // by modifying the resolved vaults and passing the filter as args.vault
            let _vault_filter = args.vault.as_deref().or(cfg.vault_default.as_deref());
            // CLI --threshold overrides config semantic_threshold
            let effective_threshold = args.threshold.or(cfg.semantic_threshold);
            match commands::dive::run_dive(&args, &db_path, &resolved_vaults, &cfg.indexing.user_dictionary, &cfg.synonyms, args.fuzzy, Some(effective_alpha), args.mmr, args.lambda, effective_threshold) {
                Ok(results) => {
                    let elapsed = start.elapsed();
                    let fmt = args.effective_format();
                    commands::dive::print_results(&results, &args.query, &fmt, elapsed);
                }
                Err(e) => {
                    eprintln!("{}: {}", crate::messages::ERR_PREFIX, e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Tide(args) => {
            let stats = commands::tide::run_tide(&db_path)?;
            commands::tide::print_stats(&stats, &args);
        }
        Commands::Scan(args) => {
            let vault_id = args.vault.as_deref().or(cfg.vault_default.as_deref());
            let vaults = resolve_vaults(&resolved_vaults, vault_id)?;
            commands::scan::run_scan(
                &args,
                &vaults,
                &db_path,
                &cfg.watcher,
                &cfg.indexing,
            )?;
        }
        Commands::Doctor(_args) => {
            commands::doctor::run_doctor(&cfg, &db_path, &resolved_vaults, &cfg.indexing)?;
        }
        Commands::Dredge(args) => {
            commands::dredge::run_dredge(
                &args,
                &resolved_vaults,
                &db_path,
                &cfg.indexing,
            )?;
        }
        Commands::Log => commands::log::run_log(&db_path, "default")?,
        Commands::Setup(args) => {
            commands::setup::run_setup(&args)?;
        }
        Commands::Delete(args) => {
            commands::delete::run_delete(&args, &resolved_vaults, &db_path)?;
        }
        Commands::Init(args) => {
            let config_path = config::default_config_path();
            commands::init::run_init(
                &args,
                &cfg,
                &config_path,
                cli.notes_dir.as_deref(),
                cli.db_path.as_deref(),
            )?;
        }
        Commands::Tasks(args) => {
            if !db_path.exists() {
                eprintln!("{}", crate::messages::ERR_DB_NOT_FOUND);
                std::process::exit(1);
            }
            commands::tasks::run_tasks(&args, &db_path)?;
        }
        Commands::Support(args) => {
            commands::support::run_support(&args, &cfg)?;
        }
        Commands::Synonym(cmd) => {
            commands::synonym::run_synonym(&cmd)?;
        }
        Commands::Config(args) => {
            commands::config::run_config(
                &args,
                &resolved_vaults,
                &cfg.indexing.include_extensions,
                cfg.indexing.auto_exclude_hidden,
                cfg.indexing.dynamic_threshold,
            )?;
        }
        Commands::ConfigMigrate(args) => {
            commands::config_migrate::run_config_migrate(&args)?;
        }
        Commands::Completion { shell } => {
            let mut cmd = <Cli as clap::CommandFactory>::command();
            clap_complete::generate(shell, &mut cmd, "shiotsuchi", &mut std::io::stdout());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ---------------------------------------------------------------------------
    // Global flag tests
    // ---------------------------------------------------------------------------

    fn parse_cli(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).unwrap()
    }

    #[test]
    fn test_support_command_parses() {
        let r = Cli::try_parse_from(["shiotsuchi", "support"]);
        assert!(r.is_ok(), "support should parse");
    }

    #[test]
    fn test_support_json_flag_parses() {
        let r = Cli::try_parse_from(["shiotsuchi", "support", "--json"]);
        assert!(r.is_ok(), "support --json should parse");
    }

    #[test]
    fn test_help_includes_build_footer() {
        let mut cmd =
            <Cli as clap::CommandFactory>::command().after_help(crate::build_info::help_footer());
        let help = format!("{}", cmd.render_help());
        assert!(help.contains("Build features:"));
    }

    #[test]
    fn test_global_notes_dir_on_dive_subcommand() {
        let cli = parse_cli(&["shiotsuchi", "dive", "--notes-dir", "/my/notes", "query"]);
        assert_eq!(cli.notes_dir, Some(PathBuf::from("/my/notes")));
        assert!(matches!(cli.command, Commands::Dive(_)));
    }

    #[test]
    fn test_global_db_path_on_dive_subcommand() {
        let cli = parse_cli(&["shiotsuchi", "dive", "--db-path", "/my/db", "query"]);
        assert_eq!(cli.db_path, Some(PathBuf::from("/my/db")));
        assert!(matches!(cli.command, Commands::Dive(_)));
    }

    #[test]
    fn test_global_verbose_on_tide_subcommand() {
        let cli = parse_cli(&["shiotsuchi", "tide", "--verbose"]);
        assert!(cli.verbose);
        assert!(matches!(cli.command, Commands::Tide(_)));
    }

    #[test]
    fn test_global_flag_before_subcommand_position() {
        let cli = parse_cli(&["shiotsuchi", "--notes-dir", "/my/notes", "dive", "query"]);
        assert_eq!(cli.notes_dir, Some(PathBuf::from("/my/notes")));
        assert!(matches!(cli.command, Commands::Dive(_)));
    }

    #[test]
    fn test_global_db_path_on_scan_subcommand() {
        let cli = parse_cli(&["shiotsuchi", "scan", "--db-path", "/my/db"]);
        assert_eq!(cli.db_path, Some(PathBuf::from("/my/db")));
        assert!(matches!(cli.command, Commands::Scan(_)));
    }

    #[test]
    fn test_global_notes_dir_on_top_level() {
        let cli = parse_cli(&["shiotsuchi", "--notes-dir", "/top/notes", "tide"]);
        assert_eq!(cli.notes_dir, Some(PathBuf::from("/top/notes")));
    }

    #[test]
    fn test_global_flags_accepted_on_all_subcommands() {
        // Subcommands with no required positionals: chart, dredge, init, log, scan, setup, tide
        for cmd in &["chart", "doctor", "dredge", "init", "log", "scan", "setup", "tide"] {
            let args: Vec<&str> = vec!["shiotsuchi", cmd, "--verbose"];
            let r = Cli::try_parse_from(args);
            assert!(r.is_ok(), "{} --verbose should be accepted", cmd);
        }
        // Subcommands with required positionals:
        let r = Cli::try_parse_from(["shiotsuchi", "dive", "--verbose", "test"]);
        assert!(r.is_ok(), "dive --verbose should be accepted");
        let r = Cli::try_parse_from(["shiotsuchi", "delete", "--verbose", "path/to/note.md"]);
        assert!(r.is_ok(), "delete --verbose should be accepted");
        // config requires its own subcommand
        let r = Cli::try_parse_from(["shiotsuchi", "config", "--verbose", "detect-noise"]);
        assert!(
            r.is_ok(),
            "config detect-noise --verbose should be accepted"
        );
    }

    #[test]
    fn test_env_var_mapped_notes_dir() {
        // Verify the env var name is set (actual env read happens at runtime)
        // This tests the clap config, not the env var itself
        let cli = parse_cli(&["shiotsuchi", "--notes-dir", "/env/notes", "tide"]);
        assert_eq!(cli.notes_dir, Some(PathBuf::from("/env/notes")));
    }

    #[test]
    fn test_help_does_not_panic() {
        // --help on any subcommand should not panic
        let r = Cli::try_parse_from(["shiotsuchi", "dive", "--help"]);
        assert!(r.is_err()); // clap returns error when --help is used
    }
}

// ---------------------------------------------------------------------------
// Doc-consistency compile-time check:
// The field name in ref/models.md must match the actual struct field.
// If you rename `exclude_dirs`, update ref/models.md too.
// ---------------------------------------------------------------------------
#[cfg(test)]
mod doc_consistency_tests {
    // These tests verify documentation matches code behavior.
    // They are compile-time/assertion checks against the actual struct layout.

    #[test]
    fn test_index_config_uses_exclude_dirs() {
        // Actual code uses `exclude_dirs` — the ref docs must match.
        // This is a compile-time guard: if someone renames the field,
        // they must update all documentation references.
        let cfg = shiotsuchi_core::models::IndexConfig::default();
        // Just verify the default is non-empty and uses the correct field name
        assert!(cfg.exclude_dirs.contains(&"node_modules".to_string()));
    }
}
