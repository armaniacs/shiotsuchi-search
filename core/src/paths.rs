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
}
