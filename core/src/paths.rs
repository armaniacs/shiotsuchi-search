use std::env;
use std::path::PathBuf;

/// Returns the XDG cache home directory, falling back to `~/.cache`.
fn xdg_cache_home() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".cache"))
}

/// Returns the user's home directory, falling back to current directory.
fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Returns the default database path for shiotsuchi:
/// `$XDG_CACHE_HOME/shiotsuchi/db.sqlite3` or `~/.cache/shiotsuchi/db.sqlite3`.
pub fn default_db_path() -> PathBuf {
    xdg_cache_home().join("shiotsuchi").join("db.sqlite3")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_db_path_structure() {
        let path = default_db_path();
        // Check it ends with "shiotsuchi/db.sqlite3"
        assert_eq!(path.file_name().unwrap(), "db.sqlite3");
        assert_eq!(path.parent().unwrap().file_name().unwrap(), "shiotsuchi");
    }

    #[test]
    fn test_default_db_path_respects_xdg() {
        // Save original value
        let original = env::var("XDG_CACHE_HOME").ok();
        // Set temporary XDG_CACHE_HOME
        unsafe {
            env::set_var("XDG_CACHE_HOME", "/tmp/xyz_cache");
        }
        let path = default_db_path();
        assert!(path.starts_with("/tmp/xyz_cache"));
        // Restore
        match original {
            Some(val) => unsafe {
                env::set_var("XDG_CACHE_HOME", val);
            },
            None => unsafe {
                env::remove_var("XDG_CACHE_HOME");
            },
        }
    }

    #[test]
    fn test_default_db_path_contains_cache_dir() {
        let path = default_db_path();
        let path_str = path.to_string_lossy();
        // Should contain either .cache or a named XDG cache directory
        assert!(path_str.contains("cache") || path_str.contains("Cache"),
            "default_db_path should include a cache directory, got: {}", path_str);
    }

    #[test]
    fn test_xdg_cache_home_returns_valid_path() {
        let path = xdg_cache_home();
        assert!(!path.as_os_str().is_empty(), "xdg_cache_home should return a non-empty path");
    }

    #[test]
    fn test_home_dir_returns_some_path() {
        let home = home_dir();
        assert!(!home.as_os_str().is_empty(), "home_dir should return a non-empty path");
    }

    #[test]
    fn test_default_db_path_creatable_parent() {
        // Verify the parent directory structure is plausible
        let db_path = default_db_path();
        let parent = db_path.parent().expect("db_path should have a parent");
        // The parent path should not be empty
        assert!(!parent.as_os_str().is_empty());
    }
}
