use clap::Args;
use obsidian_shiotsuchi_vault_core::db::NoteDatabase;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct DeleteArgs {
    #[arg(help = "Path to the note relative to vault root (e.g., meeting/notes.md)")]
    pub path: String,
}

pub fn run_delete(
    args: &DeleteArgs,
    notes_dir: &PathBuf,
    db_path: &PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let full_path = notes_dir.join(&args.path);
    let canonical = full_path.canonicalize()?;
    let vault_canonical = notes_dir.canonicalize()?;
    if !canonical.starts_with(&vault_canonical) {
        return Err("Path escapes vault directory".into());
    }

    let db = NoteDatabase::open(db_path)?;
    db.delete_note(&args.path)?;
    println!("Deleted note: {}", args.path);
    Ok(())
}
