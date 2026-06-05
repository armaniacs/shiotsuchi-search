use crate::messages;
use crate::msg_fmt;
use clap::Args;
use shiotsuchi_core::{db::NoteDatabase, indexer::cleanup_deleted, models::IndexConfig};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
#[command(about = crate::messages::DREDGE_ABOUT)]
pub struct DredgeArgs {
    #[arg(long, help = "実際には削除せず、削除対象を表示する")]
    pub dry_run: bool,

    #[arg(long, help = "クリーンアップ後に VACUUM を実行してディスク容量を解放する")]
    pub vacuum: bool,

    /// Purge files older than retention_days (from config) for each vault.
    /// Requires retention_days to be set in config; prints a message if not configured.
    #[arg(long)]
    pub expired: bool,

    /// Clear all VLM extraction hashes and reprocess PDFs on next index.
    #[arg(long)]
    pub force_vlm_reprocess: bool,
}

pub fn run_dredge(
    args: &DredgeArgs,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    indexing_cfg: &crate::config::IndexingConfig,
    vlm_cfg: &shiotsuchi_core::config::VlmConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if !db_path.exists() {
        eprintln!("{}", messages::DREDGE_DB_NOT_FOUND);
        std::process::exit(1);
    }

    let db = NoteDatabase::open(db_path)?;

    // Handle --force-vlm-reprocess flag
    if args.force_vlm_reprocess {
        let count = db.clear_vlm_hashes()?;
        println!("{}", msg_fmt!(messages::DREDGE_VLM_HASHES_CLEARED, count));
        return Ok(());
    }

    let config = IndexConfig {
        vaults: vaults.to_vec(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
        user_dictionary: indexing_cfg.user_dictionary.clone(),
        enable_pdf_extraction: indexing_cfg.enable_pdf_extraction,
        backlink_scoring: indexing_cfg.backlink_scoring,
        vlm_enabled: vlm_cfg.enabled,
        vlm_consent_obtained: vlm_cfg.consent_obtained,
        vlm_provider: vlm_cfg.provider.clone(),
        vlm_model: vlm_cfg.model.clone(),
        vlm_max_pages_per_doc: vlm_cfg.max_pages_per_doc,
        embedding_usage: indexing_cfg.embedding_usage.clone(),
    };

    // Handle --expired flag
    if args.expired {
        if let Some(ref retention) = indexing_cfg.retention_days {
            // Build retention_days map: use the global retention_days value for all vaults
            let mut retention_map: HashMap<String, u32> = HashMap::new();
            for (vault_name, _) in vaults {
                retention_map.insert(vault_name.clone(), *retention);
            }

            let total_expired = db.count_expired(&retention_map)?;

            if args.dry_run {
                if total_expired == 0 {
                    println!("{}", messages::DREDGE_EXPIRED_NONE);
                } else {
                    println!("{}", msg_fmt!(messages::DREDGE_EXPIRED_DRY_RUN, total_expired));
                }
            } else {
                if std::io::stdin().is_terminal() {
                    let theme = crate::util::dialoguer_theme();
                    let confirmed = dialoguer::Confirm::with_theme(&*theme)
                        .with_prompt(msg_fmt!(
                            messages::DREDGE_EXPIRED_CONFIRM,
                            total_expired
                        ))
                        .default(false)
                        .interact()?;

                    if !confirmed {
                        println!("{}", messages::DREDGE_ABORTED);
                        return Ok(());
                    }
                }

                let purged = db.purge_expired(&retention_map)?;
                if purged == 0 {
                    println!("{}", messages::DREDGE_EXPIRED_NONE);
                } else {
                    println!("{}", msg_fmt!(messages::DREDGE_EXPIRED_PURGED, purged));
                }
            }
        } else {
            println!("{}", messages::DREDGE_RETENTION_NOT_CONFIGURED);
        }
        return Ok(());
    }

    let stale = cleanup_deleted(&db, &config)?;

    if stale.is_empty() {
        println!("{}", messages::DREDGE_NO_STALE);
    } else if args.dry_run {
        println!("{}", msg_fmt!(messages::DREDGE_WOULD_REMOVE, stale.len()));
        for path in &stale {
            println!("  - {}", path);
        }
    } else {
        println!("{}", msg_fmt!(messages::DREDGE_REMOVED, stale.len()));
        for path in &stale {
            println!("  - {}", path);
        }
    }

    if args.vacuum && !args.dry_run {
        let conn = db.write_conn.borrow();
        conn.execute_batch("VACUUM")?;
        println!("{}", messages::DREDGE_VACUUM_DONE);
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
            force_vlm_reprocess: false,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg, &Default::default());
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
            force_vlm_reprocess: false,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg, &Default::default());
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
            force_vlm_reprocess: false,
        };
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg, &Default::default());
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
            force_vlm_reprocess: false,
        };
        // Default config has retention_days = None
        let idx_cfg = IndexingConfig::default();
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg, &Default::default());
        assert!(result.is_ok());
    }

    #[test]
    fn test_dredge_expired_with_config() {
        let temp = TempDir::new().unwrap();
        let db_file = temp.path().join("test.db");
        let db = NoteDatabase::open(&db_file).unwrap();

        // Insert a file with an old mtime (100 days ago)
        // Note: mtime is stored in MILLISECONDS (matching file_mtime() in indexer.rs)
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let old_mtime = now - (100 * 86_400_000); // 100 days ago in ms

        // Create a chunk and file_cache entry with old mtime
        let chunk = shiotsuchi_core::models::Chunk {
            id: None,
            file_path: "old.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "old content".into(),
            tokenized_content: "old content".into(),
            vault_name: "default".into(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
        };
        db.insert_chunks(&[chunk]).unwrap();
        db.upsert_file_cache_for_tests("default", "old.md", "hash_old", old_mtime, "none").unwrap();

        // Also insert a recent file (should not be purged)
        let recent_chunk = shiotsuchi_core::models::Chunk {
            id: None,
            file_path: "recent.md".into(),
            chunk_index: 0,
            parent_header: None,
            content: "recent content".into(),
            tokenized_content: "recent content".into(),
            vault_name: "default".into(),
            tags: String::new(),
            frontmatter_date: String::new(),
            title: String::new(),
            emphasized_text: String::new(),
        };
        db.insert_chunks(&[recent_chunk]).unwrap();
        db.upsert_file_cache_for_tests("default", "recent.md", "hash_recent", now, "none").unwrap();

        let args = DredgeArgs {
            dry_run: false,
            vacuum: false,
            expired: true,
            force_vlm_reprocess: false,
        };
        let idx_cfg = IndexingConfig {
            retention_days: Some(30), // 30 days retention
            ..Default::default()
        };
        let result = run_dredge(&args, &[("default".to_string(), temp.path().to_path_buf())], &db_file, &idx_cfg, &Default::default());
        assert!(result.is_ok());

        // Verify old file was purged, recent file remains
        assert!(db.cached_hash("default", "old.md").unwrap().is_none(), "old file should be purged");
        assert!(db.cached_hash("default", "recent.md").unwrap().is_some(), "recent file should remain");
    }
}
