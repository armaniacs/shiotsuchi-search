use std::path::PathBuf;

/// Resolve a vault name to its canonicalized directory path.
///
/// Returns `Err(msg)` if the vault is not found or the directory cannot be
/// canonicalized (e.g., doesn't exist, permission denied). This function is
/// used by both MCP and HTTP handlers for centralized path traversal protection.
pub fn resolve_vault_dir(
    vaults: &[(String, PathBuf)],
    vault_name: &str,
) -> Result<PathBuf, String> {
    let (_name, dir) = vaults
        .iter()
        .find(|(name, _)| name == vault_name)
        .ok_or_else(|| format!("vault '{}' not found", vault_name))?;
    dir.canonicalize()
        .map_err(|e| format!("vault '{}' directory is not accessible: {}", vault_name, e))
}

/// Validate that a file path stays within the given vault and return its canonicalized
/// absolute path. Used as a path traversal guard when reading files from disk.
///
/// Returns `Err(msg)` if the vault is not found, the file does not exist, or the
/// file escapes the vault directory (e.g., via symlink or `..` traversal).
pub fn resolve_file_in_vault(
    vaults: &[(String, PathBuf)],
    vault_name: &str,
    file_path: &str,
) -> Result<PathBuf, String> {
    let canonical_vault = resolve_vault_dir(vaults, vault_name)?;
    let full_path = canonical_vault.join(file_path);
    let canonical_file = full_path
        .canonicalize()
        .map_err(|e| format!("file '{}' is not accessible: {}", file_path, e))?;
    if !canonical_file.starts_with(&canonical_vault) {
        return Err(format!(
            "file '{}' escapes vault '{}' directory (path traversal rejected)",
            file_path, vault_name
        ));
    }
    Ok(canonical_file)
}
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
