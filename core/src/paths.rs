use std::path::PathBuf;

/// Returns the default database path for shiotsuchi:
///
/// | Platform | Path |
/// |----------|------|
/// | macOS    | `~/Library/Application Support/shiotsuchi/db.sqlite3` |
/// | Linux    | `$XDG_DATA_HOME/shiotsuchi/db.sqlite3` or `~/.local/share/shiotsuchi/db.sqlite3` |
/// | Windows  | `{FOLDERID_LocalAppData}/shiotsuchi/db.sqlite3` |
///
/// Falls back to `./shiotsuchi/db.sqlite3` if the OS data directory cannot be determined.
pub fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("shiotsuchi")
        .join("db.sqlite3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_db_path_structure() {
        let path = default_db_path();
        assert_eq!(path.file_name().unwrap(), "db.sqlite3");
        assert_eq!(path.parent().unwrap().file_name().unwrap(), "shiotsuchi");
    }

    #[test]
    fn test_default_db_path_under_data_dir() {
        let path = default_db_path();
        let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        assert!(
            path.starts_with(&base),
            "default_db_path should be under data_dir, got: {}",
            path.display()
        );
    }

    #[test]
    fn test_default_db_path_creatable_parent() {
        let db_path = default_db_path();
        let parent = db_path.parent().expect("db_path should have a parent");
        assert!(!parent.as_os_str().is_empty());
    }
}
