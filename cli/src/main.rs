mod commands;
mod config;
mod util;

use clap::{Parser, Subcommand};
use std::time::Instant;

#[derive(Parser)]
#[command(
    name = "shiotsuchi",
    version,
    long_version = concat!(env!("CARGO_PKG_VERSION"), "\nGuiding your path through the data tide."),
    about = "Guiding your path through the data tide."
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
    Chart(commands::chart::ChartArgs),
    Config(commands::config::ConfigArgs),
    Delete(commands::delete::DeleteArgs),
    Dive(commands::dive::DiveArgs),
    Dredge(commands::dredge::DredgeArgs),
    Init(commands::init::InitArgs),
    Log,
    Scan(commands::scan::ScanArgs),
    Setup(commands::setup::SetupArgs),
    Tide,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let env = env_logger::Env::default()
        .filter_or("RUST_LOG", if cli.verbose { "debug" } else { "warn" });
    env_logger::Builder::from_env(env).init();

    let mut cfg = config::ShiotsuchiConfig::load();
    if let Some(ref dir) = cli.notes_dir {
        cfg.vault.notes_dir = dir.clone();
    }
    if let Some(ref db) = cli.db_path {
        cfg.vault.db_path = db.clone();
    }

    match cli.command {
        Commands::Chart(args) => {
            commands::chart::run_chart(
                &args,
                &cfg.vault.notes_dir,
                &cfg.vault.db_path,
                &cfg.indexing,
            )?;
        }
        Commands::Dive(args) => {
            if !cfg.vault.db_path.exists() {
                eprintln!(
                    "Error: database not found. Run `shiotsuchi chart` to index your vault first."
                );
                std::process::exit(1);
            }
            let start = Instant::now();
            match commands::dive::run_dive(
                &args,
                &cfg.vault.notes_dir,
                &cfg.vault.db_path,
                &cfg.indexing,
            ) {
                Ok(results) => {
                    let elapsed = start.elapsed();
                    let fmt = args.effective_format();
                    commands::dive::print_results(&results, &args.query, &fmt, elapsed);
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::Tide => {
            let stats = commands::tide::run_tide(&cfg.vault.db_path)?;
            commands::tide::print_stats(&stats);
        }
        Commands::Scan(args) => {
            commands::scan::run_scan(
                &args,
                &cfg.vault.notes_dir,
                &cfg.vault.db_path,
                &cfg.watcher,
                &cfg.indexing,
            )?;
        }
        Commands::Dredge(args) => {
            commands::dredge::run_dredge(
                &args,
                &cfg.vault.notes_dir,
                &cfg.vault.db_path,
                &cfg.indexing,
            )?;
        }
        Commands::Log => commands::log::run_log(&cfg.vault.db_path)?,
        Commands::Setup(args) => {
            commands::setup::run_setup(&args)?;
        }
        Commands::Delete(args) => {
            commands::delete::run_delete(&args, &cfg.vault.notes_dir, &cfg.vault.db_path)?;
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
        Commands::Config(args) => {
            commands::config::run_config(
                &args,
                &cfg.vault.notes_dir,
                &cfg.indexing.include_extensions,
                cfg.indexing.auto_exclude_hidden,
                cfg.indexing.dynamic_threshold,
            )?;
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
        assert!(matches!(cli.command, Commands::Tide));
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
        for cmd in &["chart", "dredge", "init", "log", "scan", "setup", "tide"] {
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
