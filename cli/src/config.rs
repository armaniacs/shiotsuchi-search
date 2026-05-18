use serde::{Deserialize, Serialize};
use shiotsuchi_core::paths::default_db_path as core_default_db_path;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DatabaseConfig {
    pub db_path: Option<PathBuf>,
}

/// A single vault entry, used by both old `[vault]` and new `[vaults.xxx]`.
/// Only `notes_dir` is consumed from `[vaults.xxx]` entries.
/// `db_path` is legacy-only (old `[vault]` section) and ignored in `[vaults.xxx]`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct VaultEntry {
    pub notes_dir: Option<PathBuf>,
    /// Legacy: old [vault] held db_path here; ignored in [vaults.xxx].
    /// Use [database].db_path instead.
    #[serde(default)]
    pub db_path: Option<PathBuf>,
}

pub(crate) fn xdg_config_home() -> PathBuf {
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
    pub snippet_lines: usize,
    /// Maximum characters allowed in a search snippet (128–65 535). Default: 1000.
    pub max_snippet_chars: usize,
    pub include_extensions: Vec<String>,
    /// Directory names to exclude from indexing (renamed from exclude_patterns).
    /// The old key will cause a deserialize error — use `exclude_dirs` instead.
    pub exclude_dirs: Vec<String>,
    pub auto_exclude_hidden: bool,
    pub follow_links: bool,
    /// Minimum number of matching files for a directory to be dynamically detected
    /// as a noise candidate. Defaults to 5.
    pub dynamic_threshold: usize,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            snippet_lines: 3,
            max_snippet_chars: 1000,
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            // .git/.obsidian は auto_exclude_hidden=true により自動除外される
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
    /// Resolve vault entries: merge legacy [vault] + new [vaults.xxx]
    pub fn resolved_vaults(&self) -> Vec<(String, PathBuf)> {
        let mut vaults: Vec<(String, PathBuf)> = Vec::new();

        if let Some(ref v) = self.vault {
            if let Some(ref dir) = v.notes_dir {
                vaults.push(("default".to_string(), dir.clone()));
            }
        }

        let mut names_seen = std::collections::HashSet::new();
        for (name, entry) in &self.vaults {
            if let Some(ref dir) = entry.notes_dir {
                vaults.push((name.clone(), dir.clone()));
                names_seen.insert(name.clone());
            }
        }

        if vaults.is_empty() {
            vaults.push(("default".to_string(), PathBuf::from(".")));
            eprintln!("[warn] No vaults configured. Using current directory as 'default' vault.");
        }

        vaults
    }

    /// Resolve db_path from [database] or legacy [vault]
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

    /// Load configuration from the XDG config directory.
    ///
    /// # Security
    /// The config file (`config.toml`) is created with `0o600` permissions on Unix when
    /// generated via `shiotsuchi init`. Avoid storing secrets (API tokens, passwords) in
    /// this file. If sensitive values are needed, prefer environment variables or OS-level
    /// secret management.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = ShiotsuchiConfig::default();
        assert!(config.vaults.is_empty());
        assert!(config.vault.is_none());
        assert!(config.database.db_path.is_none());
        assert_eq!(config.indexing.include_extensions, vec!["md", "markdown"]);
    }

    #[test]
    fn test_load_from_toml_old_format() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
            [vault]
            notes_dir = "/tmp/notes"

            [indexing]
            snippet_lines = 5
        "#,
        )
        .unwrap();

        let config = ShiotsuchiConfig::load_from(&config_path).unwrap();
        let legacy = config.vault.as_ref().unwrap();
        assert_eq!(
            legacy.notes_dir.as_ref().unwrap().to_string_lossy(),
            "/tmp/notes"
        );
        assert_eq!(config.indexing.snippet_lines, 5);
    }

    #[test]
    fn test_load_from_toml_new_format() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
            [database]
            db_path = "/tmp/shiotsuchi.db"

            [vaults.work]
            notes_dir = "/work/notes"

            [indexing]
            snippet_lines = 10
        "#,
        )
        .unwrap();

        let config = ShiotsuchiConfig::load_from(&config_path).unwrap();
        assert!(config.vault.is_none());
        assert_eq!(
            config.database.db_path.as_ref().unwrap().to_string_lossy(),
            "/tmp/shiotsuchi.db"
        );
        assert_eq!(
            config.vaults.get("work").unwrap().notes_dir.as_ref().unwrap().to_string_lossy(),
            "/work/notes"
        );
        assert_eq!(config.indexing.snippet_lines, 10);
    }

    #[test]
    fn test_exclude_dirs_rejects_old_key() {
        let result =
            toml::from_str::<ShiotsuchiConfig>("[indexing]\nexclude_patterns = ['node_modules']");
        assert!(
            result.is_err(),
            "expected error for old key exclude_patterns"
        );
    }

    #[test]
    fn test_exclude_dirs_accepts_new_key() {
        let result =
            toml::from_str::<ShiotsuchiConfig>("[indexing]\nexclude_dirs = ['node_modules']");
        assert!(result.is_ok(), "expected ok for new key exclude_dirs");
        let config = result.unwrap();
        assert_eq!(config.indexing.exclude_dirs, vec!["node_modules"]);
    }

    #[test]
    fn test_max_snippet_chars_default_is_1000() {
        let config = ShiotsuchiConfig::default();
        assert_eq!(config.indexing.max_snippet_chars, 1000);
    }

    #[test]
    fn test_max_snippet_chars_from_toml() {
        let result = toml::from_str::<ShiotsuchiConfig>("[indexing]\nmax_snippet_chars = 2048");
        assert!(result.is_ok(), "expected ok for max_snippet_chars");
        assert_eq!(result.unwrap().indexing.max_snippet_chars, 2048);
    }

    #[test]
    fn test_max_snippet_chars_clamped_by_search_config() {
        let result = toml::from_str::<ShiotsuchiConfig>("[indexing]\nmax_snippet_chars = 65555");
        assert!(result.is_ok(), "oversized value should deserialize");
        assert_eq!(result.unwrap().indexing.max_snippet_chars, 65555);
    }

    #[test]
    fn test_resolved_vaults_empty_defaults_to_current_dir() {
        let config = ShiotsuchiConfig::default();
        let vaults = config.resolved_vaults();
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].0, "default");
        assert_eq!(vaults[0].1, PathBuf::from("."));
    }

    #[test]
    fn test_resolved_vaults_legacy_format() {
        let mut config = ShiotsuchiConfig::default();
        config.vault = Some(VaultEntry {
            notes_dir: Some(PathBuf::from("/tmp/notes")),
            db_path: None,
        });
        let vaults = config.resolved_vaults();
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].0, "default");
        assert_eq!(vaults[0].1, PathBuf::from("/tmp/notes"));
    }

    #[test]
    fn test_resolved_vaults_new_format() {
        let mut config = ShiotsuchiConfig::default();
        config.vaults.insert(
            "work".to_string(),
            VaultEntry {
                notes_dir: Some(PathBuf::from("/work/notes")),
                db_path: None,
            },
        );
        config.vaults.insert(
            "personal".to_string(),
            VaultEntry {
                notes_dir: Some(PathBuf::from("/personal/notes")),
                db_path: None,
            },
        );
        let vaults = config.resolved_vaults();
        assert_eq!(vaults.len(), 2);
        let names: Vec<&str> = vaults.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"work"));
        assert!(names.contains(&"personal"));
    }

    #[test]
    fn test_resolved_vaults_legacy_and_new_merged() {
        let mut config = ShiotsuchiConfig::default();
        config.vault = Some(VaultEntry {
            notes_dir: Some(PathBuf::from("/legacy/notes")),
            db_path: None,
        });
        config.vaults.insert(
            "work".to_string(),
            VaultEntry {
                notes_dir: Some(PathBuf::from("/work/notes")),
                db_path: None,
            },
        );
        let vaults = config.resolved_vaults();
        assert_eq!(vaults.len(), 2);
        assert_eq!(vaults[0].0, "default");
        assert_eq!(vaults[0].1, PathBuf::from("/legacy/notes"));
        assert_eq!(vaults[1].0, "work");
        assert_eq!(vaults[1].1, PathBuf::from("/work/notes"));
    }

    #[test]
    fn test_resolved_db_path_from_database() {
        let mut config = ShiotsuchiConfig::default();
        config.database.db_path = Some(PathBuf::from("/custom/db.sqlite"));
        assert_eq!(config.resolved_db_path(), PathBuf::from("/custom/db.sqlite"));
    }

    #[test]
    fn test_resolved_db_path_from_legacy_vault() {
        let mut config = ShiotsuchiConfig::default();
        config.vault = Some(VaultEntry {
            notes_dir: None,
            db_path: Some(PathBuf::from("/legacy/db.sqlite")),
        });
        assert_eq!(config.resolved_db_path(), PathBuf::from("/legacy/db.sqlite"));
    }

    #[test]
    fn test_resolved_db_path_database_overrides_legacy() {
        let mut config = ShiotsuchiConfig::default();
        config.database.db_path = Some(PathBuf::from("/new/db.sqlite"));
        config.vault = Some(VaultEntry {
            notes_dir: None,
            db_path: Some(PathBuf::from("/old/db.sqlite")),
        });
        assert_eq!(config.resolved_db_path(), PathBuf::from("/new/db.sqlite"));
    }

    #[test]
    fn test_resolved_db_path_default_fallback() {
        let config = ShiotsuchiConfig::default();
        assert_eq!(config.resolved_db_path(), core_default_db_path());
    }
}
