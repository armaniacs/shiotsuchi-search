use serde::{Deserialize, Serialize};
use shiotsuchi_core::paths::default_db_path as core_default_db_path;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    pub notes_dir: PathBuf,
    pub db_path: PathBuf,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            notes_dir: PathBuf::from("."),
            db_path: core_default_db_path(),
        }
    }
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
    pub vault: VaultConfig,
    pub indexing: IndexingConfig,
    pub watcher: WatcherConfig,
}

impl ShiotsuchiConfig {
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
            Self::load_from(&default_path).unwrap_or_else(|e| {
                eprintln!(
                    "Warning: failed to load config from {}: {}. Using defaults.",
                    default_path.display(),
                    e
                );
                Self::default()
            })
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
        assert_eq!(config.indexing.include_extensions, vec!["md", "markdown"]);
    }

    #[test]
    fn test_load_from_toml() {
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
        assert_eq!(config.vault.notes_dir.to_string_lossy(), "/tmp/notes");
        assert_eq!(config.indexing.snippet_lines, 5);
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
        // The SearchConfig::new clamps values; verify CLI config integrates correctly
        let result = toml::from_str::<ShiotsuchiConfig>("[indexing]\nmax_snippet_chars = 65555");
        assert!(result.is_ok(), "oversized value should deserialize");
        // Actual clamping happens at SearchConfig construction time, not deserialization
        assert_eq!(result.unwrap().indexing.max_snippet_chars, 65555);
    }
}
