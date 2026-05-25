use crate::config::{self, DatabaseConfig, ShiotsuchiConfig, VaultEntry};
use clap::Args;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Args, Debug)]
pub struct ConfigMigrateArgs {
    #[arg(long)]
    pub config: Option<PathBuf>,
}

pub fn run_config_migrate(args: &ConfigMigrateArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config_path = args.config.clone().unwrap_or_else(config::default_config_path);

    if !config_path.exists() {
        eprintln!("Config file not found: {}", config_path.display());
        return Ok(());
    }

    let old_cfg = ShiotsuchiConfig::load_from(&config_path)?;

    if old_cfg.vault.is_none() {
        eprintln!("Config is already in new format — no migration needed.");
        return Ok(());
    }

    let legacy_vault = old_cfg.vault.as_ref().unwrap();
    let new_db_path = old_cfg
        .database
        .db_path
        .clone()
        .or_else(|| legacy_vault.db_path.clone());

    let mut new_vaults: HashMap<String, VaultEntry> = HashMap::new();
    if let Some(ref nd) = legacy_vault.notes_dir {
        new_vaults.insert(
            "default".to_string(),
            VaultEntry {
                notes_dir: Some(nd.clone()),
                db_path: None,
            },
        );
    }

    let new_cfg = ShiotsuchiConfig {
        database: DatabaseConfig {
            db_path: new_db_path,
        },
        vaults: new_vaults,
        vault: None,
        indexing: old_cfg.indexing,
        watcher: old_cfg.watcher,
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let backup_path = config_path.with_extension(format!("toml.bak.{}", timestamp));
    fs::copy(&config_path, &backup_path)?;

    let toml_str = toml::to_string_pretty(&new_cfg)?;
    fs::write(&config_path, toml_str)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&config_path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    {
        log::warn!("Config file permissions not restricted — not supported on this platform.");
    }

    eprintln!("Config migrated successfully.");
    eprintln!("Backup saved to: {}", backup_path.display());
    eprintln!("New format written to: {}", config_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn test_config_path(temp: &TempDir) -> PathBuf {
        temp.path().join("config.toml")
    }

    #[test]
    fn test_nonexistent_config_is_noop() {
        let temp = TempDir::new().unwrap();
        let path = test_config_path(&temp);
        let args = ConfigMigrateArgs {
            config: Some(path.clone()),
        };
        // Should not error — just prints "not found" and returns
        run_config_migrate(&args).unwrap();
        assert!(!path.exists(), "no config file should be created");
    }

    #[test]
    fn test_already_new_format_is_noop() {
        let temp = TempDir::new().unwrap();
        let path = test_config_path(&temp);
        fs::write(
            &path,
            r#"
[vaults.work]
notes_dir = "/work/notes"

[database]
db_path = "/tmp/db.sqlite"
"#,
        )
        .unwrap();

        let args = ConfigMigrateArgs {
            config: Some(path.clone()),
        };
        run_config_migrate(&args).unwrap();

        // Content should be unchanged
        let contents = fs::read_to_string(&path).unwrap();
        assert!(
            contents.contains("vaults.work"),
            "new format should be preserved: {}",
            contents
        );

        // No backup should have been created
        let backups: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".bak."))
            .collect();
        assert!(
            backups.is_empty(),
            "no backup expected for already-new format"
        );
    }

    #[test]
    fn test_migrates_old_vault_format() {
        let temp = TempDir::new().unwrap();
        let path = test_config_path(&temp);
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

        let args = ConfigMigrateArgs {
            config: Some(path.clone()),
        };
        run_config_migrate(&args).unwrap();

        // File should now have new format
        let contents = fs::read_to_string(&path).unwrap();
        assert!(
            !contents.contains("[vault]"),
            "old [vault] section should be removed"
        );
        assert!(
            contents.contains("[vaults.default]"),
            "new vaults.default should exist"
        );
        assert!(
            contents.contains("exclude_dirs"),
            "indexing section should be preserved"
        );

        // Backup should exist
        let backups: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("config.toml.bak."))
            .collect();
        assert!(
            !backups.is_empty(),
            "backup file should exist after migration"
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_migrated_config_permissions_0600() {
        use std::os::unix::fs::PermissionsExt;
        let temp = TempDir::new().unwrap();
        let path = test_config_path(&temp);
        fs::write(
            &path,
            r#"
[vault]
notes_dir = "/tmp/notes"
"#,
        )
        .unwrap();

        let args = ConfigMigrateArgs {
            config: Some(path.clone()),
        };
        run_config_migrate(&args).unwrap();

        let meta = fs::metadata(&path).unwrap();
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "migrated config should have 0o600 permissions, got {:o}",
            mode
        );
    }
}
