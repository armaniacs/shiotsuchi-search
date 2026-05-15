use clap::Args;
use shiotsuchi_core::{
    constants::EXPECTED_MODEL_SHA256,
    embedder::{resolve_model_path, verify_model_hash},
};
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Check current setup status without making changes.
    #[arg(long)]
    pub check: bool,
}

fn default_model_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| {
                    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                })
                .join(".local")
                .join("share")
        })
        .join("shiotsuchi")
}

pub fn run_setup(args: &SetupArgs) -> Result<(), Box<dyn std::error::Error>> {
    let model_dir = default_model_dir();
    let model_path = model_dir.join("model.onnx");

    if let Some(found) = resolve_model_path(None) {
        let found_metadata = std::fs::metadata(&found)?;
        let size_mb = found_metadata.len() as f64 / 1_048_576.0;
        println!("Embedder model found: {}", found.display());
        println!("  Size: {:.1} MB", size_mb);

        if !EXPECTED_MODEL_SHA256.is_empty() {
            match verify_model_hash(&found) {
                Ok(true) => println!("  Checksum: OK (SHA-256 matches expected value)"),
                Ok(false) => {
                    eprintln!(
                        "  Checksum: MISMATCH — the model file may be corrupted or from a different source."
                    );
                    eprintln!("  Expected SHA-256: {}", EXPECTED_MODEL_SHA256);
                }
                Err(e) => {
                    eprintln!("  Checksum: error computing hash: {}", e);
                }
            }
        } else {
            println!("  Checksum: skipped (no expected hash configured)");
        }

        println!("Semantic search is available.");
        return Ok(());
    }

    if args.check {
        println!("Embedder model not found.");
        println!("Expected location: {}", model_path.display());
        println!("(Also checks the SHIOTSUCHI_EMBED_MODEL_PATH environment variable.");
        println!(" Run `shiotsuchi setup` without --check for full setup instructions.)");
        return Ok(());
    }

    // Print setup instructions
    println!("Shiotsuchi Setup — Semantic Search Model");
    println!("{}", "─".repeat(50));
    println!();
    println!("To enable vector and hybrid search, place an ONNX embedding model at:");
    println!("  {}", model_path.display());
    println!();
    println!("Steps:");
    println!(
        "  1. Create the directory:\n     mkdir -p {}",
        model_dir.display()
    );
    println!();
    println!("  2. Download a compatible ONNX embedding model (e.g. Qwen3-Embedding-0.6B)");
    println!("     and save it as:");
    println!("       {}", model_path.display());
    println!("     If the model uses external data (model.onnx_data), copy it too:");
    println!("       {}", model_dir.join("model.onnx_data").display());
    if !EXPECTED_MODEL_SHA256.is_empty() {
        println!();
        println!("     Expected SHA-256: {}", EXPECTED_MODEL_SHA256);
    }
    println!();
    println!("  3. Verify the setup:");
    println!("     shiotsuchi setup --check");
    println!();
    println!(
        "Alternatively, set the SHIOTSUCHI_EMBED_MODEL_PATH environment variable \
         to point to your model file."
    );

    // Create the directory as a convenience
    if !model_dir.exists() {
        std::fs::create_dir_all(&model_dir)?;
        println!();
        println!("Created directory: {}", model_dir.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_check_no_model() {
        // With no model present (typical CI env), --check should succeed without error
        let args = SetupArgs { check: true };
        let result = run_setup(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_setup_prints_instructions() {
        // Without --check, should print instructions and succeed
        let args = SetupArgs { check: false };
        let result = run_setup(&args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_model_dir_is_under_home() {
        let dir = default_model_dir();
        // Should end with shiotsuchi/
        assert!(dir.ends_with("shiotsuchi"));
    }
}
