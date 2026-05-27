pub use shiotsuchi_core::config::{
    default_config_path, thesaurus_path, DatabaseConfig, IndexingConfig,
    ShiotsuchiConfig, VaultEntry, WatcherConfig,
};

#[cfg(test)]
mod tests {
    use super::*;
    use shiotsuchi_core::paths::default_db_path as core_default_db_path;
    use std::collections::HashMap;
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = ShiotsuchiConfig::default();
        assert!(config.vaults.is_empty());
        assert!(config.vault.is_none());
        assert!(config.database.db_path.is_none());
        assert_eq!(config.indexing.include_extensions, vec!["md", "markdown"]);
        assert_eq!(config.hybrid_alpha, None);
    }

    #[test]
    fn test_hybrid_alpha_from_toml() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "hybrid_alpha = 0.8\n").unwrap();

        let config = ShiotsuchiConfig::load_from(&config_path).unwrap();
        assert_eq!(config.hybrid_alpha, Some(0.8));
    }

    #[test]
    fn test_hybrid_alpha_omitted_from_toml_is_none() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "[indexing]\nexclude_dirs = []\n").unwrap();

        let config = ShiotsuchiConfig::load_from(&config_path).unwrap();
        assert_eq!(config.hybrid_alpha, None);
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
            exclude_dirs = ["node_modules"]
        "#,
        )
        .unwrap();

        let config = ShiotsuchiConfig::load_from(&config_path).unwrap();
        let legacy = config.vault.as_ref().unwrap();
        assert_eq!(
            legacy.notes_dir.as_ref().unwrap().to_string_lossy(),
            "/tmp/notes"
        );
        assert_eq!(config.indexing.exclude_dirs, vec!["node_modules"]);
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
            exclude_dirs = ["node_modules"]
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
        assert_eq!(config.indexing.exclude_dirs, vec!["node_modules"]);
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
    fn test_resolved_vaults_empty_defaults_to_current_dir() {
        let config = ShiotsuchiConfig::default();
        let vaults = config.resolved_vaults();
        assert_eq!(vaults.len(), 1);
        assert_eq!(vaults[0].0, "default");
        assert_eq!(vaults[0].1, PathBuf::from("."));
    }

    #[test]
    fn test_resolved_vaults_legacy_format() {
        let config = ShiotsuchiConfig {
            vault: Some(VaultEntry {
                notes_dir: Some(PathBuf::from("/tmp/notes")),
                db_path: None,
            }),
            ..Default::default()
        };
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
        let mut named_vaults = std::collections::HashMap::new();
        named_vaults.insert(
            "work".to_string(),
            VaultEntry {
                notes_dir: Some(PathBuf::from("/work/notes")),
                ..Default::default()
            },
        );
        let config = ShiotsuchiConfig {
            vault: Some(VaultEntry {
                notes_dir: Some(PathBuf::from("/legacy/notes")),
                ..Default::default()
            }),
            vaults: named_vaults,
            database: DatabaseConfig::default(),
            indexing: IndexingConfig::default(),
            watcher: WatcherConfig::default(),
            synonyms: HashMap::new(),
            hybrid_alpha: None,
            vault_default: None,
            semantic_threshold: None,
            embedder: EmbedderConfig::default(),
        };
        let vaults = config.resolved_vaults();
        assert_eq!(vaults.len(), 2);
        assert_eq!(vaults[0].0, "default");
        assert_eq!(vaults[0].1, PathBuf::from("/legacy/notes"));
        assert_eq!(vaults[1].0, "work");
        assert_eq!(vaults[1].1, PathBuf::from("/work/notes"));
    }

    #[test]
    fn test_resolved_db_path_from_database() {
        let config = ShiotsuchiConfig {
            database: DatabaseConfig {
                db_path: Some(PathBuf::from("/custom/db.sqlite")),
            },
            vaults: HashMap::new(),
            vault: None,
            indexing: IndexingConfig::default(),
            watcher: WatcherConfig::default(),
            synonyms: HashMap::new(),
            hybrid_alpha: None,
            vault_default: None,
            semantic_threshold: None,
            embedder: EmbedderConfig::default(),
        };
        assert_eq!(config.resolved_db_path(), PathBuf::from("/custom/db.sqlite"));
    }

    #[test]
    fn test_resolved_db_path_from_legacy_vault() {
        let config = ShiotsuchiConfig {
            vault: Some(VaultEntry {
                notes_dir: None,
                db_path: Some(PathBuf::from("/legacy/db.sqlite")),
            }),
            database: DatabaseConfig::default(),
            vaults: HashMap::new(),
            indexing: IndexingConfig::default(),
            watcher: WatcherConfig::default(),
            synonyms: HashMap::new(),
            hybrid_alpha: None,
            vault_default: None,
            semantic_threshold: None,
            embedder: EmbedderConfig::default(),
        };
        assert_eq!(config.resolved_db_path(), PathBuf::from("/legacy/db.sqlite"));
    }

    #[test]
    fn test_resolved_db_path_database_overrides_legacy() {
        let config = ShiotsuchiConfig {
            database: DatabaseConfig {
                db_path: Some(PathBuf::from("/new/db.sqlite")),
            },
            vault: Some(VaultEntry {
                notes_dir: None,
                db_path: Some(PathBuf::from("/old/db.sqlite")),
            }),
            vaults: HashMap::new(),
            indexing: IndexingConfig::default(),
            watcher: WatcherConfig::default(),
            synonyms: HashMap::new(),
            hybrid_alpha: None,
            vault_default: None,
            semantic_threshold: None,
            embedder: EmbedderConfig::default(),
        };
        assert_eq!(config.resolved_db_path(), PathBuf::from("/new/db.sqlite"));
    }

    #[test]
    fn test_resolved_db_path_default_fallback() {
        let config = ShiotsuchiConfig::default();
        assert_eq!(config.resolved_db_path(), core_default_db_path());
    }
}
