use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultConfig {
    pub notes_dir: PathBuf,
    pub db_path: PathBuf,
}

impl Default for VaultConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_default();
        Self {
            notes_dir: PathBuf::from("."),
            db_path: home.join(".shiotsuchi").join("db.sqlite3"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexingConfig {
    pub snippet_lines: usize,
    pub include_extensions: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

impl Default for IndexingConfig {
    fn default() -> Self {
        Self {
            snippet_lines: 3,
            include_extensions: vec!["md".to_string(), "markdown".to_string()],
            exclude_patterns: vec![
                ".obsidian".to_string(),
                ".git".to_string(),
                "node_modules".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WatcherConfig {
    pub debounce_ms: u64,
    pub enabled: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 500,
            enabled: true,
        }
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

    pub fn load() -> Self {
        let default_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".shiotsuchi")
            .join("config.toml");
        if default_path.exists() {
            Self::load_from(&default_path).unwrap_or_default()
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
        assert_eq!(config.watcher.debounce_ms, 500);
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
}
