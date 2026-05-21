use crate::paths::default_db_path as core_default_db_path;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DatabaseConfig {
    pub db_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VaultEntry {
    pub notes_dir: Option<PathBuf>,
    #[serde(default)]
    pub db_path: Option<PathBuf>,
}

fn xdg_config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
                .join(".config")
        })
}

pub fn default_config_path() -> PathBuf {
    xdg_config_home().join("shiotsuchi").join("config.toml")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct IndexingConfig {
    pub include_extensions: Vec<String>,
    pub exclude_dirs: Vec<String>,
    pub auto_exclude_hidden: bool,
    pub follow_links: bool,
    pub dynamic_threshold: usize,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            exclude_dirs: vec!["node_modules".to_string()],
            auto_exclude_hidden: true,
            follow_links: false,
            dynamic_threshold: 5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WatcherConfig {
    pub enabled: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ShiotsuchiConfig {
    pub database: DatabaseConfig,
    pub vaults: HashMap<String, VaultEntry>,
    pub vault: Option<VaultEntry>,
    pub indexing: IndexingConfig,
    pub watcher: WatcherConfig,
}

impl ShiotsuchiConfig {
    pub fn resolved_vaults(&self) -> Vec<(String, PathBuf)> {
        let mut vaults: Vec<(String, PathBuf)> = Vec::new();

        if let Some(ref v) = self.vault {
            if let Some(ref dir) = v.notes_dir {
                vaults.push(("default".to_string(), dir.clone()));
            }
        }

        for (name, entry) in &self.vaults {
            if let Some(ref dir) = entry.notes_dir {
                vaults.push((name.clone(), dir.clone()));
            }
        }

        if vaults.is_empty() {
            vaults.push(("default".to_string(), PathBuf::from(".")));
            eprintln!("[warn] No vaults configured. Using current directory as 'default' vault.");
        }

        vaults
    }

    pub fn resolved_db_path(&self) -> PathBuf {
        self.database
            .db_path
            .clone()
            .or_else(|| self.vault.as_ref().and_then(|v| v.db_path.clone()))
            .unwrap_or_else(core_default_db_path)
    }

    pub fn load_from(path: &Path) -> Result<Self, config::ConfigError> {
        config::Config::builder()
            .add_source(config::File::from(path))
            .build()?
            .try_deserialize()
    }

    pub fn load() -> Self {
        let default_path = xdg_config_home().join("shiotsuchi").join("config.toml");
        if default_path.exists() {
            let cfg = Self::load_from(&default_path).unwrap_or_else(|e| {
                eprintln!(
                    "Warning: failed to load config from {}: {}. Using defaults.",
                    default_path.display(),
                    e
                );
                Self::default()
            });
            if cfg.vault.is_some() {
                eprintln!(
                    "[hint] Your config uses the old [vault] format. Run 'shiotsuchi config-migrate' to upgrade."
                );
            }
            cfg
        } else {
            Self::default()
        }
    }
}
