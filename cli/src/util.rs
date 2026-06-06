use std::path::Path;

/// Select the dialoguer theme based on the NO_COLOR environment variable.
/// Respects https://no-color.org/ — when set, use SimpleTheme (no ANSI colors).
pub fn dialoguer_theme() -> Box<dyn dialoguer::theme::Theme> {
    if std::env::var("NO_COLOR").is_ok() {
        Box::new(dialoguer::theme::SimpleTheme)
    } else {
        Box::new(dialoguer::theme::ColorfulTheme::default())
    }
}

/// Set the parent directory of `path` to `0o700` permissions on Unix.
///
/// Creates the parent if it doesn't exist. Safe to call on any platform:
/// the operation is a no-op on non-Unix targets. Errors are logged via
/// `tracing::warn!` but do not abort execution (best-effort security).
///
/// Currently only called from tests; kept for future use in new commands
/// that create database directories.
#[allow(dead_code)]
pub fn secure_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(parent) {
                if meta.permissions().mode() & 0o777 != 0o700 {
                    if let Err(e) =
                        std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                    {
                        tracing::warn!("Failed to set parent directory permissions to 0o700: {}", e);
                    }
                }
            }
            #[cfg(not(unix))]
            {
                tracing::warn!("Directory permissions not restricted — not supported on this platform.");
            }
        }
        let _ = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    #[cfg(unix)]
    fn test_secure_parent_dir_creates_with_0700() {
        let temp = TempDir::new().unwrap();
        let nested = temp.path().join("a").join("b").join("test.db");
        std::fs::create_dir_all(nested.parent().unwrap()).unwrap();

        secure_parent_dir(&nested);

        let parent = nested.parent().unwrap();
        let mode = std::fs::metadata(parent).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    #[cfg(unix)]
    fn test_secure_parent_dir_preserves_existing_0700() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("subdir");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();

        let path = dir.join("test.db");
        secure_parent_dir(&path);

        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn test_secure_parent_dir_handles_nonexistent_parent() {
        let temp = TempDir::new().unwrap();
        let nested = temp.path().join("nonexistent").join("test.db");

        // Should not panic
        secure_parent_dir(&nested);
    }

    #[test]
    fn test_secure_parent_dir_noop_without_parent() {
        // Path with no parent component (e.g., just "test.db")
        secure_parent_dir(Path::new("test.db"));
        // Should not panic
    }
}
