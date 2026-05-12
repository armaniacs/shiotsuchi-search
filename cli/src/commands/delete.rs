use clap::Args;
use shiotsuchi_core::db::NoteDatabase;
use std::path::Path;

#[derive(Args, Debug)]
pub struct DeleteArgs {
    #[arg(help = "Path to the note relative to vault root (e.g., meeting/notes.md)")]
    pub path: String,
}

pub fn run_delete(
    args: &DeleteArgs,
    notes_dir: &Path,
    db_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = &args.path;
    // Reject absolute paths or paths with directory traversal components
    if Path::new(path).is_absolute() || path.split('/').any(|c| c == "..") {
        return Err("Invalid path: must be relative and within vault".into());
    }

    // When the file still exists on disk, verify it resolves within the vault.
    // If the file has been deleted from disk (e.g. manual rm), skip the
    // canonicalize check and proceed with DB cleanup — the path was validated
    // against traversal above, and the relative path stored in the DB matches.
    let vault_canonical = notes_dir.canonicalize()?;
    let full_path = notes_dir.join(path);
    if full_path.exists() {
        let canonical = full_path.canonicalize()?;
        if !canonical.starts_with(&vault_canonical) {
            return Err("Path escapes vault directory".into());
        }
    }

    let db = NoteDatabase::open(db_path)?;
    db.delete_chunks_for_file(&args.path)?;
    db.delete_file_cache(&args.path)?;
    println!("Deleted: {}", args.path);
    Ok(())
}
