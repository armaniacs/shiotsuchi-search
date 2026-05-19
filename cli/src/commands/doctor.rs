use clap::Args;
use std::path::Path;

#[derive(Args, Debug)]
pub struct DoctorArgs {}

pub fn run_doctor(db_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut all_ok = true;

    // 1. Config
    let config_path = dirs::config_dir().map(|p| p.join("shiotsuchi").join("config.toml"));
    if let Some(ref p) = config_path {
        if p.exists() {
            println!("[ok] Config: {}", p.display());
        } else {
            println!("[..] Config: {} (not found — using defaults)", p.display());
        }
    }

    // 2. Database
    if db_path.exists() {
        match shiotsuchi_core::db::NoteDatabase::open(db_path) {
            Ok(db) => {
                match db.stats() {
                    Ok(stats) => println!("[ok] Database: {} ({} files, {} chunks)", db_path.display(), stats.total_files, stats.total_chunks),
                    Err(e) => {
                        println!("[!!] Database: {} (open ok but stats failed: {})", db_path.display(), e);
                        all_ok = false;
                    }
                }
            }
            Err(e) => {
                println!("[!!] Database: {} (open failed: {})", db_path.display(), e);
                all_ok = false;
            }
        }
    } else {
        println!("[..] Database: {} (not found — run `shiotsuchi chart`)", db_path.display());
    }

    // 3. Vaporetto model
    match shiotsuchi_core::tokenizer::get_tokenizer() {
        Ok(_) => println!("[ok] Tokenizer: Vaporetto model loaded"),
        Err(e) => println!("[..] Tokenizer: {} (FTS fallback)", e),
    }

    // 4. Embedder model
    match shiotsuchi_core::embedder::resolve_model_path(None) {
        Some(p) => match shiotsuchi_core::embedder::Embedder::load(&p) {
            Ok(_) => println!("[ok] Embedder: ONNX model loaded"),
            Err(e) => println!("[..] Embedder: model found but load failed: {}", e),
        },
        None => println!("[..] Embedder: ONNX model not found (vector search disabled)"),
    }

    // 5. Vault configuration
    let vaults = crate::config::ShiotsuchiConfig::load().vaults;
    if vaults.is_empty() {
        println!("[..] Vaults: none configured");
    } else {
        for (name, entry) in &vaults {
            if let Some(ref dir) = entry.notes_dir {
                let status = if dir.exists() { "[ok]" } else { "[!!]" };
                println!("{} Vault '{}': {}", status, name, dir.display());
                if !dir.exists() { all_ok = false; }
            }
        }
    }

    if all_ok {
        println!("\nAll checks passed.");
    } else {
        println!("\nSome checks failed. See messages above.");
    }

    Ok(())
}
