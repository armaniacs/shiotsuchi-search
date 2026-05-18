use crate::config::IndexingConfig;
use clap::Args;
use shiotsuchi_core::{
    db::NoteDatabase,
    embedder::{resolve_model_path, Embedder},
    indexer::{index_directory, IndexResult},
    models::IndexConfig,
    tokenizer::get_tokenizer,
};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Args, Debug)]
pub struct CleanArgs {}

/// Copy a single file to `<path>.bak.<timestamp>`.
/// Returns the backup path if the original existed, None otherwise.
fn backup_file(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let backup_name = format!("{}.bak.{}", path.to_string_lossy(), ts);
    let backup_path = PathBuf::from(&backup_name);
    match std::fs::copy(path, &backup_path) {
        Ok(_) => {
            #[cfg(unix)]
            if let Ok(meta) = std::fs::metadata(path) {
                let _ = std::fs::set_permissions(&backup_path, meta.permissions());
            }
            Some(backup_path)
        }
        Err(e) => {
            eprintln!("Warning: failed to backup {}: {}", path.display(), e);
            None
        }
    }
}

/// Remove the DB file and its WAL/SHM companions.
fn delete_db_files(db_path: &Path) {
    let _ = std::fs::remove_file(db_path);
    let base = db_path.to_string_lossy();
    for suffix in ["-wal", "-shm"] {
        let companion = PathBuf::from(format!("{}{}", base, suffix));
        let _ = std::fs::remove_file(&companion);
    }
}

pub fn run_clean(
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    indexing_cfg: &IndexingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    if !db_path.exists() {
        eprintln!("Error: database not found at {}", db_path.display());
        eprintln!("Run `shiotsuchi chart` to create the index first.");
        std::process::exit(1);
    }

    // 1. Backup
    let backed_up = backup_file(db_path);
    let base = db_path.to_string_lossy();
    for suffix in ["-wal", "-shm"] {
        let companion = PathBuf::from(format!("{}{}", base, suffix));
        backup_file(&companion);
    }

    // 2. Delete originals
    delete_db_files(db_path);

    // 3. Re-index from scratch
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::util::secure_parent_dir(db_path);

    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;
    let config = IndexConfig {
        vaults: vaults.to_vec(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
    };

    let embedder = resolve_model_path(None).and_then(|p| match Embedder::load(&p) {
        Ok(e) => {
            eprintln!("[info] Embedder model loaded — vector indexing enabled.");
            Some(e)
        }
        Err(e) => {
            eprintln!("[warn] Could not load embedder: {}.", e);
            None
        }
    });

    let (results, invalid_patterns) =
        index_directory(&db, &tokenizer, &config, embedder.as_ref(), None)?;

    let mut indexed = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;
    for (_, _, result) in &results {
        match result {
            IndexResult::Inserted | IndexResult::Updated => indexed += 1,
            IndexResult::Skipped => skipped += 1,
            IndexResult::Error(_) => errors += 1,
        }
    }

    if let Some(ref backup_path) = backed_up {
        println!("Backup saved to: {}", backup_path.display());
    }
    println!(
        "Re-indexed {} files ({} skipped, {} errors)",
        indexed, skipped, errors
    );
    if invalid_patterns > 0 {
        println!("  {} invalid pattern(s) in exclude_dirs", invalid_patterns);
    }

    if embedder.is_none() {
        eprintln!(
            "[info] Embedder model not found — vector indexing skipped. \
             Run `shiotsuchi setup` to enable semantic search."
        );
    }

    Ok(())
}
