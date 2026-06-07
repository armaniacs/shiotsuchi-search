use crate::messages;
use crate::msg_fmt;
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
#[command(about = crate::messages::CLEAN_ABOUT)]
pub struct CleanArgs {
    /// Purge ALL user data (chunks, FTS, vectors, cache) and rebuild index.
    /// Prompts for confirmation before proceeding. Does NOT delete config.toml.
    #[arg(long)]
    pub purge_all: bool,
}

/// Copy a single file to `<path>.bak.<timestamp>`.
/// Returns the backup path if the original existed, None otherwise.
pub(crate) fn backup_file(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    // Prune old backups keeping only the 3 most recent (by filename: *.bak.TIMESTAMP).
    if let Some(parent) = path.parent() {
        let base_name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
        let prefix = format!("{}.bak.", base_name);
        let mut backups: Vec<_> = match std::fs::read_dir(parent) {
            Ok(entries) => entries
                .filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().starts_with(&prefix))
                .collect(),
            Err(_) => Vec::new(),
        };
        backups.sort_by_key(|e| e.file_name());
        for old in backups.iter().rev().skip(3) {
            let _ = std::fs::remove_file(old.path());
        }
    }

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
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
            eprintln!("{}", msg_fmt!(messages::CLEAN_BACKUP_FAILED, path.display(), e));
            None
        }
    }
}

/// Remove the DB file and its WAL/SHM companions.
/// Refuses to follow symlinks to prevent potential file escape attacks.
pub(crate) fn delete_db_files(db_path: &Path) {
    let base = db_path.to_string_lossy();
    let names = [
        base.as_ref().to_string(),
        format!("{}-wal", base),
        format!("{}-shm", base),
    ];
    for name in &names {
        let path = Path::new(name);
        if path.exists() {
            if path.is_symlink() {
                tracing::warn!("Refusing to remove symlink: {}", path.display());
                continue;
            }
            let _ = std::fs::remove_file(path);
        }
    }
}

pub fn run_clean(
    args: &CleanArgs,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
    indexing_cfg: &IndexingConfig,
    vlm_cfg: &shiotsuchi_core::config::VlmConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Handle --purge-all flag
    if args.purge_all {
        if !db_path.exists() {
            return Err(msg_fmt!(messages::CLEAN_DB_NOT_FOUND, db_path.display()).into());
        }

        let theme = crate::util::dialoguer_theme();
        let confirmed = dialoguer::Confirm::with_theme(&*theme)
            .with_prompt("WARNING: This will delete ALL indexed data for ALL vaults. Continue?")
            .default(false)
            .interact()?;

        if !confirmed {
            println!("{}", messages::CLEAN_PURGE_ABORTED);
            return Ok(());
        }

        let db = NoteDatabase::open(db_path)?;
        db.purge_all_user_data()?;
        println!("{}", messages::CLEAN_PURGE_DONE);

        // Continue to rebuild index (fall through to normal clean logic)
    } else {
        if !db_path.exists() {
            return Err(msg_fmt!(messages::CLEAN_DB_NOT_FOUND, db_path.display()).into());
        }
    }

    // Build IndexConfig
    let config = IndexConfig::from_cli_configs(vaults.to_vec(), indexing_cfg, vlm_cfg);

    let tokenizer = get_tokenizer()?;

    let embedder = resolve_model_path(None).and_then(|p| match Embedder::load(&p) {
        Ok(e) => {
            eprintln!("{}", messages::INFO_EMBEDDER_LOADED);
            Some(e)
        }
        Err(e) => {
            eprintln!("{}", msg_fmt!(messages::WARN_EMBEDDER_LOAD, e));
            None
        }
    });

    // Build new DB at a temporary path (same directory, for atomic rename)
    let tmp_path = db_path.with_extension(format!(
        "sqlite3.tmp.{}",
        std::process::id()
    ));
    // Clean up any stale temp file from a previous crash
    let _ = std::fs::remove_file(&tmp_path);
    let tmp_base = tmp_path.to_string_lossy();
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(PathBuf::from(format!("{}{}", tmp_base, suffix)));
    }

    // Index into temp DB first (before touching original)
    {
        let db = NoteDatabase::open(&tmp_path)?;
        let (results, invalid_patterns, _excluded) =
            index_directory(&db, &tokenizer, &config, embedder.as_ref(), None)?;

        // Checkpoint WAL so all data is in the main .db file before rename
        db.wal_checkpoint()?;
        drop(db);

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
        println!("{}", msg_fmt!(messages::CLEAN_REINDEXED, indexed, skipped, errors));
        if invalid_patterns > 0 {
            println!("{}", msg_fmt!(messages::INDEX_PATTERN_WARN, invalid_patterns));
        }
    }

    // Backup the old DB (now that indexing succeeded)
    let backed_up = backup_file(db_path);
    let base = db_path.to_string_lossy();
    for suffix in ["-wal", "-shm"] {
        let companion = PathBuf::from(format!("{}{}", base, suffix));
        backup_file(&companion);
    }

    // Delete old DB files (free the file name for rename)
    delete_db_files(db_path);

    // Atomic rename: temp → real path.
    // On the same filesystem this is an atomic metadata swap.
    // Cross-device fallback: copy + delete.
    if let Err(e) = std::fs::rename(&tmp_path, db_path) {
        eprintln!("{}", msg_fmt!(messages::CLEAN_RENAME_FAILED, e));
        std::fs::copy(&tmp_path, db_path)?;
        std::fs::remove_file(&tmp_path)?;
    }

    // Clean up any temp companions
    for suffix in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(PathBuf::from(format!("{}{}", tmp_base, suffix)));
    }

    if let Some(ref backup_path) = backed_up {
        println!("{}", msg_fmt!(messages::CLEAN_BACKUP_SAVED, backup_path.display()));
    }

    if embedder.is_none() {
        eprintln!("{}", messages::INFO_EMBEDDER_SKIPPED);
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
        let content = fs::read_to_string(&backup).unwrap();
        assert_eq!(content, "hello");
        assert!(file.exists());
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
        let backed = std::fs::read(result.unwrap()).unwrap();
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
        delete_db_files(&db);
        assert!(!db.exists(), "db should be deleted");
    }

    #[test]
    fn test_delete_db_files_does_not_panic_on_nonexistent_db() {
        let tmp = TempDir::new().unwrap();
        delete_db_files(&tmp.path().join("never-created.db"));
    }

    // ---------------------------------------------------------------------------
    // run_clean error tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_run_clean_missing_db_returns_error() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("nonexistent.db");
        let vaults = vec![("default".to_string(), tmp.path().join("vault"))];
        let args = CleanArgs { purge_all: false };
        let result = super::run_clean(&args, &vaults, &db_path, &IndexingConfig::default(), &Default::default());
        assert!(result.is_err(), "clean without DB should return error");
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("見つかりません"), "error should mention '見つかりません', got: {}", msg);
    }

    // ---------------------------------------------------------------------------
    // run_clean integration test
    // ---------------------------------------------------------------------------

    /// Helper: find files in a directory whose name contains a substring.
    fn find_files(dir: &std::path::Path, pattern: &str) -> Vec<std::fs::DirEntry> {
        std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(pattern))
            .collect()
    }

    #[test]
    fn test_run_clean_full_flow() {
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("a.md"), "# A\n\nContent A").unwrap();
        fs::write(vault.join("b.md"), "# B\n\nContent B").unwrap();

        let db_path = tmp.path().join("test.db");
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
        let args = CleanArgs { purge_all: false };

        if let Err(e) = super::run_clean(&args, &vaults, &db_path, &idx_cfg, &Default::default()) {
            let msg = format!("{}", e);
            if msg.contains("no model") || msg.contains("NoModel") || msg.contains("No such file") {
                eprintln!("[SKIPPED] clean::test_run_clean_full_flow — Vaporetto model not available");
                return;
            }
            panic!("clean failed: {}", e);
        }

        assert!(db_path.exists(), "DB should exist after clean");

        let parent = db_path.parent().unwrap();
        assert!(!find_files(parent, ".bak.").is_empty(), "backup file should exist after clean");
        assert!(find_files(parent, ".tmp.").is_empty(), "stale temp files should not remain");

        match shiotsuchi_core::db::NoteDatabase::open(&db_path) {
            Ok(db) => {
                let stats = db.stats().unwrap();
                assert!(stats.total_files >= 2, "should have indexed at least 2 files, got {}", stats.total_files);
            }
            Err(e) => panic!("DB should be openable after clean: {}", e),
        }
    }
}
