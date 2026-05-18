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
