pub use shiotsuchi_core::config::{
    default_config_path, DatabaseConfig, IndexingConfig, ShiotsuchiConfig, VaultEntry,
    WatcherConfig,
};

#[cfg(test)]
mod tests {
    use super::*;
    use shiotsuchi_core::paths::default_db_path as core_default_db_path;
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
