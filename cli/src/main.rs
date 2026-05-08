mod commands;
mod config;

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
    Init(commands::init::InitArgs),
    Log,
    Scan(commands::scan::ScanArgs),
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
        Commands::Log => commands::log::run_log(&cfg.vault.db_path)?,
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
    #[test]
    fn test_version_flag_compiles() {
        assert!(true);
    }
}
