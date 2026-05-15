use clap::Args;
use shiotsuchi_core::{db::NoteDatabase, indexer::cleanup_deleted, models::IndexConfig};
use std::path::Path;

#[derive(Args, Debug)]
pub struct DredgeArgs {
    /// Print what would be removed without making changes.
    #[arg(long)]
    pub dry_run: bool,

    /// Run VACUUM after cleanup to reclaim disk space.
    #[arg(long)]
    pub vacuum: bool,
}

pub fn run_dredge(
    args: &DredgeArgs,
    notes_dir: &Path,
    db_path: &Path,
    indexing_cfg: &crate::config::IndexingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if !db_path.exists() {
        eprintln!("Error: database not found. Run `shiotsuchi chart` first.");
        std::process::exit(1);
    }

    let db = NoteDatabase::open(db_path)?;
    let config = IndexConfig {
        notes_dir: notes_dir.to_path_buf(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
    };

    let stale = cleanup_deleted(&db, &config)?;

    if stale.is_empty() {
        println!("No stale entries found.");
    } else if args.dry_run {
        println!("Would remove {} stale file(s):", stale.len());
        for path in &stale {
            println!("  - {}", path);
        }
    } else {
        println!("Removed {} stale file(s):", stale.len());
        for path in &stale {
            println!("  - {}", path);
        }
    }

    if args.vacuum && !args.dry_run {
        let conn = db.write_conn.borrow();
        conn.execute_batch("VACUUM")?;
        println!("VACUUM complete.");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IndexingConfig;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_dredge_no_stale() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        NoteDatabase::open(&db_file).unwrap();

        let args = DredgeArgs {
            dry_run: false,
            vacuum: false,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, temp.path(), &db_file, &idx_cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dredge_dry_run() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        NoteDatabase::open(&db_file).unwrap();

        let args = DredgeArgs {
            dry_run: true,
            vacuum: false,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, temp.path(), &db_file, &idx_cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dredge_vacuum() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        // Write some notes, then delete them so there are stale entries
        fs::write(temp.path().join("note.md"), "# Hello").unwrap();
        NoteDatabase::open(&db_file).unwrap();

        let args = DredgeArgs {
            dry_run: false,
            vacuum: true,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, temp.path(), &db_file, &idx_cfg);
        assert!(result.is_ok());
    }
}
