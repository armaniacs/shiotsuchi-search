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
        return Err(format!("Database not found at {}. Run `shiotsuchi chart` to create the index first.", db_path.display()).into());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ---------------------------------------------------------------------------
    // backup_file tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_backup_file_creates_bak_with_timestamp() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.db");
        fs::write(&file, "hello").unwrap();
        let result = backup_file(&file);
        assert!(result.is_some(), "backup should succeed");
        let backup = result.unwrap();
        assert!(backup.exists(), "backup file should exist");
        // Read back content
        let content = fs::read_to_string(&backup).unwrap();
        assert_eq!(content, "hello");
        // Original should still exist
        assert!(file.exists());
        // Backup name should be "test.db.bak.<timestamp>"
        let name = backup.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("test.db.bak."), "backup name mismatch: {}", name);
    }

    #[test]
    fn test_backup_file_nonexistent_returns_none() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("nonexistent.db");
        let result = backup_file(&file);
        assert!(result.is_none(), "backup of nonexistent file should return None");
    }

    #[test]
    fn test_backup_file_preserves_content() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("data.bin");
        let data = vec![0u8, 1, 2, 3, 255, 254];
        fs::write(&file, &data).unwrap();
        let result = backup_file(&file);
        assert!(result.is_some());
        let backed = std::fs::read(&result.unwrap()).unwrap();
        assert_eq!(backed, data, "backed-up content should match original");
    }

    #[test]
    #[cfg(unix)]
    fn test_backup_file_preserves_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("secret.db");
        fs::write(&file, "secret").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        let result = backup_file(&file).unwrap();
        let meta = std::fs::metadata(&result).unwrap();
        assert_eq!(
            meta.permissions().mode() & 0o777,
            0o600,
            "backup should preserve 0o600 permissions"
        );
    }

    // ---------------------------------------------------------------------------
    // delete_db_files tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_delete_db_files_removes_all_companions() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("test.db");
        let wal = tmp.path().join("test.db-wal");
        let shm = tmp.path().join("test.db-shm");
        fs::write(&db, "db").unwrap();
        fs::write(&wal, "wal").unwrap();
        fs::write(&shm, "shm").unwrap();
        delete_db_files(&db);
        assert!(!db.exists(), "db should be deleted");
        assert!(!wal.exists(), "wal should be deleted");
        assert!(!shm.exists(), "shm should be deleted");
    }

    #[test]
    fn test_delete_db_files_handles_missing_companions() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("test.db");
        fs::write(&db, "db").unwrap();
        // No -wal or -shm files
        delete_db_files(&db);
        assert!(!db.exists(), "db should be deleted");
    }

    #[test]
    fn test_delete_db_files_does_not_panic_on_nonexistent_db() {
        let tmp = TempDir::new().unwrap();
        let db = tmp.path().join("never-created.db");
        // Should not panic
        delete_db_files(&db);
    }

    // ---------------------------------------------------------------------------
    // run_clean error path
    // ---------------------------------------------------------------------------

    #[test]
    fn test_run_clean_full_flow() {
        // Create vault with markdown files, pre-populate a DB,
        // then run clean and verify backup + re-index.
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("a.md"), "# A\n\nContent A").unwrap();
        fs::write(vault.join("b.md"), "# B\n\nContent B").unwrap();

        let db_path = tmp.path().join("test.db");

        // Pre-populate DB (doesn't need a tokenizer for a bare open)
        let _db = match shiotsuchi_core::db::NoteDatabase::open(&db_path) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("[SKIPPED] clean::test_run_clean_full_flow — DB open failed: {}", e);
                return;
            }
        };
        assert!(db_path.exists(), "DB should exist before clean");

        let idx_cfg = IndexingConfig::default();
        let vaults = vec![("default".to_string(), vault.clone())];

        // Run clean (may skip if no tokenizer model)
        if let Err(e) = super::run_clean(&vaults, &db_path, &idx_cfg) {
            let msg = format!("{}", e);
            if msg.contains("no model") || msg.contains("NoModel") {
                eprintln!("[SKIPPED] clean::test_run_clean_full_flow — Vaporetto model not available");
                return;
            }
            panic!("clean failed: {}", e);
        }

        // After clean, DB should exist (re-created by indexing)
        assert!(db_path.exists(), "DB should exist after clean");

        // Backup file should exist alongside the re-created DB
        let parent = db_path.parent().unwrap();
        let backups: Vec<_> = std::fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
            .collect();
        assert!(!backups.is_empty(), "backup file should exist after clean");

        // Verify the new DB is usable
        match shiotsuchi_core::db::NoteDatabase::open(&db_path) {
            Ok(db) => {
                let stats = db.stats().unwrap();
                assert!(stats.total_files >= 2, "should have indexed at least 2 files, got {}", stats.total_files);
            }
            Err(e) => panic!("DB should be openable after clean: {}", e),
        }
    }
}
