use clap::Args;
use shiotsuchi_core::{db::NoteDatabase, indexer::cleanup_deleted, models::IndexConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct DredgeArgs {
    /// Print what would be removed without making changes.
    #[arg(long)]
    pub dry_run: bool,

    /// Run VACUUM after cleanup to reclaim disk space.
    #[arg(long)]
    pub vacuum: bool,

    /// Purge files older than retention_days (from config) for each vault.
    /// Requires retention_days to be set in config; prints a message if not configured.
    #[arg(long)]
    pub expired: bool,
}

pub fn run_dredge(
    args: &DredgeArgs,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    indexing_cfg: &crate::config::IndexingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if !db_path.exists() {
        eprintln!("Error: database not found. Run `shiotsuchi chart` first.");
        std::process::exit(1);
    }

    let db = NoteDatabase::open(db_path)?;
    let config = IndexConfig {
        vaults: vaults.to_vec(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
    };

    // Handle --expired flag
    if args.expired {
        if let Some(ref retention) = indexing_cfg.retention_days {
            // Build retention_days map: use the global retention_days value for all vaults
            let mut retention_map: HashMap<String, u32> = HashMap::new();
            for (vault_name, _) in vaults {
                retention_map.insert(vault_name.clone(), *retention);
            }
            let purged = db.purge_expired(&retention_map)?;
            if purged == 0 {
                println!("No expired files found.");
            } else {
                println!("Purged {} expired file(s).", purged);
            }
        } else {
            println!("retention_days not configured. Set retention_days in config.toml to use --expired.");
        }
        return Ok(());
    }

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
            expired: false,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg);
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
            expired: false,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg);
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
            expired: false,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dredge_expired_without_config() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        NoteDatabase::open(&db_file).unwrap();

        let args = DredgeArgs {
            dry_run: false,
            vacuum: false,
            expired: true,
        };
        // Default config has retention_days = None
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dredge_expired_with_config() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        let db = NoteDatabase::open(&db_file).unwrap();

        // Insert a file with an old mtime (100 days ago)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let old_mtime = now - (100 * 86400); // 100 days ago

        // Create a chunk and file_cache entry with old mtime
        let chunk = shiotsuchi_core::models::Chunk {
            id: None,
            file_path: "old.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "old content".into(),
            tokenized_content: "old content".into(),
            vault_name: "default".into(),
        };
        db.insert_chunks(&[chunk]).unwrap();
        db.upsert_file_cache("default", "old.md", "hash_old", old_mtime, "none").unwrap();

        // Also insert a recent file (should not be purged)
        let recent_chunk = shiotsuchi_core::models::Chunk {
            id: None,
            file_path: "recent.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "recent content".into(),
            tokenized_content: "recent content".into(),
            vault_name: "default".into(),
        };
        db.insert_chunks(&[recent_chunk]).unwrap();
        db.upsert_file_cache("default", "recent.md", "hash_recent", now, "none").unwrap();

        let args = DredgeArgs {
            dry_run: false,
            vacuum: false,
            expired: true,
        };
        let idx_cfg = IndexingConfig {
            retention_days: Some(30), // 30 days retention
            ..Default::default()
        };
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg);
        assert!(result.is_ok());

        // Verify old file was purged, recent file remains
        assert!(db.cached_hash("default", "old.md").unwrap().is_none(), "old file should be purged");
        assert!(db.cached_hash("default", "recent.md").unwrap().is_some(), "recent file should remain");
    }
}
