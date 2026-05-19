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
