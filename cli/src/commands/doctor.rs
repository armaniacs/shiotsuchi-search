use crate::config::{IndexingConfig, ShiotsuchiConfig};
use crate::messages;
use crate::msg_fmt;
use clap::Args;
use dialoguer::{theme::ColorfulTheme, Confirm};
use shiotsuchi_core::{
    db::NoteDatabase,
    embedder::{resolve_model_path, Embedder},
    indexer::{index_directory, IndexResult},
    models::IndexConfig,
    tokenizer::get_tokenizer,
};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
#[command(about = crate::messages::DOCTOR_ABOUT)]
pub struct DoctorArgs {}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns true when stdin and stdout are both connected to a terminal.
fn is_tty() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Ask a yes/no question using dialoguer. Returns `true` on yes.
/// Returns an error in non-TTY environments (caller should check `is_tty()`
/// first).
fn ask(prompt: &str) -> Result<bool, Box<dyn std::error::Error>> {
    Ok(Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .default(false)
        .interact()?)
}

/// Set file permissions to `0o600` (owner read/write only) on Unix.
/// No-op on other platforms.
#[cfg(unix)]
fn set_restrictive_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn set_restrictive_permissions(_path: &Path) {}

/// Return all keys inside `[indexing]` that are not recognised by the
/// current schema.  An empty vec means either the section is absent,
/// the file is unreadable, or no unknown fields exist.
fn find_unknown_indexing_fields(config_path: &Path) -> Vec<String> {
    let content = match std::fs::read_to_string(config_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let table: toml::Table = match content.parse() {
        Ok(t) => t,
        Err(_) => return vec![],
    };
    // Keep this list in sync with `core::config::IndexingConfig` fields.
    // If new fields are added to IndexingConfig, add them here too.
    let known: [&str; 5] = [
        "include_extensions",
        "exclude_dirs",
        "auto_exclude_hidden",
        "follow_links",
        "dynamic_threshold",
    ];
    match table.get("indexing").and_then(|v| v.as_table()) {
        Some(indexing) => indexing
            .keys()
            .filter(|k| !known.contains(&k.as_str()))
            .cloned()
            .collect(),
        None => vec![],
    }
}

/// Remove the given keys from the `[indexing]` section of the config file.
/// A timestamped backup is created first.
fn fix_config_unknown_fields(
    config_path: &Path,
    unknown_fields: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let backup_path = backup_config_file(config_path)?;

    let content = std::fs::read_to_string(config_path)?;
    let mut table: toml::Table = content.parse()?;
    if let Some(indexing) = table.get_mut("indexing").and_then(|v| v.as_table_mut()) {
        for field in unknown_fields {
            indexing.remove(field.as_str());
        }
    }
    let output = toml::to_string_pretty(&table)?;
    std::fs::write(config_path, output)?;
    set_restrictive_permissions(config_path);

    println!("{}", msg_fmt!(messages::DOCTOR_BACKUP_SAVED, backup_path.display()));
    Ok(())
}

/// Migrate an old-format config (`[vault]` section) to the new multi-vault
/// format.  Creates a backup before writing.
fn fix_config_old_vault_format(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let old_cfg = ShiotsuchiConfig::load_from(config_path)?;
    let legacy_vault = match old_cfg.vault.as_ref() {
        Some(v) => v,
        None => return Ok(()), // not legacy – nothing to do
    };

    let new_db_path = old_cfg
        .database
        .db_path
        .clone()
        .or_else(|| legacy_vault.db_path.clone());

    let mut new_vaults = std::collections::HashMap::new();
    if let Some(ref nd) = legacy_vault.notes_dir {
        new_vaults.insert(
            "default".to_string(),
            crate::config::VaultEntry {
                notes_dir: Some(nd.clone()),
                db_path: None,
            },
        );
    }

    let new_cfg = ShiotsuchiConfig {
        database: crate::config::DatabaseConfig {
            db_path: new_db_path,
        },
        vaults: new_vaults,
        vault: None,
        indexing: old_cfg.indexing,
        watcher: old_cfg.watcher,
        synonyms: HashMap::new(),
        hybrid_alpha: old_cfg.hybrid_alpha,
    };

    let backup_path = backup_config_file(config_path)?;
    let toml_str = toml::to_string_pretty(&new_cfg)?;
    std::fs::write(config_path, toml_str)?;
    set_restrictive_permissions(config_path);

    println!("{}", msg_fmt!(messages::DOCTOR_BACKUP_SAVED, backup_path.display()));
    Ok(())
}

/// Create a timestamped backup of a config file.
/// Returns the path of the backup that was created.
fn backup_config_file(config_path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let timestamp = format!("{}.{:06}", now.as_secs(), now.subsec_micros());
    let mut backup_path = config_path.with_extension(format!("toml.bak.{}", timestamp));
    let mut counter = 1u32;
    while backup_path.exists() {
        backup_path = config_path.with_extension(format!("toml.bak.{}.{}", timestamp, counter));
        counter += 1;
    }
    std::fs::copy(config_path, &backup_path)?;
    set_restrictive_permissions(&backup_path);
    Ok(backup_path)
}

/// Index the vault into a (new or existing) database.
///
/// Creates parent directories, opens the database, loads the tokenizer and
/// optional embedder, and runs `index_directory`.
fn index_vault(
    db_path: &Path,
    vaults: &[(String, PathBuf)],
    indexing_cfg: &IndexingConfig,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    let db = NoteDatabase::open(db_path)?;
    let tokenizer = get_tokenizer()?;
    let config = IndexConfig {
        vaults: vaults.to_vec(),
        include_extensions: indexing_cfg.include_extensions.clone(),
        exclude_dirs: indexing_cfg.exclude_dirs.clone(),
        auto_exclude_hidden: indexing_cfg.auto_exclude_hidden,
        follow_links: indexing_cfg.follow_links,
        dynamic_threshold: indexing_cfg.dynamic_threshold,
        user_dictionary: indexing_cfg.user_dictionary.clone(),
    };
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
    println!("{}", msg_fmt!(messages::INDEX_SUMMARY, indexed, skipped, errors));
    if invalid_patterns > 0 {
        println!("{}", msg_fmt!(messages::INDEX_PATTERN_WARN, invalid_patterns));
    }

    if embedder.is_none() {
        eprintln!("{}", messages::INFO_EMBEDDER_SKIPPED);
    }

    let stats = db.stats()?;
    Ok((stats.total_files, stats.total_chunks))
}

/// Back up the old database files and re-index from scratch.
fn rebuild_db(
    db_path: &Path,
    vaults: &[(String, PathBuf)],
    indexing_cfg: &IndexingConfig,
) -> Result<(usize, usize), Box<dyn std::error::Error>> {
    // Backup old DB (best-effort)
    let backed_up = super::clean::backup_file(db_path);
    let base = db_path.to_string_lossy();
    for suffix in ["-wal", "-shm"] {
        let _ = super::clean::backup_file(&PathBuf::from(format!("{}{}", base, suffix)));
    }
    // Delete old DB files
    super::clean::delete_db_files(db_path);
    // Index fresh
    let result = index_vault(db_path, vaults, indexing_cfg);
    if let Some(ref backup_path) = backed_up {
        println!("{}", msg_fmt!(messages::DOCTOR_BACKUP_SAVED, backup_path.display()));
    }
    result
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

pub fn run_doctor(
    _cfg: &ShiotsuchiConfig,
    db_path: &Path,
    vaults: &[(String, PathBuf)],
    indexing_cfg: &IndexingConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut all_ok = true;
    let tty = is_tty();

    // -----------------------------------------------------------------------
    // 1. Config file
    // -----------------------------------------------------------------------
    let config_path = crate::config::default_config_path();
    if config_path.exists() {
        // Try to actually parse the config
        match ShiotsuchiConfig::load_from(&config_path) {
            Ok(_) => {
                println!("{}", msg_fmt!(messages::DOCTOR_CONFIG_OK, config_path.display()));
            }
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("unknown field") {
                    println!("{}", msg_fmt!(messages::DOCTOR_CONFIG_ERROR, config_path.display(), e));
                    all_ok = false;

                    let unknown = find_unknown_indexing_fields(&config_path);
                    if tty && !unknown.is_empty() {
                        let field_list = unknown.join(", ");
                        if ask(&msg_fmt!(messages::DOCTOR_CONFIG_FIX_PROMPT, field_list))? {
                            match fix_config_unknown_fields(&config_path, &unknown) {
                                Ok(()) => println!("{}", messages::DOCTOR_CONFIG_FIXED),
                                Err(fix_err) => {
                                    eprintln!("{}", msg_fmt!(messages::DOCTOR_CONFIG_FIX_FAILED, fix_err))
                                }
                            }
                        }
                    }
                } else {
                    println!("{}", msg_fmt!(messages::DOCTOR_CONFIG_ERROR, config_path.display(), e));
                    all_ok = false;
                }
            }
        }

        // Check for old [vault] format (re-load the config after any fix)
        if let Ok(reloaded) = ShiotsuchiConfig::load_from(&config_path) {
            if reloaded.vault.is_some() {
                println!("{}", msg_fmt!(messages::DOCTOR_CONFIG_OLD_FORMAT, config_path.display()));
                if tty
                    && ask(messages::DOCTOR_CONFIG_MIGRATE_PROMPT)?
                {
                    match fix_config_old_vault_format(&config_path) {
                        Ok(()) => println!("{}", messages::DOCTOR_CONFIG_MIGRATED),
                        Err(err) => eprintln!("{}", msg_fmt!(messages::DOCTOR_CONFIG_MIGRATE_FAILED, err)),
                    }
                }
            }
        }
    } else {
        println!("{}", msg_fmt!(messages::DOCTOR_CONFIG_NOT_FOUND, config_path.display()));
    }

    // -----------------------------------------------------------------------
    // 2. Database
    // -----------------------------------------------------------------------
    if db_path.exists() {
        match NoteDatabase::open(db_path) {
            Ok(db) => match db.stats() {
                Ok(stats) => println!("{}", msg_fmt!(messages::DOCTOR_DB_OK, db_path.display(), stats.total_files, stats.total_chunks)),
                Err(e) => {
                    println!("{}", msg_fmt!(messages::DOCTOR_DB_STATS_FAILED, db_path.display(), e));
                    all_ok = false;
                    drop(db);
                    if tty && ask(messages::DOCTOR_DB_REBUILD_PROMPT)? {
                        match rebuild_db(db_path, vaults, indexing_cfg) {
                            Ok((files, chunks)) => println!("{}", msg_fmt!(messages::DOCTOR_DB_REBUILT, files, chunks)),
                            Err(fix_err) => {
                                eprintln!("{}", msg_fmt!(messages::DOCTOR_DB_REBUILD_FAILED, fix_err))
                            }
                        }
                    }
                }
            },
            Err(e) => {
                println!("{}", msg_fmt!(messages::DOCTOR_DB_OPEN_FAILED, db_path.display(), e));
                all_ok = false;
                if tty && ask(messages::DOCTOR_DB_REBUILD_PROMPT)? {
                    match rebuild_db(db_path, vaults, indexing_cfg) {
                        Ok((files, chunks)) => println!("{}", msg_fmt!(messages::DOCTOR_DB_REBUILT, files, chunks)),
                        Err(fix_err) => {
                            eprintln!("{}", msg_fmt!(messages::DOCTOR_DB_REBUILD_FAILED, fix_err))
                        }
                    }
                }
            }
        }
    } else {
        println!("{}", msg_fmt!(messages::DOCTOR_DB_NOT_FOUND, db_path.display()));
        if tty && ask(messages::DOCTOR_DB_CREATE_PROMPT)? {
            match index_vault(db_path, vaults, indexing_cfg) {
                Ok((files, chunks)) => println!("{}", msg_fmt!(messages::DOCTOR_DB_CREATED, files, chunks)),
                Err(fix_err) => eprintln!("{}", msg_fmt!(messages::DOCTOR_DB_INDEX_FAILED, fix_err)),
            }
        }
    }

    // -----------------------------------------------------------------------
    // 3. Vaporetto tokenizer
    // -----------------------------------------------------------------------
    match get_tokenizer() {
        Ok(_) => println!("{}", messages::DOCTOR_TOKENIZER_OK),
        Err(e) => println!("{}", msg_fmt!(messages::DOCTOR_TOKENIZER_FALLBACK, e)),
    }

    // -----------------------------------------------------------------------
    // 4. Embedder model
    // -----------------------------------------------------------------------
    match resolve_model_path(None) {
        Some(p) => match Embedder::load(&p) {
            Ok(_) => println!("{}", messages::DOCTOR_EMBEDDER_OK),
            Err(e) => println!("{}", msg_fmt!(messages::DOCTOR_EMBEDDER_LOAD_FAILED, e)),
        },
        None => {
            println!("{}", messages::DOCTOR_EMBEDDER_NOT_FOUND);
            println!("{}", messages::DOCTOR_EMBEDDER_HINT);
        }
    }

    // -----------------------------------------------------------------------
    // 5. Vault directories
    // -----------------------------------------------------------------------
    if vaults.is_empty() {
        println!("{}", messages::DOCTOR_VAULT_NONE);
    } else {
        for (name, dir) in vaults {
            if dir.exists() {
                println!("{}", msg_fmt!(messages::DOCTOR_VAULT_OK, name, dir.display()));
            } else {
                println!("{}", msg_fmt!(messages::DOCTOR_VAULT_ERROR, name, dir.display()));
                println!("{}", messages::DOCTOR_VAULT_NOT_EXIST);
                all_ok = false;
            }
        }
    }

    // -----------------------------------------------------------------------
    // Summary
    // -----------------------------------------------------------------------
    if all_ok {
        println!("{}", messages::DOCTOR_ALL_PASSED);
    } else {
        println!("{}", messages::DOCTOR_SOME_FAILED);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ------------------------------------------------------------------
    // Helper: known fields in [indexing]
    // ------------------------------------------------------------------

    #[test]
    fn test_find_unknown_fields_empty_when_none() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            r#"[indexing]
include_extensions = ["md"]
exclude_dirs = ["node_modules"]
auto_exclude_hidden = true
follow_links = false
dynamic_threshold = 5
"#,
        )
        .unwrap();
        let unknown = find_unknown_indexing_fields(&path);
        assert!(unknown.is_empty(), "expected no unknown fields, got {:?}", unknown);
    }

    #[test]
    fn test_find_unknown_fields_detects_extra_keys() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            r#"[indexing]
include_extensions = ["md"]
snippet_lines = 3
dynamic_threshold = 5
"#,
        )
        .unwrap();
        let unknown = find_unknown_indexing_fields(&path);
        assert_eq!(unknown, vec!["snippet_lines"]);
    }

    #[test]
    fn test_find_unknown_fields_multiple_unknown() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            r#"[indexing]
snippet_lines = 3
verbose = true
dynamic_threshold = 5
"#,
        )
        .unwrap();
        let mut unknown = find_unknown_indexing_fields(&path);
        unknown.sort();
        assert_eq!(unknown, vec!["snippet_lines", "verbose"]);
    }

    #[test]
    fn test_find_unknown_fields_missing_section_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(&path, "[database]\ndb_path = \"/tmp/db\"\n").unwrap();
        let unknown = find_unknown_indexing_fields(&path);
        assert!(unknown.is_empty());
    }

    #[test]
    fn test_find_unknown_fields_unreadable_file_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("nonexistent.toml");
        let unknown = find_unknown_indexing_fields(&path);
        assert!(unknown.is_empty());
    }

    // ------------------------------------------------------------------
    // Helper: fix_config_unknown_fields
    // ------------------------------------------------------------------

    #[test]
    fn test_fix_unknown_fields_removes_them() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            r#"[indexing]
include_extensions = ["md"]
snippet_lines = 3
dynamic_threshold = 5
"#,
        )
        .unwrap();

        fix_config_unknown_fields(&path, &["snippet_lines".to_string()]).unwrap();

        // Unknown field should be gone; file should parse cleanly
        let reloaded = ShiotsuchiConfig::load_from(&path).unwrap();
        assert_eq!(reloaded.indexing.include_extensions, vec!["md"]);
        assert_eq!(reloaded.indexing.dynamic_threshold, 5);
    }

    #[test]
    fn test_fix_unknown_fields_creates_backup() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        let original = r#"[indexing]
include_extensions = ["md"]
snippet_lines = 3
"#;
        fs::write(&path, original).unwrap();

        fix_config_unknown_fields(&path, &["snippet_lines".to_string()]).unwrap();

        // Backup file should exist
        let parent = tmp.path();
        let backups: Vec<_> = fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("config.toml.bak.")
            })
            .collect();
        assert!(!backups.is_empty(), "backup should exist");

        // Backup content should be the original
        let backup_content = fs::read_to_string(backups[0].path()).unwrap();
        assert_eq!(backup_content, original);
    }

    // ------------------------------------------------------------------
    // Helper: fix_config_old_vault_format
    // ------------------------------------------------------------------

    #[test]
    fn test_fix_old_vault_format_migrates() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            r#"
[vault]
notes_dir = "/tmp/notes"

[indexing]
exclude_dirs = ["node_modules"]
"#,
        )
        .unwrap();

        fix_config_old_vault_format(&path).unwrap();

        // After migration, vault should be None, vaults should contain "default"
        let reloaded = ShiotsuchiConfig::load_from(&path).unwrap();
        assert!(reloaded.vault.is_none(), "old vault should be removed");
        assert!(
            reloaded.vaults.contains_key("default"),
            "default vault should exist"
        );
        assert_eq!(
            reloaded.vaults["default"]
                .notes_dir
                .as_ref()
                .unwrap()
                .to_string_lossy(),
            "/tmp/notes"
        );
    }

    #[test]
    fn test_fix_old_vault_format_noop_when_already_new() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(
            &path,
            r#"
[vaults.work]
notes_dir = "/work/notes"
"#,
        )
        .unwrap();

        // Should not error, should not create unnecessary backup
        fix_config_old_vault_format(&path).unwrap();

        let parent = tmp.path();
        let backups: Vec<_> = fs::read_dir(parent)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("config.toml.bak.")
            })
            .collect();
        assert!(backups.is_empty(), "no backup should be created for new format");
    }

    // ------------------------------------------------------------------
    // Helper: backup_config_file
    // ------------------------------------------------------------------

    #[test]
    fn test_backup_config_file_creates_copy() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(&path, "hello world").unwrap();

        let backup = backup_config_file(&path).unwrap();
        assert!(backup.exists());
        assert_eq!(fs::read_to_string(&backup).unwrap(), "hello world");
    }

    #[test]
    fn test_backup_config_file_avoid_collision() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(&path, "original").unwrap();

        // Create a fake backup with a matching name pattern
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let ts = format!("{}.{:06}", now.as_secs(), now.subsec_micros());
        let fake = path.with_extension(format!("toml.bak.{}", ts));
        fs::write(&fake, "fake").unwrap();

        let backup = backup_config_file(&path).unwrap();
        assert_ne!(backup, fake, "should pick a different name on collision");
        assert!(backup.exists());
        assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
    }

    // ------------------------------------------------------------------
    // Helper: index_vault (integration)
    // ------------------------------------------------------------------

    #[test]
    fn test_index_vault_creates_db_with_files() {
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("a.md"), "# A\n\nHello").unwrap();
        fs::write(vault.join("b.md"), "# B\n\nWorld").unwrap();

        let db_path = tmp.path().join("cache").join("test.db");
        let vaults = vec![("default".to_string(), vault)];
        let idx_cfg = IndexingConfig::default();

        match index_vault(&db_path, &vaults, &idx_cfg) {
            Ok((files, chunks)) => {
                assert!(files >= 2, "should index at least 2 files, got {}", files);
                assert!(chunks >= 2, "should have at least 2 chunks, got {}", chunks);
            }
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("no model") || msg.contains("NoModel") {
                    eprintln!("[SKIPPED] test_index_vault_creates_db_with_files — Vaporetto model not available");
                    return;
                }
                panic!("index_vault failed: {}", e);
            }
        }
    }

    // ------------------------------------------------------------------
    // Helper: rebuild_db (integration)
    // ------------------------------------------------------------------

    #[test]
    fn test_rebuild_db_reindexes_after_damage() {
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        fs::write(vault.join("note.md"), "# Note\n\nContent").unwrap();

        let db_path = tmp.path().join("test.db");
        let vaults = vec![("default".to_string(), vault)];
        let idx_cfg = IndexingConfig::default();

        // First index
        match index_vault(&db_path, &vaults, &idx_cfg) {
            Ok(_) => {}
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("no model") || msg.contains("NoModel") {
                    eprintln!("[SKIPPED] — Vaporetto model not available");
                    return;
                }
                panic!("first index failed: {}", e);
            }
        }

        // "Damage" the DB by writing garbage
        fs::write(&db_path, "garbage data").unwrap();

        // Rebuild
        match rebuild_db(&db_path, &vaults, &idx_cfg) {
            Ok((files, chunks)) => {
                assert!(
                    files >= 1,
                    "should re-index at least 1 file, got {}",
                    files
                );
                assert!(chunks >= 1, "should have at least 1 chunk, got {}", chunks);
            }
            Err(e) => {
                let msg = format!("{}", e);
                if msg.contains("no model") || msg.contains("NoModel") {
                    eprintln!("[SKIPPED] — Vaporetto model not available");
                    return;
                }
                panic!("rebuild failed: {}", e);
            }
        }
    }
}
