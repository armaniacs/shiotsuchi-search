use clap::Args;
use shiotsuchi_core::db::NoteDatabase;
use std::path::{Path, PathBuf};

#[derive(Args, Debug)]
pub struct DeleteArgs {
    #[arg(help = "Path to the note relative to vault root (e.g., meeting/notes.md)")]
    pub path: String,
}

pub fn run_delete(
    args: &DeleteArgs,
    vaults: &[(String, PathBuf)],
    db_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = &args.path;
    // Reject absolute paths or paths with directory traversal components
    if Path::new(path).is_absolute() || path.split('/').any(|c| c == "..") {
        return Err("Invalid path: must be relative and within vault".into());
    }

    if vaults.is_empty() {
        return Err("No vaults configured. Nothing to delete.".into());
    }

    // Find which vault this file belongs to
    let vault_idx = vaults.iter().position(|(_, vault_dir)| {
        vault_dir.join(path).exists()
    });

    let (vault_name, vault_dir) = if let Some(idx) = vault_idx {
        &vaults[idx]
    } else {
        // File not found on disk — use first vault for DB cleanup
        &vaults[0]
    };

    // When the file still exists on disk, verify it resolves within the vault.
    // If the file has been deleted from disk (e.g. manual rm), skip the
    // canonicalize check and proceed with DB cleanup — the path was validated
    // against traversal above, and the relative path stored in the DB matches.
    let vault_canonical = vault_dir.canonicalize()?;
    let full_path = vault_dir.join(path);
    if full_path.exists() {
        let canonical = full_path.canonicalize()?;
        if !canonical.starts_with(&vault_canonical) {
            return Err("Path escapes vault directory".into());
        }
    }

    let db = NoteDatabase::open(db_path)?;
    db.delete_chunks_for_file(vault_name, path)?;
    db.delete_file_cache(vault_name, path)?;
    println!("Deleted: {}", path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ------------------------------------------------------------------
    // Path validation (early return, no DB needed)
    // ------------------------------------------------------------------

    #[test]
    fn test_rejects_absolute_path() {
        let tmp = TempDir::new().unwrap();
        let args = DeleteArgs {
            path: "/etc/passwd".to_string(),
        };
        let result = run_delete(&args, &[], &tmp.path().join("nonexistent.db"));
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Invalid"), "expected Invalid path error, got: {}", msg);
    }

    #[test]
    fn test_rejects_directory_traversal() {
        let tmp = TempDir::new().unwrap();
        let args = DeleteArgs {
            path: "../../secret.md".to_string(),
        };
        let result = run_delete(&args, &[], &tmp.path().join("nonexistent.db"));
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Invalid"), "expected Invalid path error, got: {}", msg);
    }

    #[test]
    fn test_rejects_traversal_in_middle_of_path() {
        let tmp = TempDir::new().unwrap();
        let args = DeleteArgs {
            path: "notes/../../etc/passwd".to_string(),
        };
        let result = run_delete(&args, &[], &tmp.path().join("nonexistent.db"));
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("Invalid"), "expected Invalid path error, got: {}", msg);
    }

    // ------------------------------------------------------------------
    // Vault resolution (needs DB)
    // ------------------------------------------------------------------

    /// Helper: run_delete with a single-vault setup containing `file.md`.
    /// Returns the error message on failure, or Ok(()) on success.
    fn try_delete(
        vault: &std::path::Path,
        db_path: &std::path::Path,
        path: &str,
    ) -> Result<(), String> {
        let args = DeleteArgs { path: path.to_string() };
        let vaults = vec![("default".to_string(), vault.to_path_buf())];
        match run_delete(&args, &vaults, db_path) {
            Ok(()) => Ok(()),
            Err(e) => Err(format!("{}", e)),
        }
    }

    #[test]
    fn test_accepts_valid_relative_path_within_vault() {
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("note.md"), "# Note").unwrap();

        let db_path = tmp.path().join("test.db");
        let _db = shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();

        // note.md exists on disk and is within vault — should succeed
        let result = try_delete(&vault, &db_path, "note.md");
        assert!(result.is_ok(), "valid path should be accepted: {:?}", result);
    }

    #[test]
    fn test_rejects_symlink_escape() {
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path().join("vault");
        let outside = tmp.path().join("outside");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let secret = outside.join("secret.md");
        std::fs::write(&secret, "secrets").unwrap();

        // Create a symlink inside vault pointing outside
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&outside, vault.join("escape")).unwrap();
        }
        #[cfg(not(unix))]
        {
            std::os::windows::fs::symlink_dir(&outside, vault.join("escape")).unwrap();
        }

        let db_path = tmp.path().join("test.db");
        let _db = shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();

        // "escape/secret.md" resolves to outside/vault — should be rejected
        let result = try_delete(&vault, &db_path, "escape/secret.md");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("escape") || msg.contains("traversal") || msg.contains("outside"),
            "expected escape rejection, got: {}",
            msg
        );
    }

    #[test]
    fn test_vault_resolution_fallback_when_file_missing_from_disk() {
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        // note.md does NOT exist on disk

        let db_path = tmp.path().join("test.db");
        let db = shiotsuchi_core::db::NoteDatabase::open(&db_path).unwrap();
        // Insert chunks manually so the DB has a record
        use shiotsuchi_core::chunker::split_into_chunks;
        let tok = match shiotsuchi_core::tokenizer::get_tokenizer() {
            Ok(t) => t,
            Err(_) => return, // skip when no model
        };
        let chunks = split_into_chunks("# Deleted note\n\nContent.", &tok, "note.md", "default");
        db.insert_chunks(&chunks).unwrap();
        drop(db);

        // File deleted from disk — delete should still succeed (DB cleanup)
        let result = try_delete(&vault, &db_path, "note.md");
        assert!(
            result.is_ok(),
            "delete should succeed even when file is missing from disk: {:?}",
            result
        );
    }

    #[test]
    fn test_empty_vaults_returns_error() {
        let tmp = TempDir::new().unwrap();
        let args = DeleteArgs {
            path: "note.md".to_string(),
        };
        let result = run_delete(&args, &[], &tmp.path().join("test.db"));
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("No vaults"),
            "expected 'No vaults' error, got: {}",
            msg
        );
    }
}
